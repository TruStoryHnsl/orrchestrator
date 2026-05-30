//! Per-session file-ownership registry.
//!
//! Each file's content enters the LLM context window exactly once per workflow
//! session. The agent that first reads a file OWNS it; later agents route edits
//! through the owner via [`FileRegistry::request_edit`].
//!
//! See `plans/specs/file-registry.md` for the full design spec.
//! PM resolutions: T2-q1 (inline edit-handle), T2-q2 (re-read disk on
//! inheritance) — see `plans/specs/pm-decisions-2026-05-14.md`.
//!
//! Multi-machine workflows are out of scope for v1; the registry is
//! per-machine, per-workflow.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CRASH_GRACE: Duration = Duration::from_secs(300); // 5 min
pub const DEFAULT_REGISTRY_PATH: &str = ".orrch/file_registry.json";
pub const DEFAULT_AUDIT_LOG: &str = ".orrch/file_registry_audit.log";

/// Soft-warning override env var. When `1`, double-acquire emits a logged
/// warning AND still returns `AlreadyOwned` — the spec is "soft warning, not
/// soft permission".
pub const SOFT_DOUBLE_READ_ENV: &str = "ORRCH_REGISTRY_SOFT_DOUBLE_READ";

/// Unique within one workflow session. Format: `<role>:<wave>:<cluster>:<seq>`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Extract the role component (`<role>` of `<role>:<wave>:<cluster>:<seq>`)
    /// for same-wave tiebreak priority. Returns the whole string if no `:` is
    /// found.
    pub fn role(&self) -> &str {
        self.0.split(':').next().unwrap_or(&self.0)
    }
}

/// Lower number = wins ties. See spec §6.
fn role_priority(role: &str) -> u32 {
    match role {
        "PM" | "ProjectManager" | "Project_Manager" => 1,
        "Software_Engineer" | "SoftwareEngineer" | "SwE" => 2,
        "Developer" | "Dev" => 3,
        "Researcher" => 4,
        "UI_Designer" | "UIDesigner" => 5,
        "Tester" | "Feature_Tester" | "FeatureTester" => 6,
        _ => 99,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ownership {
    pub path: PathBuf,
    pub owner: AgentId,
    pub acquired_at: SystemTime,
    pub last_touch: SystemTime,
    pub byte_size: u64,
    pub content_sha256: String,
    pub edit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeSpec {
    Patch { unified_diff: String },
    Replace { old: String, new: String },
    Natural { directive: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EditStatus {
    Pending,
    Accepted,
    Rejected { reason: String },
    Applied { commit_sha: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditHandle {
    pub id: String,
    pub path: PathBuf,
    pub requester: AgentId,
    pub owner: AgentId,
    pub change_spec: ChangeSpec,
    pub posted_at: SystemTime,
    pub status: EditStatus,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("file already owned by {current_owner:?}: {path:?}")]
    AlreadyOwned {
        path: PathBuf,
        current_owner: AgentId,
    },
    #[error("file not owned: {path:?}")]
    NotOwned { path: PathBuf },
    #[error("file {path:?} is owned by {current_owner:?}, not {requester:?}")]
    NotOwnedByRequester {
        path: PathBuf,
        current_owner: AgentId,
        requester: AgentId,
    },
    #[error("double-read attempt by {attempting_agent:?} on {path:?} owned by {current_owner:?}")]
    DoubleReadAttempt {
        path: PathBuf,
        attempting_agent: AgentId,
        current_owner: AgentId,
    },
    #[error("edit handle not found: {0}")]
    EditNotFound(String),
    #[error("path outside project root: {0:?}")]
    PathEscape(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

// ----------------- Clock injection ------------------

/// Clock abstraction so tests can advance time without sleeping.
pub trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> SystemTime;
}

#[derive(Debug, Clone, Copy)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Test clock: monotonically advances; each `now()` call yields a unique
/// timestamp (advance by 1µs) so FIFO ordering is reproducible.
#[derive(Debug, Clone)]
pub struct ManualClock {
    inner: Arc<Mutex<SystemTime>>,
    autotick: Duration,
}

impl ManualClock {
    pub fn new(start: SystemTime) -> Self {
        Self {
            inner: Arc::new(Mutex::new(start)),
            autotick: Duration::from_micros(1),
        }
    }

    /// Auto-advance per `now()` call (default 1µs). Set to zero to freeze.
    pub fn with_autotick(mut self, autotick: Duration) -> Self {
        self.autotick = autotick;
        self
    }

    pub fn advance(&self, d: Duration) {
        let mut g = self.inner.lock().unwrap();
        *g += d;
    }

    pub fn set(&self, t: SystemTime) {
        let mut g = self.inner.lock().unwrap();
        *g = t;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> SystemTime {
        let mut g = self.inner.lock().unwrap();
        let cur = *g;
        *g += self.autotick;
        cur
    }
}

// ----------------- Persistence layer ------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistrySnapshot {
    schema_version: u32,
    owners: HashMap<String, Ownership>, // string-keyed for JSON
    pending_edits: Vec<EditHandle>,
    /// Map of crashed agent_id (String form) -> crashed_at SystemTime.
    crashed_owners: HashMap<String, SystemTime>,
}

impl Default for RegistrySnapshot {
    fn default() -> Self {
        Self {
            schema_version: 1,
            owners: HashMap::new(),
            pending_edits: Vec::new(),
            crashed_owners: HashMap::new(),
        }
    }
}

// ----------------- Registry ------------------

pub struct FileRegistry {
    project_root: PathBuf,
    state_path: PathBuf,
    audit_path: PathBuf,
    owners: HashMap<PathBuf, Ownership>,
    pending_edits: Vec<EditHandle>,
    crashed_owners: HashMap<AgentId, SystemTime>,
    clock: Arc<dyn Clock>,
    /// When true, a double-read conflict logs a SOFT warning instead of a hard
    /// block (the conflict is still returned as an error either way). Captured
    /// once at construction from `SOFT_DOUBLE_READ_ENV` so behavior is a stable
    /// per-instance property rather than a process-global env read on every
    /// `acquire` — the latter races across concurrently-running registries
    /// (e.g. parallel tests) that mutate the shared env.
    soft_double_read: bool,
    /// Monotonic counter for UUID-like edit-handle IDs without an external dep.
    seq: u64,
}

impl std::fmt::Debug for FileRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileRegistry")
            .field("project_root", &self.project_root)
            .field("state_path", &self.state_path)
            .field("owners", &self.owners)
            .field("pending_edits", &self.pending_edits)
            .field("crashed_owners", &self.crashed_owners)
            .finish()
    }
}

impl FileRegistry {
    /// Construct with the system clock.
    pub fn load_or_init(project_root: impl AsRef<Path>) -> Result<Self, RegistryError> {
        Self::load_or_init_with_clock(project_root, Arc::new(SystemClock))
    }

    pub fn load_or_init_with_clock(
        project_root: impl AsRef<Path>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, RegistryError> {
        let project_root = project_root.as_ref().to_path_buf();
        let orrch_dir = project_root.join(".orrch");
        fs::create_dir_all(&orrch_dir)?;
        let state_path = project_root.join(DEFAULT_REGISTRY_PATH);
        let audit_path = project_root.join(DEFAULT_AUDIT_LOG);

        let snapshot = match fs::read(&state_path) {
            Ok(bytes) if !bytes.is_empty() => match serde_json::from_slice::<RegistrySnapshot>(&bytes) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        "file_registry: malformed state at {} ({}); starting fresh",
                        state_path.display(),
                        e
                    );
                    RegistrySnapshot::default()
                }
            },
            Ok(_) => RegistrySnapshot::default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => RegistrySnapshot::default(),
            Err(e) => return Err(e.into()),
        };

        let owners: HashMap<PathBuf, Ownership> = snapshot
            .owners
            .into_iter()
            .map(|(k, v)| (PathBuf::from(k), v))
            .collect();
        let crashed_owners: HashMap<AgentId, SystemTime> = snapshot
            .crashed_owners
            .into_iter()
            .map(|(k, v)| (AgentId(k), v))
            .collect();

        let me = Self {
            project_root,
            state_path,
            audit_path,
            owners,
            pending_edits: snapshot.pending_edits,
            crashed_owners,
            clock,
            soft_double_read: std::env::var(SOFT_DOUBLE_READ_ENV).ok().as_deref() == Some("1"),
            seq: 0,
        };
        Ok(me)
    }

    /// Override the soft-double-read policy for this registry instance.
    ///
    /// The default is read once from `SOFT_DOUBLE_READ_ENV` at construction;
    /// callers (and tests) use this to set the policy explicitly without
    /// mutating the process-global environment.
    pub fn set_soft_double_read(&mut self, enabled: bool) {
        self.soft_double_read = enabled;
    }

    fn save(&self) -> Result<(), RegistryError> {
        let snapshot = RegistrySnapshot {
            schema_version: 1,
            owners: self
                .owners
                .iter()
                .map(|(k, v)| (k.to_string_lossy().into_owned(), v.clone()))
                .collect(),
            pending_edits: self.pending_edits.clone(),
            crashed_owners: self
                .crashed_owners
                .iter()
                .map(|(k, v)| (k.0.clone(), *v))
                .collect(),
        };
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.state_path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&snapshot)?;
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, &self.state_path)?;
        Ok(())
    }

    fn audit(&self, verdict: &str, attempting_agent: &AgentId, path: &Path, current_owner: Option<&AgentId>) {
        let ts = self
            .clock
            .now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros())
            .unwrap_or(0);
        let line = serde_json::json!({
            "timestamp_us": ts,
            "verdict": verdict,
            "attempting_agent": attempting_agent.0,
            "path": path.to_string_lossy(),
            "current_owner": current_owner.map(|a| a.0.clone()),
        });
        let s = format!("{}\n", line);
        if let Some(parent) = self.audit_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.audit_path)
        {
            let _ = f.write_all(s.as_bytes());
        }
    }

    /// Canonicalize a path relative to the project root. For not-yet-existing
    /// files, normalize (no I/O) and ensure it stays within project_root.
    fn canonicalize_path(&self, path: &Path) -> Result<PathBuf, RegistryError> {
        let abs: PathBuf = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.project_root.join(path)
        };
        // First normalize by component to defeat `..` escapes.
        let mut out = PathBuf::new();
        for c in abs.components() {
            match c {
                std::path::Component::ParentDir => {
                    if !out.pop() {
                        return Err(RegistryError::PathEscape(path.to_path_buf()));
                    }
                }
                std::path::Component::CurDir => {}
                other => out.push(other.as_os_str()),
            }
        }
        // Try fs::canonicalize for existing files (resolves symlinks).
        if let Ok(c) = fs::canonicalize(&out) {
            out = c;
        }
        // Verify it lives under project_root (canonicalize project_root too if exists).
        let root = fs::canonicalize(&self.project_root).unwrap_or_else(|_| self.project_root.clone());
        if !out.starts_with(&root) {
            return Err(RegistryError::PathEscape(path.to_path_buf()));
        }
        Ok(out)
    }

    fn file_meta(&self, path: &Path) -> (u64, String) {
        match fs::read(path) {
            Ok(bytes) => {
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                (bytes.len() as u64, format!("{:x}", hasher.finalize()))
            }
            Err(_) => (0, String::new()),
        }
    }

    fn next_id(&mut self) -> String {
        self.seq = self.seq.wrapping_add(1);
        let ts = self
            .clock
            .now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        // Hash to produce a UUID-shaped opaque ID.
        let mut h = Sha256::new();
        h.update(format!("{}-{}", ts, self.seq).as_bytes());
        let hex = format!("{:x}", h.finalize());
        format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }

    /// Claim a file. Returns AlreadyOwned if owned by someone else.
    pub fn acquire(&mut self, path: &Path, agent: &AgentId) -> Result<AgentId, RegistryError> {
        let cpath = self.canonicalize_path(path)?;
        let now = self.clock.now();

        if let Some(existing) = self.owners.get(&cpath) {
            if &existing.owner == agent {
                // re-acquire by same owner: refresh last_touch.
                let mut owned = existing.clone();
                owned.last_touch = now;
                self.owners.insert(cpath.clone(), owned);
                self.save()?;
                self.audit("REACQUIRE", agent, &cpath, Some(agent));
                return Ok(agent.clone());
            }
            // Conflict.
            let current = existing.owner.clone();
            let soft = self.soft_double_read;
            if soft {
                tracing::warn!(
                    "file_registry: SOFT double-read attempt path={} attempting={} owner={}",
                    cpath.display(),
                    agent.0,
                    current.0
                );
                self.audit("DOUBLE_READ_SOFT", agent, &cpath, Some(&current));
            } else {
                self.audit("DOUBLE_READ_BLOCKED", agent, &cpath, Some(&current));
            }
            return Err(RegistryError::AlreadyOwned {
                path: cpath,
                current_owner: current,
            });
        }

        let (byte_size, content_sha256) = self.file_meta(&cpath);
        let ownership = Ownership {
            path: cpath.clone(),
            owner: agent.clone(),
            acquired_at: now,
            last_touch: now,
            byte_size,
            content_sha256,
            edit_count: 0,
        };
        self.owners.insert(cpath.clone(), ownership);
        self.save()?;
        self.audit("ACQUIRE", agent, &cpath, Some(agent));
        Ok(agent.clone())
    }

    /// Same as acquire but does NOT error if already owned — returns the
    /// existing owner (or `agent` if newly acquired).
    pub fn try_acquire(&mut self, path: &Path, agent: &AgentId) -> Result<AgentId, RegistryError> {
        let cpath = self.canonicalize_path(path)?;
        if let Some(existing) = self.owners.get(&cpath) {
            return Ok(existing.owner.clone());
        }
        self.acquire(path, agent)
    }

    pub fn release(&mut self, path: &Path, agent: &AgentId) -> Result<(), RegistryError> {
        let cpath = self.canonicalize_path(path)?;
        match self.owners.get(&cpath) {
            None => Err(RegistryError::NotOwned { path: cpath }),
            Some(o) if &o.owner != agent => Err(RegistryError::NotOwnedByRequester {
                path: cpath.clone(),
                current_owner: o.owner.clone(),
                requester: agent.clone(),
            }),
            Some(_) => {
                self.owners.remove(&cpath);
                self.save()?;
                self.audit("RELEASE", agent, &cpath, None);
                Ok(())
            }
        }
    }

    pub fn release_all(&mut self, agent: &AgentId) -> Result<Vec<PathBuf>, RegistryError> {
        let mut released = Vec::new();
        let paths: Vec<PathBuf> = self
            .owners
            .iter()
            .filter(|(_, o)| &o.owner == agent)
            .map(|(p, _)| p.clone())
            .collect();
        for p in paths {
            self.owners.remove(&p);
            released.push(p.clone());
            self.audit("RELEASE_ALL", agent, &p, None);
        }
        self.save()?;
        Ok(released)
    }

    /// Voluntary handoff. Sets new acquired_at = now, preserves edit_count,
    /// re-reads content_sha256 + byte_size from disk per PM-q2.
    pub fn transfer(
        &mut self,
        path: &Path,
        from: &AgentId,
        to: &AgentId,
    ) -> Result<(), RegistryError> {
        let cpath = self.canonicalize_path(path)?;
        let now = self.clock.now();
        let prev = match self.owners.get(&cpath) {
            None => return Err(RegistryError::NotOwned { path: cpath }),
            Some(o) if &o.owner != from => {
                return Err(RegistryError::NotOwnedByRequester {
                    path: cpath.clone(),
                    current_owner: o.owner.clone(),
                    requester: from.clone(),
                });
            }
            Some(o) => o.clone(),
        };
        let (byte_size, content_sha256) = self.file_meta(&cpath);
        let next = Ownership {
            path: cpath.clone(),
            owner: to.clone(),
            acquired_at: now,
            last_touch: now,
            byte_size,
            content_sha256,
            edit_count: prev.edit_count,
        };
        self.owners.insert(cpath.clone(), next);

        // Reassign pending edits targeting `from` for this path → `to`.
        for h in self.pending_edits.iter_mut() {
            if h.path == cpath && h.owner == *from && matches!(h.status, EditStatus::Pending) {
                h.owner = to.clone();
            }
        }
        self.save()?;
        self.audit("TRANSFER", to, &cpath, Some(from));
        Ok(())
    }

    pub fn transfer_all_from(
        &mut self,
        from: &AgentId,
        to: &AgentId,
    ) -> Result<Vec<PathBuf>, RegistryError> {
        let paths: Vec<PathBuf> = self
            .owners
            .iter()
            .filter(|(_, o)| &o.owner == from)
            .map(|(p, _)| p.clone())
            .collect();
        let mut moved = Vec::new();
        for p in paths {
            self.transfer(&p, from, to)?;
            moved.push(p);
        }
        Ok(moved)
    }

    /// Request that the current owner apply an edit. Routes inline (PM-q1):
    /// the EditHandle is queued for the owner to pick up in their next
    /// dispatch.
    pub fn request_edit(
        &mut self,
        path: &Path,
        change: ChangeSpec,
        requester: &AgentId,
    ) -> Result<EditHandle, RegistryError> {
        let cpath = self.canonicalize_path(path)?;
        let owner = self
            .owners
            .get(&cpath)
            .map(|o| o.owner.clone())
            .ok_or_else(|| RegistryError::NotOwned { path: cpath.clone() })?;
        let now = self.clock.now();
        let id = self.next_id();
        let handle = EditHandle {
            id: id.clone(),
            path: cpath.clone(),
            requester: requester.clone(),
            owner: owner.clone(),
            change_spec: change,
            posted_at: now,
            status: EditStatus::Pending,
        };
        self.pending_edits.push(handle.clone());
        self.save()?;
        self.audit("REQUEST_EDIT", requester, &cpath, Some(&owner));
        Ok(handle)
    }

    /// Owner applies a queued edit. Idempotent if already Applied.
    pub fn apply_edit(&mut self, handle: &EditHandle) -> Result<(), RegistryError> {
        let id = handle.id.clone();
        let pos = self
            .pending_edits
            .iter()
            .position(|h| h.id == id)
            .ok_or_else(|| RegistryError::EditNotFound(id.clone()))?;
        let h = &mut self.pending_edits[pos];
        if matches!(h.status, EditStatus::Applied { .. }) {
            return Ok(());
        }
        h.status = EditStatus::Applied { commit_sha: None };
        // bump owner edit count
        if let Some(o) = self.owners.get_mut(&h.path) {
            o.edit_count = o.edit_count.saturating_add(1);
            o.last_touch = self.clock.now();
        }
        let owner = h.owner.clone();
        let path = h.path.clone();
        self.save()?;
        self.audit("APPLY_EDIT", &owner, &path, Some(&owner));
        Ok(())
    }

    pub fn reject_edit(&mut self, handle: &EditHandle, reason: String) -> Result<(), RegistryError> {
        let id = handle.id.clone();
        let pos = self
            .pending_edits
            .iter()
            .position(|h| h.id == id)
            .ok_or_else(|| RegistryError::EditNotFound(id.clone()))?;
        self.pending_edits[pos].status = EditStatus::Rejected {
            reason: reason.clone(),
        };
        let owner = self.pending_edits[pos].owner.clone();
        let path = self.pending_edits[pos].path.clone();
        self.save()?;
        self.audit("REJECT_EDIT", &owner, &path, Some(&owner));
        Ok(())
    }

    pub fn list_owners(&self) -> Vec<(PathBuf, AgentId, SystemTime)> {
        let mut v: Vec<_> = self
            .owners
            .iter()
            .map(|(p, o)| (p.clone(), o.owner.clone(), o.acquired_at))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    pub fn pending_for(&self, owner: &AgentId) -> Vec<&EditHandle> {
        self.pending_edits
            .iter()
            .filter(|h| &h.owner == owner && matches!(h.status, EditStatus::Pending))
            .collect()
    }

    /// Forensics: was this path ever loaded into context this session?
    pub fn has_been_acquired(&self, path: &Path) -> bool {
        let cpath = match self.canonicalize_path(path) {
            Ok(p) => p,
            Err(_) => return false,
        };
        self.owners.contains_key(&cpath)
    }

    pub fn mark_crashed(&mut self, agent: &AgentId) {
        let now = self.clock.now();
        self.crashed_owners.insert(agent.clone(), now);
        let _ = self.save();
        self.audit("MARK_CRASHED", agent, Path::new(""), None);
    }

    /// Sweep: forcibly release files held by crashed agents whose grace has
    /// elapsed.
    pub fn sweep_crashed(&mut self) -> Vec<PathBuf> {
        let now = self.clock.now();
        let grace_expired: Vec<AgentId> = self
            .crashed_owners
            .iter()
            .filter_map(|(a, t)| {
                now.duration_since(*t)
                    .ok()
                    .filter(|d| *d >= CRASH_GRACE)
                    .map(|_| a.clone())
            })
            .collect();
        let mut released = Vec::new();
        for agent in grace_expired {
            let paths: Vec<PathBuf> = self
                .owners
                .iter()
                .filter(|(_, o)| o.owner == agent)
                .map(|(p, _)| p.clone())
                .collect();
            for p in paths {
                self.owners.remove(&p);
                released.push(p.clone());
                self.audit("CRASH_RELEASE", &agent, &p, Some(&agent));
            }
            self.crashed_owners.remove(&agent);
        }
        let _ = self.save();
        released
    }

    /// Same-wave conflict resolution per spec §6: among a set of candidate
    /// `(agent_id, request_timestamp)` tuples wanting the same file, pick the
    /// winner.
    ///
    /// Order: (1) earliest timestamp wins; (2) tiebreak by role priority; (3)
    /// tiebreak lexicographic agent_id.
    pub fn pick_same_wave_winner(candidates: &[(AgentId, SystemTime)]) -> Option<AgentId> {
        candidates
            .iter()
            .min_by(|a, b| {
                a.1.cmp(&b.1)
                    .then_with(|| role_priority(a.0.role()).cmp(&role_priority(b.0.role())))
                    .then_with(|| a.0 .0.cmp(&b.0 .0))
            })
            .map(|(a, _)| a.clone())
    }

    /// Read the audit log JSONL file (best-effort; returns empty on read fail).
    pub fn audit_log_entries(&self) -> Vec<serde_json::Value> {
        match fs::read_to_string(&self.audit_path) {
            Ok(s) => s
                .lines()
                .filter(|l| !l.trim().is_empty())
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn audit_path(&self) -> &Path {
        &self.audit_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(s: &str) -> AgentId {
        AgentId::new(s)
    }

    #[test]
    fn pick_same_wave_winner_fifo_then_role_then_lex() {
        let t0 = UNIX_EPOCH + Duration::from_micros(1000);
        let t1 = UNIX_EPOCH + Duration::from_micros(2000);
        // FIFO wins.
        let cands = vec![
            (agent("Developer:1:c0:0001"), t1),
            (agent("Developer:1:c1:0002"), t0),
        ];
        assert_eq!(
            FileRegistry::pick_same_wave_winner(&cands).unwrap(),
            agent("Developer:1:c1:0002")
        );
        // Tie on timestamp: role priority.
        let cands = vec![
            (agent("Researcher:1:c1:0002"), t0),
            (agent("Developer:1:c0:0001"), t0),
        ];
        // PM=1, SwE=2, Developer=3, Researcher=4 — Developer wins.
        assert_eq!(
            FileRegistry::pick_same_wave_winner(&cands).unwrap(),
            agent("Developer:1:c0:0001")
        );
        // Tie on role: lex.
        let cands = vec![
            (agent("Developer:1:c0:0002"), t0),
            (agent("Developer:1:c0:0001"), t0),
        ];
        assert_eq!(
            FileRegistry::pick_same_wave_winner(&cands).unwrap(),
            agent("Developer:1:c0:0001")
        );
    }
}
