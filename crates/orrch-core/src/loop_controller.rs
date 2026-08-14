//! LOOP-001 + LOOP-003 — the deterministic autonomous-loop RUNTIME.
//!
//! The DATA MODEL (`LoopSchedule` + JSON storage) lives in [`crate::loops`];
//! this module builds ON it and does NOT duplicate the schedule/storage/ordering
//! logic.
//!
//! CORE PRINCIPLE: the loop is a DETERMINISTIC state machine. ZERO LLM lives in
//! the control logic. Every external effect — spawning a workforce, killing its
//! sessions, asking "is there work?", asking "what time is it?" — is hidden
//! behind a trait so the controller is unit-testable with mocks (no live agents,
//! no real sleeps, no real filesystem).
//!
//! `LoopController::tick(&mut self) -> LoopState` is the ONLY entry point that
//! advances state; it is pure w.r.t. the injected [`Clock`], [`WorkEvaluator`],
//! and [`WorkflowRunner`]. No real time, no LLM, no I/O of its own.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::loops::LoopSchedule;

// ===========================================================================
// Clock — injected time so tests never sleep and never read the wall clock.
// ===========================================================================

/// Monotonic logical instant. Newtype over u128 nanoseconds so a `ManualClock`
/// can advance it by exact amounts. Real impl maps from `Instant`/`SystemTime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tstamp(pub u128);

impl Tstamp {
    pub const ZERO: Tstamp = Tstamp(0);
    pub fn saturating_since(self, earlier: Tstamp) -> Duration {
        Duration::from_nanos((self.0.saturating_sub(earlier.0)) as u64)
    }
}

/// Time source. The controller and watchdog NEVER call `Instant::now()` or
/// `SystemTime::now()` directly — they call `clock.now()`. Inject a `ManualClock`
/// in tests to drive elapsed-time logic deterministically.
pub trait Clock: Send + Sync {
    fn now(&self) -> Tstamp;
}

/// Wall-clock implementation: monotonic nanoseconds since process start.
#[derive(Debug)]
pub struct SystemClock {
    origin: std::time::Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            origin: std::time::Instant::now(),
        }
    }
}

impl SystemClock {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Tstamp {
        Tstamp(self.origin.elapsed().as_nanos())
    }
}

/// Test clock advanced by hand. Interior-mutable so it can be shared via `Arc`.
#[derive(Debug, Default)]
pub struct ManualClock {
    nanos: std::sync::atomic::AtomicU64,
}

impl ManualClock {
    pub fn new() -> Self {
        Self::default()
    }
    /// Advance the logical clock by `d`.
    pub fn advance(&self, d: Duration) {
        self.nanos
            .fetch_add(d.as_nanos() as u64, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Tstamp {
        Tstamp(self.nanos.load(std::sync::atomic::Ordering::SeqCst) as u128)
    }
}

// ===========================================================================
// LoopState — the controller's state machine (exact enum from LOOP-001).
// ===========================================================================

/// Deterministic state of one running [`LoopSchedule`].
///
/// Transitions (driven only by `tick`, never by an LLM):
///   Idle ──work appears──▶ Evaluating
///   Evaluating ──work + capacity──▶ Running{workforce, since}
///   Evaluating ──no work──▶ Idle
///   Evaluating ──work but user-gated (after K retries)──▶ Paused{reason}
///   Running ──cleanup_summary.md written──▶ AdvancingWorkforce
///   Running ──workforce needs-input it can't self-resolve (after K)──▶ Paused
///   AdvancingWorkforce ──sessions closed, index bumped──▶ Evaluating
///   Paused ──resolving feedback──▶ Evaluating
///   (Stopped only on explicit disable/teardown — the loop NEVER self-Stops on
///    drained work; it goes Idle and keeps watching.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopState {
    /// No actionable work right now. Not terminal — the controller keeps
    /// re-evaluating and leaves Idle the moment work reappears.
    Idle,
    /// Deciding what to do this tick: re-reads actionable work + capacity.
    Evaluating,
    /// A workforce is live. `workforce` is the schedule index currently
    /// dispatched; `since` is when it was spawned (for watchdog liveness).
    Running { workforce: usize, since: Tstamp },
    /// A workforce reported completion (cleanup_summary.md). Closing its
    /// sessions and bumping the schedule index (wrapping past the end).
    AdvancingWorkforce,
    /// Genuine user-decision gate reached AFTER exhausting autonomous options.
    /// Holds the diagnosed blocker. NEVER reached by simply running out of work.
    Paused { reason: BlockedReason },
    /// Terminal. Only via explicit external teardown (schedule disabled /
    /// orrchestrator shutdown). Drained work does NOT produce this.
    Stopped,
}

impl LoopState {
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            LoopState::Evaluating | LoopState::Running { .. } | LoopState::AdvancingWorkforce
        )
    }
    pub fn is_paused(&self) -> bool {
        matches!(self, LoopState::Paused { .. })
    }
}

// ===========================================================================
// BlockedReason — why a loop is parked on the user.
// ===========================================================================

/// A genuine, autonomously-irreducible blocker. Each variant carries enough
/// context to render a one-line user prompt and to decide whether a piece of
/// incoming feedback resolves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockedReason {
    /// A workforce emitted a needs-input signal it could not self-resolve
    /// (e.g. a question requiring a human decision) AFTER K autonomous attempts.
    NeedsUserDecision {
        workforce: String,
        question: String,
        autonomous_attempts: u32,
    },
    /// No inbox items remain and every remaining PLAN item is user-gated
    /// (blocked on a decision the loop cannot make on its own).
    AllRemainingWorkUserGated { pending_plan_items: Vec<String> },
    /// A session died/failed repeatedly and exhausted its bounded retries; the
    /// loop parks rather than thrash (the watchdog may also catch this).
    RetriesExhausted {
        workforce: String,
        attempts: u32,
        last_error: String,
    },
}

impl BlockedReason {
    /// Human-readable one-liner for Hypervise + the push notification.
    pub fn summary(&self) -> String {
        match self {
            BlockedReason::NeedsUserDecision {
                workforce,
                question,
                ..
            } => format!("'{workforce}' needs a decision: {question}"),
            BlockedReason::AllRemainingWorkUserGated { pending_plan_items } => format!(
                "all remaining work is user-gated ({} item(s))",
                pending_plan_items.len()
            ),
            BlockedReason::RetriesExhausted {
                workforce,
                attempts,
                last_error,
            } => format!("'{workforce}' failed after {attempts} retries: {last_error}"),
        }
    }
}

// ===========================================================================
// WorkEvaluator — "is there actionable work?" abstracted (no real FS in tests).
// ===========================================================================

/// Snapshot of a project's actionable-work surface, computed deterministically
/// from the existing detectors (`count_queued_prompts` for the inbox,
/// `open_roadmap_items()` for unchecked PLAN items). Pure data — no behaviour.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkAssessment {
    /// Count of queued inbox instructions (INS-/OPT- headers).
    pub inbox_items: usize,
    /// Unchecked PLAN items the loop is allowed to act on autonomously.
    pub actionable_plan_items: usize,
    /// Unchecked PLAN items that are gated on a user decision (cannot be
    /// auto-progressed). Used to distinguish Idle from Paused-user-gated.
    pub user_gated_plan_items: Vec<String>,
}

impl WorkAssessment {
    /// Actionable iff there is at least one inbox item OR one non-gated PLAN
    /// item. This is the exact LOOP-001 predicate.
    pub fn has_actionable_work(&self) -> bool {
        self.inbox_items > 0 || self.actionable_plan_items > 0
    }
    /// True when nothing is actionable but user-gated work remains → Paused,
    /// not Idle.
    pub fn is_fully_user_gated(&self) -> bool {
        !self.has_actionable_work() && !self.user_gated_plan_items.is_empty()
    }
}

/// Detects actionable work for a project. Real impl reads the inbox + PLAN.md
/// via the existing `count_queued_prompts` / `open_roadmap_items` paths; the
/// test impl returns a scripted sequence so work can be made to "drain after
/// N cycles" deterministically.
pub trait WorkEvaluator: Send + Sync {
    fn assess(&self, project_dir: &Path) -> WorkAssessment;
}

// Convenience: any matching fn is a WorkEvaluator (lets tests pass a closure).
impl<F> WorkEvaluator for F
where
    F: Fn(&Path) -> WorkAssessment + Send + Sync,
{
    fn assess(&self, project_dir: &Path) -> WorkAssessment {
        self(project_dir)
    }
}

// ===========================================================================
// WorkflowRunner — spawning/observing/closing a workforce abstracted.
// ===========================================================================

/// Opaque handle to a spawned workforce run. Wraps the set of session ids the
/// run owns so the controller can close exactly those on advance. The real impl
/// stores `process_manager` sids; the mock stores synthetic ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkforceHandle {
    /// Index into the LoopSchedule.workflows this handle was spawned for.
    pub workforce_index: usize,
    /// Workforce name (schedule entry) — for diagnostics/notifications.
    pub workforce_name: String,
    /// Session ids owned by this run (from `spawn_with_engine`). Closed en
    /// masse on advance via `WorkflowRunner::close`.
    pub session_ids: Vec<String>,
    /// When the run was spawned (watchdog liveness baseline).
    pub spawned_at: Tstamp,
}

/// Folded completion state of a workforce run, derived deterministically from
/// `cleanup_summary.md` presence + `workflow_status.json` + session liveness.
/// The controller polls this every tick; it contains NO LLM judgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    /// Sessions still Working/Waiting; cleanup_summary.md not yet written.
    InProgress,
    /// `cleanup_summary.md` was written → workforce is done; advance.
    Completed,
    /// Workforce emitted a needs-input signal (workflow_status "paused" with a
    /// blocked marker) the loop must evaluate against autonomous-retry budget.
    NeedsInput { question: String },
    /// A session died/the run failed without completing. Triggers bounded retry.
    Failed { error: String },
}

/// Abstracts the real session machinery (`workflow_call` + `process_manager`'s
/// `spawn_with_engine` / `kill_session`) so the controller is mockable.
///
/// Implementations MUST be deterministic from the controller's perspective:
/// `spawn` returns a handle, `poll` folds the on-disk completion signal into a
/// `RunStatus`, and `close` tears down exactly that handle's sessions. NO method
/// calls an LLM.
pub trait WorkflowRunner: Send + Sync {
    /// Spawn the named workforce for `project_dir`. Real impl resolves the
    /// workforce md, compiles the dispatch script, and spawns ONE session per
    /// team (parallel-held until cleanup). Returns a handle owning the sids.
    fn spawn(
        &mut self,
        project_dir: &Path,
        workforce_index: usize,
        workforce_name: &str,
        now: Tstamp,
    ) -> Result<WorkforceHandle, RunnerError>;

    /// Fold the workforce's completion signal. Checks `cleanup_summary.md`
    /// presence, `workflow_status.json` status, and session liveness — purely
    /// deterministic. Called every tick while Running.
    fn poll(&self, handle: &WorkforceHandle, now: Tstamp) -> RunStatus;

    /// Close every session this handle owns (`kill_session` per sid). Idempotent.
    /// Called on AdvancingWorkforce and on teardown.
    fn close(&mut self, handle: &WorkforceHandle);

    /// Number of currently-live sessions across ALL runs this runner owns —
    /// used to enforce the per-project concurrency cap (TOK-002) before spawn.
    fn live_session_count(&self) -> usize;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerError {
    /// Spawning the next workforce would exceed the concurrency cap; the
    /// controller must DEFER, not launch (LOOP-002 acceptance).
    CapacityExceeded { cap: usize, live: usize },
    /// Workforce md missing / compilation failed.
    SpawnFailed(String),
}

// ===========================================================================
// Feedback signal — what resumes a Paused loop (LOOP-003 auto-resume).
// ===========================================================================

/// A piece of user feedback delivered to a paused loop. The controller decides
/// DETERMINISTICALLY whether it clears the current `BlockedReason` (e.g. new
/// inbox items invalidate `AllRemainingWorkUserGated`; a decision answer clears
/// `NeedsUserDecision`). No LLM reads the feedback — the caller (intake/COO)
/// has already turned raw text into inbox writes / a tagged resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvingFeedback {
    /// True if the feedback wrote new inbox items (re-arms actionable work).
    pub added_inbox_work: bool,
    /// Set if the feedback explicitly answers a `NeedsUserDecision` blocker.
    pub answers_decision_for: Option<String>,
}

// ===========================================================================
// Config — bounded retry budgets etc., injected so tests pin exact values.
// ===========================================================================

#[derive(Debug, Clone, Copy)]
pub struct LoopConfig {
    /// K — autonomous attempts (retry / re-spawn) before a NeedsInput or a
    /// Failed run is allowed to escalate to Paused (LOOP-003).
    pub max_autonomous_attempts: u32,
    /// Per-project concurrent live-session cap (TOK-002). Spawns beyond this
    /// are deferred, not launched (LOOP-002).
    pub concurrency_cap: usize,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            max_autonomous_attempts: 3,
            concurrency_cap: 4,
        }
    }
}

/// Bundles a schedule with the runtime config and the live cursor the
/// controller advances. The cursor wraps modulo `schedule.workflows.len()`.
#[derive(Debug, Clone)]
pub struct LoopInstance {
    pub schedule: LoopSchedule,
    pub config: LoopConfig,
    pub project_dir: PathBuf,
    /// Current position in `schedule.workflows`. Wraps to 0 past the end.
    pub cursor: usize,
}

impl LoopInstance {
    /// Construct an instance from a schedule, taking its `project_dir` and
    /// starting the cursor at 0.
    pub fn new(schedule: LoopSchedule, config: LoopConfig) -> Self {
        let project_dir = schedule.project_dir.clone();
        Self {
            schedule,
            config,
            project_dir,
            cursor: 0,
        }
    }
}

// ===========================================================================
// LoopController — the deterministic state machine for LOOP-001 + LOOP-003.
// ===========================================================================

pub struct LoopController<R: WorkflowRunner, E: WorkEvaluator, C: Clock> {
    inst: LoopInstance,
    runner: R,
    evaluator: E,
    clock: Arc<C>,

    state: LoopState,
    /// Handle for the workforce currently Running (None otherwise).
    active: Option<WorkforceHandle>,
    /// Autonomous attempts spent on the CURRENT cursor position since the last
    /// successful advance. Reset to 0 on Completed/advance. Compared against
    /// `config.max_autonomous_attempts` (K) before any Pause (LOOP-003).
    attempts: u32,
}

impl<R: WorkflowRunner, E: WorkEvaluator, C: Clock> LoopController<R, E, C> {
    pub fn new(inst: LoopInstance, runner: R, evaluator: E, clock: Arc<C>) -> Self {
        Self {
            inst,
            runner,
            evaluator,
            clock,
            state: LoopState::Idle,
            active: None,
            attempts: 0,
        }
    }

    pub fn state(&self) -> &LoopState {
        &self.state
    }
    pub fn cursor(&self) -> usize {
        self.inst.cursor
    }
    pub fn active_handle(&self) -> Option<&WorkforceHandle> {
        self.active.as_ref()
    }
    /// Read-only access to the runner (e.g. for the watchdog to read live
    /// session counts).
    pub fn runner(&self) -> &R {
        &self.runner
    }
    /// Read-only access to the instance (schedule + config + cursor).
    pub fn instance(&self) -> &LoopInstance {
        &self.inst
    }
    /// Autonomous attempts spent on the current cursor since the last advance.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Advance the state machine exactly one step. Returns the new state.
    /// Deterministic: identical (clock, evaluator, runner) inputs ⇒ identical
    /// transitions. Safe to call on a fixed cadence (the loop driver) OR
    /// event-driven; calling it when nothing changed is a no-op transition.
    pub fn tick(&mut self) -> LoopState {
        let now = self.clock.now();
        self.state = match std::mem::replace(&mut self.state, LoopState::Evaluating) {
            // --- Stopped is terminal; only external teardown leaves it. ------
            LoopState::Stopped => LoopState::Stopped,

            // --- Paused waits for resolving feedback (see `feed`). -----------
            LoopState::Paused { reason } => LoopState::Paused { reason },

            // --- Idle: keep re-evaluating; leave the instant work appears. ---
            LoopState::Idle => self.evaluate(now),

            // --- Evaluating: the decision hub. -------------------------------
            LoopState::Evaluating => self.evaluate(now),

            // --- Running: poll the workforce's completion signal. ------------
            LoopState::Running { workforce, since } => {
                let handle = self.active.as_ref().expect("Running implies active handle");
                match self.runner.poll(handle, now) {
                    RunStatus::InProgress => LoopState::Running { workforce, since },
                    RunStatus::Completed => {
                        // Clean advance — reset the autonomous-attempt budget.
                        self.attempts = 0;
                        LoopState::AdvancingWorkforce
                    }
                    RunStatus::NeedsInput { question } => {
                        // Workforce asked for input. Spend an autonomous attempt
                        // FIRST (re-spawn / alternative); only Pause after K.
                        self.attempts += 1;
                        if self.attempts < self.inst.config.max_autonomous_attempts {
                            self.recycle_active();
                            LoopState::Evaluating // re-attempt autonomously
                        } else {
                            self.pause(BlockedReason::NeedsUserDecision {
                                workforce: self.inst.schedule.workflows[self.inst.cursor].clone(),
                                question,
                                autonomous_attempts: self.attempts,
                            })
                        }
                    }
                    RunStatus::Failed { error } => {
                        self.attempts += 1;
                        if self.attempts < self.inst.config.max_autonomous_attempts {
                            self.recycle_active(); // bounded retry (LOOP-002)
                            LoopState::Evaluating
                        } else {
                            self.pause(BlockedReason::RetriesExhausted {
                                workforce: self.inst.schedule.workflows[self.inst.cursor].clone(),
                                attempts: self.attempts,
                                last_error: error,
                            })
                        }
                    }
                }
            }

            // --- AdvancingWorkforce: close sessions, bump+wrap cursor. --------
            LoopState::AdvancingWorkforce => {
                if let Some(handle) = self.active.take() {
                    self.runner.close(&handle); // kill exactly this run's sids
                }
                let n = self.inst.schedule.workflows.len().max(1);
                self.inst.cursor = (self.inst.cursor + 1) % n; // wrap to repeat
                self.evaluate(now)
            }
        };
        self.state.clone()
    }

    // ---- decision hub: actionable-work + capacity gating -------------------

    fn evaluate(&mut self, now: Tstamp) -> LoopState {
        // Disabled schedule ⇒ idle (NOT stopped — re-enable resumes work).
        if !self.inst.schedule.enabled || self.inst.schedule.workflows.is_empty() {
            return LoopState::Idle;
        }
        let work = self.evaluator.assess(&self.inst.project_dir);

        if !work.has_actionable_work() {
            // Distinguish "nothing to do" (Idle) from "only user-gated work
            // left" (Paused) — LOOP-003. Idle is NOT terminal; Paused is parked.
            if work.is_fully_user_gated() {
                return self.pause(BlockedReason::AllRemainingWorkUserGated {
                    pending_plan_items: work.user_gated_plan_items,
                });
            }
            return LoopState::Idle;
        }

        // There IS actionable work — spawn the current workforce, respecting
        // the per-project concurrency cap (TOK-002 / LOOP-002).
        if self.runner.live_session_count() >= self.inst.config.concurrency_cap {
            // Defer, do NOT launch the N+1th. Stay Evaluating; next tick retries
            // once capacity frees. (Idle would also be valid; Evaluating keeps
            // the loop hot so it grabs the slot the instant it opens.)
            return LoopState::Evaluating;
        }

        let idx = self.inst.cursor;
        let name = self.inst.schedule.workflows[idx].clone();
        match self.runner.spawn(&self.inst.project_dir, idx, &name, now) {
            Ok(handle) => {
                self.active = Some(handle);
                LoopState::Running {
                    workforce: idx,
                    since: now,
                }
            }
            Err(RunnerError::CapacityExceeded { .. }) => LoopState::Evaluating, // defer
            Err(RunnerError::SpawnFailed(error)) => {
                self.attempts += 1;
                if self.attempts < self.inst.config.max_autonomous_attempts {
                    LoopState::Evaluating // retry spawn next tick
                } else {
                    self.pause(BlockedReason::RetriesExhausted {
                        workforce: name,
                        attempts: self.attempts,
                        last_error: error,
                    })
                }
            }
        }
    }

    /// Close the current run's sessions so an autonomous re-attempt starts
    /// clean. Does NOT advance the cursor (same workforce retried).
    fn recycle_active(&mut self) {
        if let Some(handle) = self.active.take() {
            self.runner.close(&handle);
        }
    }

    /// Park on the user. Tears down any live run; the blocker is surfaced by the
    /// driver (Hypervise + PushNotification). NEVER Stops.
    fn pause(&mut self, reason: BlockedReason) -> LoopState {
        if let Some(handle) = self.active.take() {
            self.runner.close(&handle);
        }
        LoopState::Paused { reason }
    }

    // ---- external inputs ---------------------------------------------------

    /// Deliver resolving feedback to a paused loop (LOOP-003 auto-resume).
    /// Deterministically decides whether the feedback clears the current
    /// blocker; if so, resets attempts and returns to Evaluating. A no-op on a
    /// non-paused loop.
    pub fn feed(&mut self, fb: &ResolvingFeedback) -> LoopState {
        let LoopState::Paused { reason } = &self.state else {
            return self.state.clone();
        };
        let resolved = match reason {
            BlockedReason::AllRemainingWorkUserGated { .. } => fb.added_inbox_work,
            BlockedReason::NeedsUserDecision { question, .. } => {
                fb.answers_decision_for.as_deref() == Some(question.as_str()) || fb.added_inbox_work
            }
            BlockedReason::RetriesExhausted { .. } => fb.added_inbox_work,
        };
        if resolved {
            self.attempts = 0;
            self.state = LoopState::Evaluating;
        }
        self.state.clone()
    }

    /// Explicit teardown (schedule disabled / shutdown). Closes the live run and
    /// moves to the terminal Stopped state. This is the ONLY path to Stopped.
    pub fn stop(&mut self) -> LoopState {
        if let Some(handle) = self.active.take() {
            self.runner.close(&handle);
        }
        self.state = LoopState::Stopped;
        self.state.clone()
    }
}

// ===========================================================================
// Tests — MockWorkflowRunner + ManualClock + scripted WorkEvaluator.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn schedule(workflows: &[&str]) -> LoopSchedule {
        let mut s = LoopSchedule::new(
            "test loop",
            PathBuf::from("/tmp/proj"),
            workflows.iter().map(|w| w.to_string()).collect(),
        );
        s.enabled = true;
        s
    }

    fn instance(workflows: &[&str], cfg: LoopConfig) -> LoopInstance {
        LoopInstance::new(schedule(workflows), cfg)
    }

    // -- scripted evaluator: returns a queue of assessments; last repeats -----
    struct ScriptedEval {
        seq: Mutex<std::collections::VecDeque<WorkAssessment>>,
        last: WorkAssessment,
    }
    impl ScriptedEval {
        fn new(seq: Vec<WorkAssessment>) -> Self {
            let last = seq.last().cloned().unwrap_or_default();
            Self {
                seq: Mutex::new(seq.into()),
                last,
            }
        }
    }
    impl WorkEvaluator for ScriptedEval {
        fn assess(&self, _project_dir: &Path) -> WorkAssessment {
            let mut q = self.seq.lock().unwrap();
            if q.len() > 1 {
                q.pop_front().unwrap()
            } else {
                q.front().cloned().unwrap_or_else(|| self.last.clone())
            }
        }
    }

    // -- programmable runner -------------------------------------------------
    #[derive(Default)]
    struct RunnerInner {
        live: usize,
        spawns: usize,
        closes: usize,
        spawn_log: Vec<usize>, // workforce indices spawned, in order
        next_sid: usize,
        // poll script: index n returns scripts[min(n, len-1)] for the CURRENT
        // handle. Reset each spawn so a re-spawn replays from the top.
        poll_script: Vec<RunStatus>,
        poll_calls: usize,
        spawn_should_fail: Option<String>,
    }

    struct MockRunner {
        inner: RefCell<RunnerInner>,
        cap: usize,
    }
    impl MockRunner {
        fn new(poll_script: Vec<RunStatus>) -> Self {
            Self {
                inner: RefCell::new(RunnerInner {
                    poll_script,
                    ..Default::default()
                }),
                cap: usize::MAX,
            }
        }
        fn spawns(&self) -> usize {
            self.inner.borrow().spawns
        }
        fn closes(&self) -> usize {
            self.inner.borrow().closes
        }
        fn spawn_log(&self) -> Vec<usize> {
            self.inner.borrow().spawn_log.clone()
        }
    }
    // SAFETY: tests are single-threaded; RefCell guarded by !Sync would block the
    // Send + Sync bound on WorkflowRunner. Use a Mutex-free wrapper via unsafe is
    // overkill — instead wrap in a Mutex below. To keep the trait bounds simple
    // we implement Send + Sync manually since the test driver is single-threaded.
    unsafe impl Send for MockRunner {}
    unsafe impl Sync for MockRunner {}

    impl WorkflowRunner for MockRunner {
        fn spawn(
            &mut self,
            _project_dir: &Path,
            workforce_index: usize,
            workforce_name: &str,
            now: Tstamp,
        ) -> Result<WorkforceHandle, RunnerError> {
            let mut inner = self.inner.borrow_mut();
            if inner.live >= self.cap {
                return Err(RunnerError::CapacityExceeded {
                    cap: self.cap,
                    live: inner.live,
                });
            }
            if let Some(e) = inner.spawn_should_fail.clone() {
                return Err(RunnerError::SpawnFailed(e));
            }
            inner.spawns += 1;
            inner.spawn_log.push(workforce_index);
            inner.poll_calls = 0; // re-spawn replays the poll script
            inner.live += 1;
            let sid = inner.next_sid;
            inner.next_sid += 1;
            Ok(WorkforceHandle {
                workforce_index,
                workforce_name: workforce_name.to_string(),
                session_ids: vec![format!("sid-{sid}")],
                spawned_at: now,
            })
        }

        fn poll(&self, _handle: &WorkforceHandle, _now: Tstamp) -> RunStatus {
            let mut inner = self.inner.borrow_mut();
            let n = inner.poll_calls;
            inner.poll_calls += 1;
            if inner.poll_script.is_empty() {
                return RunStatus::InProgress;
            }
            let idx = n.min(inner.poll_script.len() - 1);
            inner.poll_script[idx].clone()
        }

        fn close(&mut self, _handle: &WorkforceHandle) {
            let mut inner = self.inner.borrow_mut();
            inner.closes += 1;
            inner.live = inner.live.saturating_sub(1);
        }

        fn live_session_count(&self) -> usize {
            self.inner.borrow().live
        }
    }

    fn work(inbox: usize, plan: usize) -> WorkAssessment {
        WorkAssessment {
            inbox_items: inbox,
            actionable_plan_items: plan,
            user_gated_plan_items: vec![],
        }
    }

    // ---------------------------------------------------------------------
    // (1) LOOP-001: work drains after N cycles. Controller spawns, advances
    //     through each workforce, repeats, and reaches Idle EXACTLY when work
    //     is exhausted — with no spurious re-spawn.
    // ---------------------------------------------------------------------
    #[test]
    fn loop001_walks_schedule_then_idles_on_drain() {
        let clock = Arc::new(ManualClock::new());
        // 2-workforce schedule. Each run completes one tick after spawn.
        // Work present for the first 4 spawns, then drains.
        // Evaluator returns actionable work until we've spawned 4 workforces.
        let inst = instance(&["wf_a", "wf_b"], LoopConfig::default());

        // poll: always Completed on the first poll after spawn.
        let runner = MockRunner::new(vec![RunStatus::Completed]);

        // Evaluator: actionable for first N assessments, then drains.
        // We give 4 "has work" assessments then default (no work).
        let eval = ScriptedEval::new(vec![
            work(1, 0),
            work(1, 0),
            work(1, 0),
            work(1, 0),
            WorkAssessment::default(),
        ]);

        let mut c = LoopController::new(inst, runner, eval, clock);

        // Drive ticks and record state transitions.
        let mut states = vec![];
        for _ in 0..40 {
            let s = c.tick();
            states.push(s.clone());
            if s == LoopState::Idle {
                // give one more tick to ensure it STAYS idle (no re-spawn)
                let s2 = c.tick();
                states.push(s2.clone());
                break;
            }
        }

        // Reached Idle.
        assert_eq!(c.state(), &LoopState::Idle, "should idle on drain");
        // Exactly 4 spawns (one per actionable assessment), no more.
        assert_eq!(c.runner().spawns(), 4, "exactly 4 workforces spawned");
        // Each completed run closed its sessions on advance.
        assert_eq!(c.runner().closes(), 4, "each run closed on advance");
        // Cursor walked the schedule and wrapped: spawn order 0,1,0,1.
        assert_eq!(c.runner().spawn_log(), vec![0, 1, 0, 1], "wrapped schedule");
        // No live sessions while idle.
        assert_eq!(
            c.runner().live_session_count(),
            0,
            "no live session at idle"
        );
    }

    // ---------------------------------------------------------------------
    // (2) Blocked-on-user => Paused{reason}, NOT Stopped. Then resolving
    //     feedback => Evaluating.
    // ---------------------------------------------------------------------
    #[test]
    fn loop003_pauses_on_user_gated_then_resumes_on_feedback() {
        let clock = Arc::new(ManualClock::new());
        let inst = instance(&["wf_a"], LoopConfig::default());
        let runner = MockRunner::new(vec![RunStatus::InProgress]);

        // First assessment: no actionable work, only user-gated PLAN items.
        let eval = ScriptedEval::new(vec![WorkAssessment {
            inbox_items: 0,
            actionable_plan_items: 0,
            user_gated_plan_items: vec!["decide auth model".into()],
        }]);

        let mut c = LoopController::new(inst, runner, eval, clock);
        let s = c.tick();

        match s {
            LoopState::Paused {
                reason: BlockedReason::AllRemainingWorkUserGated { pending_plan_items },
            } => {
                assert_eq!(pending_plan_items, vec!["decide auth model".to_string()]);
            }
            other => panic!("expected Paused user-gated, got {other:?}"),
        }
        assert!(c.state().is_paused());
        assert_ne!(c.state(), &LoopState::Stopped, "must NOT be Stopped");
        // No workforce was spawned (nothing actionable).
        assert_eq!(c.runner().spawns(), 0);

        // Resolving feedback adds inbox work → re-arms actionable work.
        let s2 = c.feed(&ResolvingFeedback {
            added_inbox_work: true,
            answers_decision_for: None,
        });
        assert_eq!(s2, LoopState::Evaluating, "feedback resumes to Evaluating");
        assert_eq!(c.attempts(), 0, "attempts reset on resume");
    }

    // ---------------------------------------------------------------------
    // (3) A workforce that fails then succeeds => K-1 autonomous retries
    //     BEFORE any pause, and never pauses if it eventually completes.
    // ---------------------------------------------------------------------
    #[test]
    fn loop003_retries_k_minus_1_before_pause_on_failure() {
        let clock = Arc::new(ManualClock::new());
        let cfg = LoopConfig {
            max_autonomous_attempts: 3,
            concurrency_cap: 4,
        };
        let inst = instance(&["wf_a"], cfg);

        // Each spawned run's first poll = Failed. The runner replays this script
        // from the top on every re-spawn (poll_calls reset in spawn). So every
        // attempt fails on first poll. After K attempts, controller pauses.
        let runner = MockRunner::new(vec![RunStatus::Failed {
            error: "session died".into(),
        }]);
        // Always-actionable work so it keeps re-attempting.
        let eval = ScriptedEval::new(vec![work(1, 0)]);

        let mut c = LoopController::new(inst, runner, eval, clock);

        // Tick until paused, counting spawns.
        let mut paused = None;
        for _ in 0..50 {
            let s = c.tick();
            if let LoopState::Paused { reason } = &s {
                paused = Some(reason.clone());
                break;
            }
        }

        let reason = paused.expect("must pause after K failures");
        match reason {
            BlockedReason::RetriesExhausted {
                attempts,
                last_error,
                ..
            } => {
                assert_eq!(attempts, 3, "K=3 attempts recorded");
                assert_eq!(last_error, "session died");
            }
            other => panic!("expected RetriesExhausted, got {other:?}"),
        }
        // K=3 spawns happened (attempt 1, 2, 3 each spawned then failed).
        assert_eq!(c.runner().spawns(), 3, "exactly K spawns before pause");
    }

    #[test]
    fn loop003_fails_then_succeeds_does_not_pause() {
        let clock = Arc::new(ManualClock::new());
        let cfg = LoopConfig {
            max_autonomous_attempts: 3,
            concurrency_cap: 4,
        };
        let inst = instance(&["wf_a"], cfg);

        // The runner script: each handle fails on first poll. We want
        // "fails then succeeds": attempt 1 fails (re-spawn), attempt 2 the
        // run completes. Use a runner whose per-spawn script differs by spawn
        // count. Implement via a script that's [Failed] for spawn#1 and
        // [Completed] for spawn#2 — drive by tracking spawns in the script.
        // Simpler: make first spawn fail, subsequent spawns complete.
        struct FailThenOk {
            inner: RefCell<(usize, usize, usize)>, // (spawns, closes, live)
        }
        unsafe impl Send for FailThenOk {}
        unsafe impl Sync for FailThenOk {}
        impl WorkflowRunner for FailThenOk {
            fn spawn(
                &mut self,
                _p: &Path,
                workforce_index: usize,
                workforce_name: &str,
                now: Tstamp,
            ) -> Result<WorkforceHandle, RunnerError> {
                let mut b = self.inner.borrow_mut();
                b.0 += 1;
                b.2 += 1;
                Ok(WorkforceHandle {
                    workforce_index,
                    workforce_name: workforce_name.into(),
                    session_ids: vec!["sid".into()],
                    spawned_at: now,
                })
            }
            fn poll(&self, _h: &WorkforceHandle, _n: Tstamp) -> RunStatus {
                // Spawn #1 → Failed; spawn #2+ → Completed.
                let spawns = self.inner.borrow().0;
                if spawns <= 1 {
                    RunStatus::Failed {
                        error: "transient".into(),
                    }
                } else {
                    RunStatus::Completed
                }
            }
            fn close(&mut self, _h: &WorkforceHandle) {
                let mut b = self.inner.borrow_mut();
                b.1 += 1;
                b.2 = b.2.saturating_sub(1);
            }
            fn live_session_count(&self) -> usize {
                self.inner.borrow().2
            }
        }

        let runner = FailThenOk {
            inner: RefCell::new((0, 0, 0)),
        };
        // Drains after a couple cycles so it eventually idles, never pauses.
        let eval = ScriptedEval::new(vec![
            work(1, 0),
            work(1, 0),
            work(1, 0),
            WorkAssessment::default(),
        ]);

        let mut c = LoopController::new(inst, runner, eval, clock);

        let mut saw_paused = false;
        let mut saw_idle = false;
        for _ in 0..40 {
            let s = c.tick();
            if s.is_paused() {
                saw_paused = true;
                break;
            }
            if s == LoopState::Idle {
                saw_idle = true;
                break;
            }
        }
        assert!(!saw_paused, "must NOT pause — failure was transient");
        assert!(saw_idle, "should reach Idle after work drains");
    }

    // ---------------------------------------------------------------------
    // Extra: explicit stop() is the ONLY path to Stopped.
    // ---------------------------------------------------------------------
    #[test]
    fn stop_is_only_path_to_stopped() {
        let clock = Arc::new(ManualClock::new());
        let inst = instance(&["wf_a"], LoopConfig::default());
        let runner = MockRunner::new(vec![RunStatus::InProgress]);
        let eval = ScriptedEval::new(vec![work(1, 0)]);
        let mut c = LoopController::new(inst, runner, eval, clock);

        c.tick(); // Running
        assert!(matches!(c.state(), LoopState::Running { .. }));
        let s = c.stop();
        assert_eq!(s, LoopState::Stopped);
        // The active run's session was closed by stop.
        assert_eq!(c.runner().closes(), 1);
        // tick() on Stopped is a no-op.
        assert_eq!(c.tick(), LoopState::Stopped);
    }
}
