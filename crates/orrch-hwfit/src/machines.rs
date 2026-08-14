// =====================================================================
// HWF-004  src/machines.rs  — portable machine registry + probe cache
// =====================================================================
//
// PERSISTENCE PATH:  ~/.config/orrchestrator/machines.json
//   A machine registry is USER/HOST state, not project state — the same
//   registry (orrion, orrgate, mbp15, ...) is reused across every project the
//   user assesses, so it does NOT belong in any one project's .orrch/. It
//   mirrors the existing valves.json convention the TUI already uses. Resolve
//   via dirs::config_dir().join("orrchestrator/machines.json"); honor
//   $ORRCH_CONFIG_DIR override if set.

use crate::types::{FitResult, SystemInfo};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One entry in the registry. `localhost` is NEVER stored here — it is injected
/// implicitly by `all()` so it can't be deleted or duplicated.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Machine {
    /// Display name, unique within the registry. e.g. "orrion".
    pub name: String,
    /// SSH target "user@host". None ⇒ localhost-style (only valid for the
    /// implicit localhost entry; registry entries always carry Some).
    pub ssh_host: Option<String>,
    /// SSH port as a string ("" or "22" ⇒ default). Matches detect_system's
    /// `ssh_port: &str` arg shape.
    pub ssh_port: Option<String>,
    /// "", "linux", "windows", "termux". Passed straight to detect_system's
    /// `plat` arg. None ⇒ "".
    pub platform: Option<String>,
}

impl Machine {
    /// The implicit localhost machine. `all()` always prepends this; it is not
    /// persisted. ssh_host=None ⇒ probe() drives a LOCAL detect_system.
    pub fn localhost() -> Self {
        Machine {
            name: "localhost".to_string(),
            ssh_host: None,
            ssh_port: None,
            platform: None,
        }
    }

    /// True for the implicit localhost entry (name=="localhost" && ssh_host==None).
    pub fn is_localhost(&self) -> bool {
        self.ssh_host.is_none()
    }

    /// Probe args (host, ssh_port, plat) for detect_system. localhost ⇒ all "".
    fn probe_args(&self) -> (String, String, String) {
        (
            self.ssh_host.clone().unwrap_or_default(),
            self.ssh_port.clone().unwrap_or_default(),
            self.platform.clone().unwrap_or_default(),
        )
    }
}

/// Portable, serde-persisted registry. The on-disk JSON is exactly
/// `{ "machines": [Machine, ...] }` (localhost excluded).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MachineRegistry {
    #[serde(default)]
    pub machines: Vec<Machine>,
}

impl MachineRegistry {
    /// Resolve the portable config path:
    ///   $ORRCH_CONFIG_DIR/machines.json   (if env set)
    ///   else  dirs::config_dir()/orrchestrator/machines.json
    pub fn default_path() -> PathBuf {
        if let Ok(dir) = std::env::var("ORRCH_CONFIG_DIR") {
            return Path::new(&dir).join("machines.json");
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("orrchestrator")
            .join("machines.json")
    }

    /// Load from `default_path()`. Missing file / parse error ⇒ empty registry
    /// (never errors — a fresh user just has localhost).
    pub fn load() -> Self {
        Self::load_from(&Self::default_path())
    }

    /// Load from an explicit path (unit-testable). Missing/invalid ⇒ default.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist to `default_path()`, creating parent dirs. Pretty JSON.
    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&Self::default_path())
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// FULL machine list the UI iterates: implicit localhost FIRST, then every
    /// persisted registry entry in insertion order. This is what the TUI's
    /// machine selector binds to.
    pub fn all(&self) -> Vec<Machine> {
        let mut out = Vec::with_capacity(self.machines.len() + 1);
        out.push(Machine::localhost());
        out.extend(self.machines.iter().cloned());
        out
    }

    /// Add or replace by name (localhost name is rejected — it's implicit).
    /// Returns false if `name=="localhost"` or `ssh_host` is None. Does NOT
    /// persist; caller saves.
    pub fn upsert(&mut self, m: Machine) -> bool {
        if m.name == "localhost" || m.ssh_host.is_none() {
            return false;
        }
        if let Some(slot) = self.machines.iter_mut().find(|x| x.name == m.name) {
            *slot = m;
        } else {
            self.machines.push(m);
        }
        true
    }

    /// Remove by name. localhost can't be removed (it isn't stored).
    pub fn remove(&mut self, name: &str) {
        self.machines.retain(|x| x.name != name);
    }
}

/// Probe one machine WITH the per-host 30-min cache that already lives in
/// probe.rs (`detect_system` keys its cache by host string; localhost ⇒ "_local").
///
/// This is a thin, intent-revealing wrapper so the TUI never calls
/// detect_system directly: it maps a `Machine` to detect_system's positional
/// (host, ssh_port, plat) and forwards `fresh`. `fresh=true` ⇒ force re-scan
/// (Rescan button); `fresh=false` ⇒ reuse cache if < CACHE_TTL (1800s).
pub fn probe_machine(machine: &Machine, fresh: bool) -> SystemInfo {
    let (host, port, plat) = machine.probe_args();
    crate::probe::detect_system(&host, &port, &plat, fresh)
}

// =====================================================================
// Convenience the TUI calls directly (HWF-005 binding surface)
// =====================================================================

/// Re-exported from fit.rs for the TUI's call site (keeps imports on one path).
pub use crate::fit::RankOptions;

/// Probe `machine` (cached unless `fresh`), load the default catalog ONCE, and
/// rank it. The single call the Fit panel makes when (machine changed | Rescan
/// pressed | first view). The TUI caches the returned Vec in App state and does
/// NOT call this every frame.
///
/// `opts.limit==0` ⇒ rank_models defaults to 50 (existing behavior). On a probe
/// error (SystemInfo.error.is_some()) the returned Vec is still valid (mostly
/// no_fit rows); the TUI should surface `system.error` in the header.
pub fn rank_for_machine(
    machine: &Machine,
    fresh: bool,
    opts: &RankOptions,
) -> (SystemInfo, Vec<FitResult>) {
    let system = probe_machine(machine, fresh);
    let catalog = crate::models::load_catalog(&crate::models::default_catalog_path());
    let ranked = crate::fit::rank_models(&catalog, &system, opts);
    (system, ranked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn localhost_is_implicit_and_first() {
        let reg = MachineRegistry::default();
        let all = reg.all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "localhost");
        assert!(all[0].is_localhost());
        assert!(all[0].ssh_host.is_none());
    }

    #[test]
    fn upsert_rejects_localhost_and_hostless() {
        let mut reg = MachineRegistry::default();
        assert!(!reg.upsert(Machine::localhost()));
        // localhost name with an ssh_host still rejected (reserved name)
        assert!(!reg.upsert(Machine {
            name: "localhost".into(),
            ssh_host: Some("dev@x".into()),
            ssh_port: None,
            platform: None,
        }));
        // missing ssh_host rejected
        assert!(!reg.upsert(Machine {
            name: "node-a".into(),
            ssh_host: None,
            ssh_port: None,
            platform: None,
        }));
        assert!(reg.machines.is_empty());
    }

    #[test]
    fn upsert_inserts_then_replaces_by_name() {
        let mut reg = MachineRegistry::default();
        assert!(reg.upsert(Machine {
            name: "node-a".into(),
            ssh_host: Some("dev@node-a".into()),
            ssh_port: None,
            platform: Some("linux".into()),
        }));
        assert_eq!(reg.machines.len(), 1);
        // replace (same name): updated in place, no duplicate
        assert!(reg.upsert(Machine {
            name: "node-a".into(),
            ssh_host: Some("dev@node-a".into()),
            ssh_port: Some("2222".into()),
            platform: Some("linux".into()),
        }));
        assert_eq!(reg.machines.len(), 1);
        assert_eq!(reg.machines[0].ssh_host.as_deref(), Some("dev@node-a"));
        assert_eq!(reg.machines[0].ssh_port.as_deref(), Some("2222"));
    }

    #[test]
    fn remove_by_name() {
        let mut reg = MachineRegistry::default();
        reg.upsert(Machine {
            name: "a".into(),
            ssh_host: Some("u@a".into()),
            ssh_port: None,
            platform: None,
        });
        reg.upsert(Machine {
            name: "b".into(),
            ssh_host: Some("u@b".into()),
            ssh_port: None,
            platform: None,
        });
        reg.remove("a");
        assert_eq!(reg.machines.len(), 1);
        assert_eq!(reg.machines[0].name, "b");
        // removing missing name is a no-op
        reg.remove("nonexistent");
        assert_eq!(reg.machines.len(), 1);
    }

    #[test]
    fn registry_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("machines.json");

        let mut reg = MachineRegistry::default();
        reg.upsert(Machine {
            name: "node-a".into(),
            ssh_host: Some("dev@node-a".into()),
            ssh_port: Some("22".into()),
            platform: Some("linux".into()),
        });
        reg.upsert(Machine {
            name: "node-b".into(),
            ssh_host: Some("dev@node-b".into()),
            ssh_port: None,
            platform: Some("linux".into()),
        });
        reg.save_to(&path).unwrap();

        // On-disk JSON excludes localhost.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("localhost"));
        assert!(raw.contains("node-a"));

        let loaded = MachineRegistry::load_from(&path);
        assert_eq!(loaded.machines, reg.machines);

        // all() reconstructs implicit localhost + the two entries, in order.
        let all = loaded.all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].name, "localhost");
        assert_eq!(all[1].name, "node-a");
        assert_eq!(all[2].name, "node-b");
    }

    #[test]
    fn load_from_missing_file_yields_empty() {
        let reg = MachineRegistry::load_from(Path::new("/nonexistent/path/machines.json"));
        assert!(reg.machines.is_empty());
        assert_eq!(reg.all().len(), 1); // just localhost
    }

    #[test]
    fn load_from_garbage_yields_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("machines.json");
        std::fs::write(&path, "this is not json {{{").unwrap();
        let reg = MachineRegistry::load_from(&path);
        assert!(reg.machines.is_empty());
    }

    #[test]
    fn default_path_honors_env_override() {
        // SAFETY: single-threaded test; we restore the env after.
        let prev = std::env::var("ORRCH_CONFIG_DIR").ok();
        unsafe {
            std::env::set_var("ORRCH_CONFIG_DIR", "/tmp/orrch-test-cfg");
        }
        let p = MachineRegistry::default_path();
        assert_eq!(p, Path::new("/tmp/orrch-test-cfg/machines.json"));
        unsafe {
            match prev {
                Some(v) => std::env::set_var("ORRCH_CONFIG_DIR", v),
                None => std::env::remove_var("ORRCH_CONFIG_DIR"),
            }
        }
    }

    // ── cache TTL behavior (exercises probe.rs's per-host cache through the
    //    Machine wrapper). localhost keys "_local"; first probe populates, the
    //    second (fresh=false) returns the SAME cached value cheaply. ──
    #[test]
    fn probe_localhost_cache_is_reused_when_not_fresh() {
        let lh = Machine::localhost();

        // First probe (cold): populates the 1800s cache for "_local".
        let _warm = probe_machine(&lh, true);

        // Cached read should be effectively instant (no re-scan, no SSH/proc walk).
        let t0 = Instant::now();
        let cached = probe_machine(&lh, false);
        let dt = t0.elapsed();

        // A cache hit clones a struct; it must be far below the time a real
        // detection (nvidia-smi/sysfs/proc reads) would take. 50ms is a very
        // generous ceiling that still proves the cache short-circuited the probe.
        assert!(
            dt.as_millis() < 50,
            "cached probe took {dt:?}; expected a sub-50ms cache hit"
        );
        // The cached SystemInfo is a real result (localhost always has >=1 core).
        assert!(cached.cpu_cores >= 1);
    }
}
