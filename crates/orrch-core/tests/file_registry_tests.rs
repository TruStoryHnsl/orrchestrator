//! Integration tests for the per-session file-ownership registry.
//!
//! Tests are written from a user-oriented perspective: each scenario simulates
//! what a workflow dispatcher / agent would actually observe (audit log
//! contents, returned owner identities, error variants, on-disk state).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use orrch_core::file_registry::{
    AgentId, ChangeSpec, Clock, EditStatus, FileRegistry, ManualClock, RegistryError,
    DEFAULT_AUDIT_LOG, DEFAULT_REGISTRY_PATH, SOFT_DOUBLE_READ_ENV,
};

use tempfile::TempDir;

fn make_registry() -> (TempDir, FileRegistry, Arc<ManualClock>) {
    let dir = TempDir::new().unwrap();
    let start = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let clock = Arc::new(ManualClock::new(start));
    let reg = FileRegistry::load_or_init_with_clock(dir.path(), clock.clone() as Arc<dyn Clock>)
        .expect("registry init");
    (dir, reg, clock)
}

fn touch(dir: &TempDir, rel: &str, contents: &str) -> PathBuf {
    let p = dir.path().join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&p, contents).unwrap();
    p
}

fn read_audit(dir: &TempDir) -> Vec<serde_json::Value> {
    let p = dir.path().join(DEFAULT_AUDIT_LOG);
    let s = std::fs::read_to_string(&p).unwrap_or_default();
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .collect()
}

fn verdicts(entries: &[serde_json::Value]) -> Vec<String> {
    entries
        .iter()
        .map(|e| e["verdict"].as_str().unwrap().to_string())
        .collect()
}

fn agent(s: &str) -> AgentId {
    AgentId::new(s)
}

// ---- 1. clean_acquire_release ------------------------------------------------

#[test]
fn clean_acquire_release() {
    let (dir, mut reg, _clock) = make_registry();
    touch(&dir, "foo.rs", "fn main() {}\n");
    let a = agent("Developer:1:c0:0001");

    reg.acquire(&dir.path().join("foo.rs"), &a).unwrap();
    assert!(reg.has_been_acquired(&dir.path().join("foo.rs")));

    reg.release(&dir.path().join("foo.rs"), &a).unwrap();
    assert!(!reg.has_been_acquired(&dir.path().join("foo.rs")));

    // .orrch/file_registry.json on disk has empty owners.
    let raw = std::fs::read_to_string(dir.path().join(DEFAULT_REGISTRY_PATH)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed["owners"].as_object().unwrap().len(), 0);

    let verds = verdicts(&read_audit(&dir));
    assert!(verds.contains(&"ACQUIRE".to_string()));
    assert!(verds.contains(&"RELEASE".to_string()));
}

// ---- 2. double_acquire_hard_error -------------------------------------------

#[test]
fn double_acquire_hard_error() {
    let (dir, mut reg, _clock) = make_registry();
    touch(&dir, "foo.rs", "x");
    let a = agent("Developer:1:c0:0001");
    let b = agent("Researcher:1:c1:0002");

    reg.acquire(&dir.path().join("foo.rs"), &a).unwrap();
    // SAFETY: serial test — env var is single-threaded read by the registry.
    unsafe { std::env::remove_var(SOFT_DOUBLE_READ_ENV) };
    let err = reg
        .acquire(&dir.path().join("foo.rs"), &b)
        .expect_err("second acquire must fail");
    match err {
        RegistryError::AlreadyOwned { current_owner, .. } => {
            assert_eq!(current_owner, a);
        }
        other => panic!("unexpected error: {other}"),
    }
    let verds = verdicts(&read_audit(&dir));
    assert!(
        verds.contains(&"DOUBLE_READ_BLOCKED".to_string()),
        "audit log must record blocked attempt; got {verds:?}"
    );
}

#[test]
fn double_acquire_soft_still_returns_error() {
    let (dir, mut reg, _clock) = make_registry();
    touch(&dir, "foo.rs", "x");
    let a = agent("Developer:1:c0:0001");
    let b = agent("Researcher:1:c1:0002");

    reg.acquire(&dir.path().join("foo.rs"), &a).unwrap();
    unsafe { std::env::set_var(SOFT_DOUBLE_READ_ENV, "1") };
    let err = reg.acquire(&dir.path().join("foo.rs"), &b);
    unsafe { std::env::remove_var(SOFT_DOUBLE_READ_ENV) };

    // Spec: "soft warning, not soft permission" — still returns AlreadyOwned.
    assert!(matches!(err, Err(RegistryError::AlreadyOwned { .. })));
    let verds = verdicts(&read_audit(&dir));
    assert!(
        verds.contains(&"DOUBLE_READ_SOFT".to_string()),
        "audit log must record SOFT verdict; got {verds:?}"
    );
}

// ---- 3. transfer_succeeds ---------------------------------------------------

#[test]
fn transfer_succeeds() {
    let (dir, mut reg, _clock) = make_registry();
    touch(&dir, "foo.rs", "y");
    let a = agent("Developer:1:c0:0001");
    let b = agent("Tester:2:c0:0001");

    reg.acquire(&dir.path().join("foo.rs"), &a).unwrap();
    reg.transfer(&dir.path().join("foo.rs"), &a, &b).unwrap();

    let listing = reg.list_owners();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].1, b);

    let err = reg
        .release(&dir.path().join("foo.rs"), &a)
        .expect_err("A no longer owns it");
    assert!(matches!(err, RegistryError::NotOwnedByRequester { .. }));

    // B can release.
    reg.release(&dir.path().join("foo.rs"), &b).unwrap();
}

// ---- 4. request_edit_routed_inline ------------------------------------------

#[test]
fn request_edit_routed_inline() {
    let (dir, mut reg, _clock) = make_registry();
    touch(&dir, "foo.rs", "z");
    let a = agent("Developer:1:c0:0001");
    let b = agent("Researcher:1:c1:0002");
    reg.acquire(&dir.path().join("foo.rs"), &a).unwrap();

    let directive = "add pub mod file_registry".to_string();
    let handle = reg
        .request_edit(
            &dir.path().join("foo.rs"),
            ChangeSpec::Natural {
                directive: directive.clone(),
            },
            &b,
        )
        .expect("request_edit posts handle");
    assert_eq!(handle.owner, a);
    assert_eq!(handle.requester, b);
    match &handle.change_spec {
        ChangeSpec::Natural { directive: d } => assert_eq!(d, &directive),
        _ => panic!("wrong change_spec variant"),
    }
    assert!(matches!(handle.status, EditStatus::Pending));

    // Inline: A picks up the pending edit and applies it.
    let pending: Vec<_> = reg.pending_for(&a).into_iter().cloned().collect();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, handle.id);
    reg.apply_edit(&pending[0]).unwrap();

    // After apply, status is Applied; pending list is empty for A.
    assert!(reg.pending_for(&a).is_empty());
}

// ---- 5. crash_recovery_after_timeout ----------------------------------------

#[test]
fn crash_recovery_after_timeout() {
    let (dir, mut reg, clock) = make_registry();
    touch(&dir, "foo.rs", "q");
    let a = agent("Developer:1:c0:0001");
    let b = agent("Developer:1:c1:0002");
    reg.acquire(&dir.path().join("foo.rs"), &a).unwrap();
    reg.mark_crashed(&a);

    // Before grace elapses, sweep does nothing.
    let released = reg.sweep_crashed();
    assert!(released.is_empty(), "no release before grace elapses");

    // Advance >=300s.
    clock.advance(Duration::from_secs(301));
    let released = reg.sweep_crashed();
    assert_eq!(released.len(), 1);

    // B can now acquire.
    reg.acquire(&dir.path().join("foo.rs"), &b)
        .expect("B should acquire after crash release");

    let verds = verdicts(&read_audit(&dir));
    assert!(verds.contains(&"CRASH_RELEASE".to_string()));
}

// ---- 6. same_wave_fifo_tiebreak ---------------------------------------------

#[test]
fn same_wave_fifo_tiebreak() {
    // Three near-simultaneous candidates.
    let t0 = UNIX_EPOCH + Duration::from_micros(1_000_000);
    let t1 = UNIX_EPOCH + Duration::from_micros(1_000_001);
    let t2 = UNIX_EPOCH + Duration::from_micros(1_000_002);

    let cands = vec![
        (agent("Researcher:1:c2:0001"), t1),
        (agent("Developer:1:c0:0001"), t2),
        (agent("Developer:1:c1:0002"), t0),
    ];
    // FIFO winner: t0 → Developer:1:c1:0002.
    let w = FileRegistry::pick_same_wave_winner(&cands).unwrap();
    assert_eq!(w, agent("Developer:1:c1:0002"));

    // Tie on timestamp: role priority decides.
    let cands_tie = vec![
        (agent("Researcher:1:c2:0001"), t0),
        (agent("Developer:1:c0:0001"), t0),
        (agent("UI_Designer:1:c3:0003"), t0),
    ];
    let w = FileRegistry::pick_same_wave_winner(&cands_tie).unwrap();
    assert_eq!(w, agent("Developer:1:c0:0001"));

    // Same role, same ts: lexicographic AgentId.
    let cands_lex = vec![
        (agent("Developer:1:c0:0002"), t0),
        (agent("Developer:1:c0:0001"), t0),
    ];
    let w = FileRegistry::pick_same_wave_winner(&cands_lex).unwrap();
    assert_eq!(w, agent("Developer:1:c0:0001"));
}

// ---- 7. persistence_roundtrip -----------------------------------------------

#[test]
fn persistence_roundtrip() {
    let dir = TempDir::new().unwrap();
    let start = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let clock = Arc::new(ManualClock::new(start));
    let files = ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"];
    let agents = [
        agent("Developer:1:c0:0001"),
        agent("Developer:1:c1:0002"),
        agent("Researcher:1:c2:0003"),
        agent("Developer:1:c3:0004"),
        agent("UI_Designer:1:c4:0005"),
    ];
    let mut reg = FileRegistry::load_or_init_with_clock(
        dir.path(),
        clock.clone() as Arc<dyn Clock>,
    )
    .unwrap();
    for (f, a) in files.iter().zip(agents.iter()) {
        let p = dir.path().join(f);
        std::fs::write(&p, format!("contents of {f}")).unwrap();
        reg.acquire(&p, a).unwrap();
    }
    let listing_before = reg.list_owners();
    drop(reg);

    // Reconstruct from disk.
    let reg2 = FileRegistry::load_or_init_with_clock(
        dir.path(),
        clock.clone() as Arc<dyn Clock>,
    )
    .unwrap();
    let listing_after = reg2.list_owners();
    assert_eq!(listing_before.len(), listing_after.len());
    for ((p1, a1, t1), (p2, a2, t2)) in listing_before.iter().zip(listing_after.iter()) {
        assert_eq!(p1, p2);
        assert_eq!(a1, a2);
        assert_eq!(t1, t2);
    }
}

// ---- 8. integration_3_agent_5_file_scenario ---------------------------------

/// Replicates the Mermaid sequence diagram in plans/specs/file-registry.md §
/// "Sequence diagram — 3 agents, 5 files".
///
/// Token-economy property: each of the 5 file paths gets exactly ONE
/// "first-acquire" entry in the audit log across the whole scenario.
#[test]
fn integration_3_agent_5_file_scenario() {
    let (dir, mut reg, clock) = make_registry();

    let dev = agent("Developer:1:c0:0001");
    let swe = agent("Software_Engineer:1:c1:0002");
    let tester = agent("Tester:2:c0:0001");

    let files = [
        "lib.rs",
        "agent.rs",
        "session.rs",
        "workflow_status.rs",
        "process_manager.rs",
    ];
    for f in files {
        touch(&dir, f, &format!("// {}\n", f));
    }

    // Wave 1: Dispatcher try_acquires each file, bundling into cluster ctx.
    let dev_files = ["lib.rs", "agent.rs"];
    let swe_files = ["session.rs", "workflow_status.rs", "process_manager.rs"];
    for f in dev_files {
        let owner = reg.try_acquire(&dir.path().join(f), &dev).unwrap();
        assert_eq!(owner, dev);
    }
    for f in swe_files {
        let owner = reg.try_acquire(&dir.path().join(f), &swe).unwrap();
        assert_eq!(owner, swe);
    }

    // Mid-task: Dev requests an edit on session.rs (owned by SwE) — no re-read.
    let handle = reg
        .request_edit(
            &dir.path().join("session.rs"),
            ChangeSpec::Patch {
                unified_diff: "@@ ...".into(),
            },
            &dev,
        )
        .unwrap();
    assert_eq!(handle.owner, swe);

    // SwE applies the edit.
    let pending: Vec<_> = reg.pending_for(&swe).into_iter().cloned().collect();
    assert_eq!(pending.len(), 1);
    reg.apply_edit(&pending[0]).unwrap();

    // Both agents complete. Dispatcher transfers all to Tester (wave 2).
    clock.advance(Duration::from_secs(5));
    let moved_dev = reg.transfer_all_from(&dev, &tester).unwrap();
    let moved_swe = reg.transfer_all_from(&swe, &tester).unwrap();
    assert_eq!(moved_dev.len(), 2);
    assert_eq!(moved_swe.len(), 3);

    // Tester now owns all 5.
    let listing = reg.list_owners();
    assert_eq!(listing.len(), 5);
    for (_p, owner, _t) in &listing {
        assert_eq!(owner, &tester);
    }

    // Tester completes; release_all.
    let released = reg.release_all(&tester).unwrap();
    assert_eq!(released.len(), 5);

    // OBSERVATION: each file got exactly one ACQUIRE entry across the run.
    let entries = read_audit(&dir);
    for f in files {
        let abs = dir
            .path()
            .canonicalize()
            .unwrap()
            .join(f)
            .to_string_lossy()
            .into_owned();
        let count = entries
            .iter()
            .filter(|e| e["verdict"] == "ACQUIRE" && e["path"] == abs)
            .count();
        assert_eq!(
            count, 1,
            "file {f} must have exactly one ACQUIRE audit entry; got {count}"
        );
    }
    // And TRANSFER + RELEASE_ALL entries present.
    let verds = verdicts(&entries);
    assert!(verds.contains(&"TRANSFER".to_string()));
    assert!(verds.contains(&"RELEASE_ALL".to_string()));
    assert!(verds.contains(&"REQUEST_EDIT".to_string()));
    assert!(verds.contains(&"APPLY_EDIT".to_string()));
}

// ---- bonus: malformed file tolerated ---------------------------------------

#[test]
fn malformed_state_file_tolerated() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".orrch")).unwrap();
    std::fs::write(
        dir.path().join(DEFAULT_REGISTRY_PATH),
        "{ this is not valid json ",
    )
    .unwrap();
    let reg = FileRegistry::load_or_init(dir.path()).expect("should start fresh on malformed");
    assert!(reg.list_owners().is_empty());
}
