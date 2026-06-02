// crates/orrch-core/src/loop_watchdog.rs
//
// LoopWatchdog — LOOP-004. A SEPARATE supervisor that observes loop/session
// health independent of the controller and emits interventions. It does NOT
// drive state; it inspects an immutable view and returns a verdict. Pure,
// deterministic, Clock-injected → testable with simulated inputs. No LLM.

use std::collections::HashMap;

use crate::loop_controller::{Clock, Tstamp};

// ---------------------------------------------------------------------------
// Inputs — a snapshot view the driver assembles from process_manager + loops.
// ---------------------------------------------------------------------------

/// Per-session liveness facts, sampled from `Session.last_output_time` and the
/// session's spawn/die history. `last_output` is the controller's `Tstamp` of
/// the most recent Output event (None = never produced output).
#[derive(Debug, Clone)]
pub struct SessionView {
    pub sid: String,
    pub last_output: Option<Tstamp>,
    pub spawned_at: Tstamp,
    /// True if the session is currently Dead (waitpid reaped) but the loop has
    /// not yet advanced past it — a candidate for thrash accounting.
    pub dead: bool,
}

/// Per-loop progress + lifecycle facts the driver accumulates across ticks.
#[derive(Debug, Clone)]
pub struct LoopView {
    pub loop_id: String,
    pub sessions: Vec<SessionView>,
    /// Spawns and deaths observed within the watchdog's rolling window. Used
    /// for thrash detection (spawn→die without progress).
    pub spawns_in_window: u32,
    pub deaths_in_window: u32,
    /// Progress markers: PLAN items checked + commits since the watchdog last
    /// saw progress. Zero across M cycles ⇒ stuck.
    pub plan_items_checked_in_window: u32,
    pub commits_in_window: u32,
    /// How many consecutive watchdog windows have elapsed with no progress.
    pub no_progress_windows: u32,
}

/// Whole-system view fed to the watchdog each evaluation.
#[derive(Debug, Clone)]
pub struct WatchdogView {
    pub loops: Vec<LoopView>,
}

// ---------------------------------------------------------------------------
// Thresholds — injected so tests pin exact trigger points.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct WatchdogConfig {
    /// T — a live session with no output for longer than this is Hung.
    pub liveness_timeout: std::time::Duration,
    /// M — consecutive no-progress windows before a loop is Stuck.
    pub stuck_windows: u32,
    /// Thrash trigger: spawn/die count in-window at or above this with zero
    /// progress ⇒ restart-storm.
    pub thrash_spawn_deaths: u32,
    /// Exponential back-off base (doubled per consecutive intervention).
    pub backoff_base: std::time::Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            liveness_timeout: std::time::Duration::from_secs(600), // 10 min
            stuck_windows: 5,
            thrash_spawn_deaths: 6,
            backoff_base: std::time::Duration::from_secs(30),
        }
    }
}

// ---------------------------------------------------------------------------
// Outputs — classification + interventions.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopHealth {
    Healthy,
    /// A session has gone silent past the liveness timeout (sid + stale-for ns).
    Hung { sid: String, stale_nanos: u128 },
    /// No PLAN/commit progress across M windows.
    Stuck { no_progress_windows: u32 },
    /// Spawn→die churn without progress (restart storm / retry storm).
    Thrash { spawns: u32, deaths: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intervention {
    /// Kill the named hung session and let the controller re-spawn (LOOP-004).
    KillRestart { loop_id: String, sid: String, reason: String },
    /// Apply exponential back-off before the loop may spawn again.
    BackOff { loop_id: String, wait: std::time::Duration, reason: String },
    /// Pause the loop with a diagnosed fault (drives controller → Paused).
    PauseWithFault { loop_id: String, fault: String },
    /// Surface to the user — beyond autonomous remediation.
    Escalate { loop_id: String, fault: String },
}

// ---------------------------------------------------------------------------
// Watchdog — pure classifier. Holds only the per-loop back-off counter (its
// own minimal state); everything else comes from the injected view + clock.
// ---------------------------------------------------------------------------

pub struct LoopWatchdog<C: Clock> {
    config: WatchdogConfig,
    clock: std::sync::Arc<C>,
    /// Consecutive interventions per loop → drives exponential back-off and the
    /// escalation ladder (BackOff → PauseWithFault → Escalate).
    intervention_streak: HashMap<String, u32>,
}

impl<C: Clock> LoopWatchdog<C> {
    pub fn new(config: WatchdogConfig, clock: std::sync::Arc<C>) -> Self {
        Self { config, clock, intervention_streak: HashMap::new() }
    }

    /// Classify one loop's health from its view + the current clock. Pure given
    /// (view, now, config). Liveness wins over thrash wins over stuck.
    pub fn classify(&self, lv: &LoopView) -> LoopHealth {
        let now = self.clock.now();
        // Liveness: any live (non-dead) session silent past T.
        let mut worst: Option<(String, u128)> = None;
        for s in &lv.sessions {
            if s.dead { continue; }
            let last = s.last_output.unwrap_or(s.spawned_at);
            let stale = now.0.saturating_sub(last.0);
            if stale > self.config.liveness_timeout.as_nanos()
                && worst.as_ref().map_or(true, |(_, w)| stale > *w)
            {
                worst = Some((s.sid.clone(), stale));
            }
        }
        if let Some((sid, stale_nanos)) = worst {
            return LoopHealth::Hung { sid, stale_nanos };
        }
        // Thrash: spawn/die churn with zero progress in-window.
        let churn = lv.spawns_in_window + lv.deaths_in_window;
        if churn >= self.config.thrash_spawn_deaths
            && lv.plan_items_checked_in_window == 0
            && lv.commits_in_window == 0
        {
            return LoopHealth::Thrash { spawns: lv.spawns_in_window, deaths: lv.deaths_in_window };
        }
        // Stuck: M consecutive no-progress windows.
        if lv.no_progress_windows >= self.config.stuck_windows {
            return LoopHealth::Stuck { no_progress_windows: lv.no_progress_windows };
        }
        LoopHealth::Healthy
    }

    /// Evaluate every loop and emit interventions. Mutates only the back-off
    /// streak counter. Returns one intervention per unhealthy loop (healthy ⇒
    /// none — the watchdog's own liveness is its heartbeat of having run).
    pub fn evaluate(&mut self, view: &WatchdogView) -> Vec<Intervention> {
        let mut out = Vec::new();
        for lv in &view.loops {
            let health = self.classify(lv);
            let intervention = match health {
                LoopHealth::Healthy => {
                    self.intervention_streak.remove(&lv.loop_id); // reset ladder
                    None
                }
                LoopHealth::Hung { sid, stale_nanos } => {
                    // First remediation for a hung session: kill+restart.
                    Some(Intervention::KillRestart {
                        loop_id: lv.loop_id.clone(),
                        sid,
                        reason: format!("no output for {}s", stale_nanos / 1_000_000_000),
                    })
                }
                LoopHealth::Thrash { spawns, deaths } => {
                    // Escalation ladder per consecutive intervention:
                    //   1 → BackOff, 2 → PauseWithFault, ≥3 → Escalate.
                    let streak = self.bump(&lv.loop_id);
                    let fault = format!("restart storm: {spawns} spawn / {deaths} die, no progress");
                    match streak {
                        1 => Some(Intervention::BackOff {
                            loop_id: lv.loop_id.clone(),
                            wait: self.backoff(streak),
                            reason: fault,
                        }),
                        2 => Some(Intervention::PauseWithFault { loop_id: lv.loop_id.clone(), fault }),
                        _ => Some(Intervention::Escalate { loop_id: lv.loop_id.clone(), fault }),
                    }
                }
                LoopHealth::Stuck { no_progress_windows } => {
                    let streak = self.bump(&lv.loop_id);
                    let fault = format!("no progress across {no_progress_windows} windows");
                    if streak <= 1 {
                        Some(Intervention::BackOff { loop_id: lv.loop_id.clone(), wait: self.backoff(streak), reason: fault })
                    } else {
                        Some(Intervention::PauseWithFault { loop_id: lv.loop_id.clone(), fault })
                    }
                }
            };
            if let Some(i) = intervention { out.push(i); }
        }
        out
    }

    fn bump(&mut self, loop_id: &str) -> u32 {
        let e = self.intervention_streak.entry(loop_id.to_string()).or_insert(0);
        *e += 1;
        *e
    }

    /// Exponential back-off: base * 2^(streak-1).
    fn backoff(&self, streak: u32) -> std::time::Duration {
        self.config.backoff_base * 2u32.pow(streak.saturating_sub(1).min(10))
    }
}

// ---------------------------------------------------------------------------
// LOOP-004 acceptance tests — ManualClock + simulated inputs.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loop_controller::ManualClock;
    use std::sync::Arc;
    use std::time::Duration;

    fn clock_at(nanos: u64) -> Arc<ManualClock> {
        let c = Arc::new(ManualClock::new());
        c.advance(Duration::from_nanos(nanos));
        c
    }

    fn healthy_session(sid: &str, last_output: Tstamp) -> SessionView {
        SessionView { sid: sid.into(), last_output: Some(last_output), spawned_at: Tstamp(0), dead: false }
    }

    // (1) A live session whose last output is older than the liveness timeout
    //     => Hung => evaluate yields KillRestart for that sid.
    #[test]
    fn stale_session_is_hung_and_killed_restarted() {
        let cfg = WatchdogConfig::default(); // liveness_timeout = 600s
        // now = liveness + 1s. last_output = 0 => stale = liveness + 1s > T.
        let now_ns = cfg.liveness_timeout.as_nanos() as u64 + Duration::from_secs(1).as_nanos() as u64;
        let clock = clock_at(now_ns);
        let mut wd = LoopWatchdog::new(cfg, clock);

        let lv = LoopView {
            loop_id: "L1".into(),
            sessions: vec![healthy_session("S1", Tstamp(0))],
            spawns_in_window: 0,
            deaths_in_window: 0,
            plan_items_checked_in_window: 0,
            commits_in_window: 0,
            no_progress_windows: 0,
        };

        // classify
        match wd.classify(&lv) {
            LoopHealth::Hung { ref sid, stale_nanos } => {
                assert_eq!(sid, "S1");
                assert!(stale_nanos > cfg.liveness_timeout.as_nanos());
            }
            other => panic!("expected Hung, got {other:?}"),
        }

        // evaluate => KillRestart
        let out = wd.evaluate(&WatchdogView { loops: vec![lv] });
        assert_eq!(out.len(), 1);
        match &out[0] {
            Intervention::KillRestart { loop_id, sid, reason } => {
                assert_eq!(loop_id, "L1");
                assert_eq!(sid, "S1");
                assert!(reason.contains("no output"));
            }
            other => panic!("expected KillRestart, got {other:?}"),
        }
    }

    // A live session whose last output is recent (within T) is NOT hung.
    #[test]
    fn fresh_session_not_hung() {
        let cfg = WatchdogConfig::default();
        let now_ns = cfg.liveness_timeout.as_nanos() as u64 + Duration::from_secs(1).as_nanos() as u64;
        let clock = clock_at(now_ns);
        let wd = LoopWatchdog::new(cfg, clock);

        // last_output 1ns ago.
        let lv = LoopView {
            loop_id: "L1".into(),
            sessions: vec![healthy_session("S1", Tstamp((now_ns - 1) as u128))],
            spawns_in_window: 0,
            deaths_in_window: 0,
            plan_items_checked_in_window: 0,
            commits_in_window: 0,
            no_progress_windows: 0,
        };
        assert_eq!(wd.classify(&lv), LoopHealth::Healthy);
    }

    // A dead session never triggers liveness (excluded from the scan).
    #[test]
    fn dead_session_excluded_from_liveness() {
        let cfg = WatchdogConfig::default();
        let now_ns = cfg.liveness_timeout.as_nanos() as u64 + Duration::from_secs(1).as_nanos() as u64;
        let clock = clock_at(now_ns);
        let wd = LoopWatchdog::new(cfg, clock);

        let mut s = healthy_session("S1", Tstamp(0)); // very stale...
        s.dead = true; // ...but dead, so not Hung
        let lv = LoopView {
            loop_id: "L1".into(),
            sessions: vec![s],
            spawns_in_window: 0,
            deaths_in_window: 0,
            plan_items_checked_in_window: 0,
            commits_in_window: 0,
            no_progress_windows: 0,
        };
        assert_eq!(wd.classify(&lv), LoopHealth::Healthy);
    }

    // (2) N spawn/die within the window with zero progress => Thrash.
    //     First consecutive intervention = BackOff (with fault reason),
    //     second consecutive = PauseWithFault, third = Escalate.
    #[test]
    fn thrash_escalation_ladder() {
        let cfg = WatchdogConfig::default(); // thrash_spawn_deaths = 6, backoff_base = 30s
        let clock = clock_at(0);
        let mut wd = LoopWatchdog::new(cfg, clock);

        let thrash = |id: &str| LoopView {
            loop_id: id.into(),
            sessions: vec![], // no live sessions => liveness can't fire
            spawns_in_window: 4,
            deaths_in_window: 4, // churn = 8 >= 6
            plan_items_checked_in_window: 0,
            commits_in_window: 0,
            no_progress_windows: 0,
        };

        // classify => Thrash
        match wd.classify(&thrash("L1")) {
            LoopHealth::Thrash { spawns, deaths } => {
                assert_eq!(spawns, 4);
                assert_eq!(deaths, 4);
            }
            other => panic!("expected Thrash, got {other:?}"),
        }

        // 1st evaluate => BackOff carrying a fault reason, wait = base * 2^0.
        let out = wd.evaluate(&WatchdogView { loops: vec![thrash("L1")] });
        match &out[0] {
            Intervention::BackOff { loop_id, wait, reason } => {
                assert_eq!(loop_id, "L1");
                assert_eq!(*wait, cfg.backoff_base);
                assert!(reason.contains("restart storm"));
            }
            other => panic!("expected BackOff, got {other:?}"),
        }

        // 2nd consecutive => PauseWithFault.
        let out = wd.evaluate(&WatchdogView { loops: vec![thrash("L1")] });
        match &out[0] {
            Intervention::PauseWithFault { loop_id, fault } => {
                assert_eq!(loop_id, "L1");
                assert!(fault.contains("restart storm"));
            }
            other => panic!("expected PauseWithFault, got {other:?}"),
        }

        // 3rd consecutive => Escalate.
        let out = wd.evaluate(&WatchdogView { loops: vec![thrash("L1")] });
        match &out[0] {
            Intervention::Escalate { loop_id, .. } => assert_eq!(loop_id, "L1"),
            other => panic!("expected Escalate, got {other:?}"),
        }
    }

    // Thrash requires ZERO progress: any commit/plan-item in-window suppresses it.
    #[test]
    fn churn_with_progress_is_not_thrash() {
        let cfg = WatchdogConfig::default();
        let clock = clock_at(0);
        let wd = LoopWatchdog::new(cfg, clock);

        let lv = LoopView {
            loop_id: "L1".into(),
            sessions: vec![],
            spawns_in_window: 5,
            deaths_in_window: 5, // high churn...
            plan_items_checked_in_window: 1, // ...but progress happened
            commits_in_window: 0,
            no_progress_windows: 0,
        };
        assert_eq!(wd.classify(&lv), LoopHealth::Healthy);
    }

    // (3) A healthy, progressing loop => no intervention.
    #[test]
    fn healthy_loop_yields_no_intervention() {
        let cfg = WatchdogConfig::default();
        let now_ns = Duration::from_secs(1000).as_nanos() as u64;
        let clock = clock_at(now_ns);
        let mut wd = LoopWatchdog::new(cfg, clock);

        let lv = LoopView {
            loop_id: "L1".into(),
            sessions: vec![healthy_session("S1", Tstamp(now_ns as u128))], // just produced output
            spawns_in_window: 1,
            deaths_in_window: 0,
            plan_items_checked_in_window: 2,
            commits_in_window: 1,
            no_progress_windows: 0,
        };
        assert_eq!(wd.classify(&lv), LoopHealth::Healthy);
        let out = wd.evaluate(&WatchdogView { loops: vec![lv] });
        assert!(out.is_empty(), "healthy loop must emit no interventions, got {out:?}");
    }

    // Stuck: M consecutive no-progress windows => BackOff then PauseWithFault.
    #[test]
    fn stuck_after_m_windows() {
        let cfg = WatchdogConfig::default(); // stuck_windows = 5
        let clock = clock_at(1000);
        let mut wd = LoopWatchdog::new(cfg, clock);

        let stuck = |id: &str| LoopView {
            loop_id: id.into(),
            sessions: vec![healthy_session("S1", Tstamp(1000))], // fresh output, not hung
            spawns_in_window: 0,
            deaths_in_window: 0,
            plan_items_checked_in_window: 0,
            commits_in_window: 0,
            no_progress_windows: 5, // >= M
        };

        assert_eq!(
            wd.classify(&stuck("L1")),
            LoopHealth::Stuck { no_progress_windows: 5 }
        );

        let out = wd.evaluate(&WatchdogView { loops: vec![stuck("L1")] });
        assert!(matches!(out[0], Intervention::BackOff { .. }));
        let out = wd.evaluate(&WatchdogView { loops: vec![stuck("L1")] });
        assert!(matches!(out[0], Intervention::PauseWithFault { .. }));
    }

    // Healthy classification resets the escalation ladder.
    #[test]
    fn healthy_resets_streak() {
        let cfg = WatchdogConfig::default();
        let clock = clock_at(1000);
        let mut wd = LoopWatchdog::new(cfg, clock);

        let stuck = LoopView {
            loop_id: "L1".into(),
            sessions: vec![healthy_session("S1", Tstamp(1000))],
            spawns_in_window: 0,
            deaths_in_window: 0,
            plan_items_checked_in_window: 0,
            commits_in_window: 0,
            no_progress_windows: 5,
        };
        let healthy = LoopView {
            loop_id: "L1".into(),
            sessions: vec![healthy_session("S1", Tstamp(1000))],
            spawns_in_window: 0,
            deaths_in_window: 0,
            plan_items_checked_in_window: 1,
            commits_in_window: 0,
            no_progress_windows: 0,
        };

        // streak -> 1 (BackOff)
        assert!(matches!(
            wd.evaluate(&WatchdogView { loops: vec![stuck.clone()] })[0],
            Intervention::BackOff { .. }
        ));
        // healthy resets streak
        assert!(wd.evaluate(&WatchdogView { loops: vec![healthy] }).is_empty());
        // back to stuck => streak restarts at 1 => BackOff again, not Pause
        assert!(matches!(
            wd.evaluate(&WatchdogView { loops: vec![stuck] })[0],
            Intervention::BackOff { .. }
        ));
    }
}
