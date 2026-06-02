use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::intent::{OllamaInterpreter, VoiceAction, VoiceInterpreter};
use crate::protocol::Utterance;

pub type VoiceActivityLog = Arc<Mutex<Vec<VoiceActivity>>>;

static GLOBAL_ACTIVITY_LOG: OnceLock<VoiceActivityLog> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VoiceActivityStatus {
    Proposed,
    Confirmed,
    Dispatched,
    Rejected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceActivity {
    pub ts_ms: u64,
    pub utterance: String,
    pub action: VoiceAction,
    pub status: VoiceActivityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VoiceControlConfig {
    pub auto_dispatch: bool,
    pub max_concurrent_dispatches: usize,
}

impl VoiceControlConfig {
    pub fn from_env() -> Self {
        Self {
            auto_dispatch: env_flag("ORRCH_VOICE_AUTO_DISPATCH"),
            max_concurrent_dispatches: std::env::var("ORRCH_VOICE_MAX_CONCURRENT_DISPATCHES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(2),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDispatchResult {
    pub session_id: Option<String>,
    pub message: String,
}

pub trait ActionDispatcher: Send + Sync {
    fn execute(&self, action: &VoiceAction) -> Result<ActionDispatchResult>;
}

#[derive(Clone)]
pub struct VoiceControlLoop {
    inner: Arc<VoiceControlLoopInner>,
}

struct VoiceControlLoopInner {
    interpreter: Arc<dyn VoiceInterpreter>,
    dispatcher: Arc<dyn ActionDispatcher>,
    config: VoiceControlConfig,
    pending: Mutex<Option<PendingAction>>,
    recent_context: Mutex<VecDeque<String>>,
    activity_log: VoiceActivityLog,
    active_dispatches: AtomicUsize,
}

#[derive(Debug, Clone)]
struct PendingAction {
    utterance: String,
    action: VoiceAction,
}

impl VoiceControlLoop {
    pub fn new(
        interpreter: Arc<dyn VoiceInterpreter>,
        dispatcher: Arc<dyn ActionDispatcher>,
        config: VoiceControlConfig,
    ) -> Self {
        Self {
            inner: Arc::new(VoiceControlLoopInner {
                interpreter,
                dispatcher,
                config,
                pending: Mutex::new(None),
                recent_context: Mutex::new(VecDeque::new()),
                activity_log: Arc::new(Mutex::new(Vec::new())),
                active_dispatches: AtomicUsize::new(0),
            }),
        }
    }

    pub fn from_env(dispatcher: Arc<dyn ActionDispatcher>) -> Self {
        Self::new(
            Arc::new(OllamaInterpreter::from_env()),
            dispatcher,
            VoiceControlConfig::from_env(),
        )
    }

    pub fn activity_handle(&self) -> VoiceActivityLog {
        self.inner.activity_log.clone()
    }

    /// Publish the loop log through a process-global handle for the future TUI
    /// panel. The first enabled loop owns the handle for the process lifetime.
    pub fn publish_activity_handle(&self) {
        let _ = GLOBAL_ACTIVITY_LOG.set(self.activity_handle());
    }

    pub fn global_activity_log() -> Option<VoiceActivityLog> {
        GLOBAL_ACTIVITY_LOG.get().cloned()
    }

    pub fn start(&self, receiver: mpsc::Receiver<Utterance>) -> thread::JoinHandle<()> {
        let loop_handle = self.clone();
        thread::Builder::new()
            .name("orrch-voice-control-loop".into())
            .spawn(move || {
                for utterance in receiver {
                    loop_handle.handle_utterance(utterance);
                }
            })
            .expect("failed to spawn orrch-voice-control-loop")
    }

    pub fn handle_text_for_test(&self, text: &str) {
        self.handle_utterance(Utterance {
            text: text.to_string(),
            ts_ms: now_ms(),
        });
    }

    pub fn handle_utterance(&self, utterance: Utterance) {
        let recent = self.recent_context();
        let action = match self.inner.interpreter.interpret(&utterance.text, &recent) {
            Ok(action) => action,
            Err(err) => {
                self.record(
                    utterance.text,
                    VoiceAction::None,
                    VoiceActivityStatus::Error,
                    Some(err.to_string()),
                );
                return;
            }
        };

        self.remember_context(&utterance.text);
        match action {
            VoiceAction::Note { .. } | VoiceAction::None => {
                self.record(
                    utterance.text,
                    action,
                    VoiceActivityStatus::Dispatched,
                    None,
                );
            }
            VoiceAction::Confirm => self.confirm_pending(utterance.text),
            VoiceAction::Cancel => self.cancel_pending(utterance.text),
            VoiceAction::Dispatch { .. } | VoiceAction::SpawnSession { .. } => {
                if self.inner.config.auto_dispatch {
                    self.execute_heavy(utterance.text, action);
                } else {
                    *self.inner.pending.lock().unwrap() = Some(PendingAction {
                        utterance: utterance.text.clone(),
                        action: action.clone(),
                    });
                    self.record(utterance.text, action, VoiceActivityStatus::Proposed, None);
                }
            }
        }
    }

    fn recent_context(&self) -> Vec<String> {
        self.inner
            .recent_context
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    fn remember_context(&self, utterance: &str) {
        let mut context = self.inner.recent_context.lock().unwrap();
        context.push_back(utterance.to_string());
        while context.len() > 8 {
            context.pop_front();
        }
    }

    fn confirm_pending(&self, utterance: String) {
        let pending = self.inner.pending.lock().unwrap().take();
        let Some(pending) = pending else {
            self.record(
                utterance,
                VoiceAction::Confirm,
                VoiceActivityStatus::Rejected,
                Some("no pending voice action to confirm".to_string()),
            );
            return;
        };

        self.record(
            utterance,
            pending.action.clone(),
            VoiceActivityStatus::Confirmed,
            Some(format!("confirmed utterance: {}", pending.utterance)),
        );
        self.execute_heavy(pending.utterance, pending.action);
    }

    fn cancel_pending(&self, utterance: String) {
        let pending = self.inner.pending.lock().unwrap().take();
        let action = pending
            .map(|pending| pending.action)
            .unwrap_or(VoiceAction::Cancel);
        self.record(utterance, action, VoiceActivityStatus::Rejected, None);
    }

    fn execute_heavy(&self, utterance: String, action: VoiceAction) {
        if !self.try_acquire_dispatch_slot() {
            self.record(
                utterance,
                action,
                VoiceActivityStatus::Rejected,
                Some(format!(
                    "dispatch cap reached ({})",
                    self.inner.config.max_concurrent_dispatches
                )),
            );
            return;
        }

        let inner = self.inner.clone();
        thread::spawn(move || {
            let result = inner.dispatcher.execute(&action);
            match result {
                Ok(result) => {
                    let detail = match result.session_id {
                        Some(session_id) => {
                            Some(format!("{}; session_id={session_id}", result.message))
                        }
                        None => Some(result.message),
                    };
                    push_activity(
                        &inner.activity_log,
                        utterance,
                        action,
                        VoiceActivityStatus::Dispatched,
                        detail,
                    );
                }
                Err(err) => push_activity(
                    &inner.activity_log,
                    utterance,
                    action,
                    VoiceActivityStatus::Error,
                    Some(err.to_string()),
                ),
            }
            inner.active_dispatches.fetch_sub(1, Ordering::SeqCst);
        });
    }

    fn try_acquire_dispatch_slot(&self) -> bool {
        let max = self.inner.config.max_concurrent_dispatches;
        loop {
            let active = self.inner.active_dispatches.load(Ordering::SeqCst);
            if active >= max {
                return false;
            }
            if self
                .inner
                .active_dispatches
                .compare_exchange(active, active + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return true;
            }
        }
    }

    fn record(
        &self,
        utterance: String,
        action: VoiceAction,
        status: VoiceActivityStatus,
        detail: Option<String>,
    ) {
        push_activity(&self.inner.activity_log, utterance, action, status, detail);
    }
}

#[derive(Clone)]
pub struct CoreActionDispatcher {
    projects_dir: PathBuf,
    process_manager: Arc<Mutex<orrch_core::ProcessManager>>,
    dispatch_backend: Option<orrch_core::BackendKind>,
    runtime_handle: tokio::runtime::Handle,
}

impl CoreActionDispatcher {
    pub fn from_env(runtime_handle: tokio::runtime::Handle) -> Self {
        let config = orrch_core::Config::load();
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
        let process_manager = Arc::new(Mutex::new(orrch_core::ProcessManager::new(event_tx)));
        let dispatch_backend = select_dispatch_backend(&process_manager.lock().unwrap().backends);
        Self {
            projects_dir: config.projects_dir,
            process_manager,
            dispatch_backend,
            runtime_handle,
        }
    }

    #[cfg(test)]
    fn new_for_test(
        projects_dir: PathBuf,
        dispatch_backend: Option<orrch_core::BackendKind>,
    ) -> Self {
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            projects_dir,
            process_manager: Arc::new(Mutex::new(orrch_core::ProcessManager::new(event_tx))),
            dispatch_backend,
            runtime_handle: tokio::runtime::Handle::current(),
        }
    }

    fn project_dir(&self, project: &str) -> Result<PathBuf> {
        let path = Path::new(project);
        let project_dir = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.projects_dir.join(project)
        };
        if !project_dir.is_dir() {
            anyhow::bail!("project directory '{}' not found", project_dir.display());
        }
        Ok(project_dir)
    }
}

impl ActionDispatcher for CoreActionDispatcher {
    fn execute(&self, action: &VoiceAction) -> Result<ActionDispatchResult> {
        match action {
            VoiceAction::Dispatch {
                project,
                instruction,
            } => {
                let project_dir = self.project_dir(project)?;
                let timestamp = orrch_core::feedback::chrono_lite_timestamp();
                orrch_core::intake_review::distribute_to_inbox_from_intake(
                    instruction,
                    &project_dir,
                    &timestamp,
                    65_536,
                )?;
                Ok(ActionDispatchResult {
                    session_id: None,
                    message: format!("appended instruction to {}", project_dir.display()),
                })
            }
            VoiceAction::SpawnSession { project, goal } => {
                let project_dir = self.project_dir(project)?;
                let backend = self.dispatch_backend.ok_or_else(|| {
                    anyhow::anyhow!("no non-Claude backend configured for dispatch")
                })?;
                let _runtime = self.runtime_handle.enter();
                let sid = self
                    .process_manager
                    .lock()
                    .unwrap()
                    .spawn(&project_dir, backend, Some(goal), 40, 120)
                    .with_context(|| {
                        format!(
                            "failed to spawn voice dispatch session in {}",
                            project_dir.display()
                        )
                    })?;
                Ok(ActionDispatchResult {
                    session_id: Some(sid),
                    message: format!("spawned {} dispatch session", backend.label()),
                })
            }
            _ => anyhow::bail!("voice action is not dispatchable"),
        }
    }
}

fn push_activity(
    activity_log: &VoiceActivityLog,
    utterance: String,
    action: VoiceAction,
    status: VoiceActivityStatus,
    detail: Option<String>,
) {
    activity_log.lock().unwrap().push(VoiceActivity {
        ts_ms: now_ms(),
        utterance,
        action,
        status,
        detail,
    });
}

fn select_dispatch_backend(
    backends: &orrch_core::BackendsConfig,
) -> Option<orrch_core::BackendKind> {
    if let Ok(value) = std::env::var("ORRCH_VOICE_DISPATCH_BACKEND") {
        let requested = normalize_backend_label(&value);
        let selected = backend_by_label(&requested).filter(|kind| allowed_dispatch_backend(*kind));
        if selected.is_none() {
            warn!("ORRCH_VOICE_DISPATCH_BACKEND={value} is not an allowed non-Claude CLI backend");
        }
        return selected;
    }

    let available = backends.available();
    ["codex", "gemini", "crush", "opencode"]
        .iter()
        .filter_map(|label| backend_by_label(label))
        .find(|kind| available.contains(kind) && allowed_dispatch_backend(*kind))
}

fn backend_by_label(label: &str) -> Option<orrch_core::BackendKind> {
    orrch_core::BackendKind::all()
        .iter()
        .copied()
        .find(|kind| kind.label() == label)
}

fn allowed_dispatch_backend(kind: orrch_core::BackendKind) -> bool {
    matches!(kind.label(), "codex" | "gemini" | "crush" | "opencode") && kind.is_cli()
}

fn normalize_backend_label(value: &str) -> String {
    value.trim().to_lowercase().replace('_', "-")
}

fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("on")
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn start_loop_from_env(
    receiver: mpsc::Receiver<Utterance>,
    runtime_handle: tokio::runtime::Handle,
) {
    let dispatcher = Arc::new(CoreActionDispatcher::from_env(runtime_handle));
    let control_loop = VoiceControlLoop::from_env(dispatcher);
    control_loop.publish_activity_handle();
    let _ = control_loop.start(receiver);
    info!("orrch-voice control loop enabled with local Ollama interpreter");
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{Receiver, Sender};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::intent::MockInterpreter;

    #[derive(Default)]
    struct RecordingDispatcher {
        calls: Mutex<Vec<VoiceAction>>,
    }

    impl ActionDispatcher for RecordingDispatcher {
        fn execute(&self, action: &VoiceAction) -> Result<ActionDispatchResult> {
            self.calls.lock().unwrap().push(action.clone());
            Ok(ActionDispatchResult {
                session_id: None,
                message: "mock dispatched".to_string(),
            })
        }
    }

    struct BlockingDispatcher {
        calls: AtomicUsize,
        release_rx: Mutex<Receiver<()>>,
    }

    impl ActionDispatcher for BlockingDispatcher {
        fn execute(&self, _action: &VoiceAction) -> Result<ActionDispatchResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _ = self.release_rx.lock().unwrap().recv();
            Ok(ActionDispatchResult {
                session_id: None,
                message: "released".to_string(),
            })
        }
    }

    fn dispatch_action() -> VoiceAction {
        VoiceAction::Dispatch {
            project: "orrchestrator".to_string(),
            instruction: "Fix the responsive tabs.".to_string(),
        }
    }

    fn wait_until<F: Fn() -> bool>(predicate: F) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            if predicate() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for predicate");
    }

    #[test]
    fn safe_mode_proposes_before_dispatching_and_confirm_executes() {
        let interpreter = Arc::new(MockInterpreter::new(vec![
            dispatch_action(),
            VoiceAction::Confirm,
        ]));
        let dispatcher = Arc::new(RecordingDispatcher::default());
        let control_loop = VoiceControlLoop::new(
            interpreter,
            dispatcher.clone(),
            VoiceControlConfig {
                auto_dispatch: false,
                max_concurrent_dispatches: 2,
            },
        );

        control_loop.handle_text_for_test("route this to the project");
        assert!(dispatcher.calls.lock().unwrap().is_empty());
        assert_eq!(
            control_loop.activity_handle().lock().unwrap()[0].status,
            VoiceActivityStatus::Proposed
        );

        control_loop.handle_text_for_test("yes, do it");
        wait_until(|| dispatcher.calls.lock().unwrap().len() == 1);
        let log = control_loop.activity_handle();
        let statuses: Vec<_> = log
            .lock()
            .unwrap()
            .iter()
            .map(|activity| activity.status.clone())
            .collect();
        assert!(statuses.contains(&VoiceActivityStatus::Confirmed));
        assert!(statuses.contains(&VoiceActivityStatus::Dispatched));
    }

    #[test]
    fn safe_mode_blocks_dispatch_without_confirm() {
        let interpreter = Arc::new(MockInterpreter::new(vec![dispatch_action()]));
        let dispatcher = Arc::new(RecordingDispatcher::default());
        let control_loop = VoiceControlLoop::new(
            interpreter,
            dispatcher.clone(),
            VoiceControlConfig {
                auto_dispatch: false,
                max_concurrent_dispatches: 2,
            },
        );

        control_loop.handle_text_for_test("please implement this");
        thread::sleep(Duration::from_millis(30));

        assert!(dispatcher.calls.lock().unwrap().is_empty());
        assert_eq!(
            control_loop.activity_handle().lock().unwrap()[0].status,
            VoiceActivityStatus::Proposed
        );
    }

    #[test]
    fn concurrent_dispatch_cap_is_enforced() {
        let (release_tx, release_rx): (Sender<()>, Receiver<()>) = mpsc::channel();
        let dispatcher = Arc::new(BlockingDispatcher {
            calls: AtomicUsize::new(0),
            release_rx: Mutex::new(release_rx),
        });
        let interpreter = Arc::new(MockInterpreter::new(vec![
            dispatch_action(),
            VoiceAction::Confirm,
            dispatch_action(),
            VoiceAction::Confirm,
        ]));
        let control_loop = VoiceControlLoop::new(
            interpreter,
            dispatcher.clone(),
            VoiceControlConfig {
                auto_dispatch: false,
                max_concurrent_dispatches: 1,
            },
        );

        control_loop.handle_text_for_test("first task");
        control_loop.handle_text_for_test("confirm");
        wait_until(|| dispatcher.calls.load(Ordering::SeqCst) == 1);

        control_loop.handle_text_for_test("second task");
        control_loop.handle_text_for_test("confirm");

        let log = control_loop.activity_handle();
        wait_until(|| {
            log.lock().unwrap().iter().any(|activity| {
                activity.status == VoiceActivityStatus::Rejected
                    && activity
                        .detail
                        .as_deref()
                        .is_some_and(|detail| detail.contains("dispatch cap reached"))
            })
        });

        release_tx.send(()).unwrap();
    }

    #[tokio::test]
    async fn core_dispatch_appends_to_project_inbox() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("orrchestrator");
        std::fs::create_dir_all(&project).unwrap();
        let dispatcher = CoreActionDispatcher::new_for_test(tmp.path().to_path_buf(), None);
        let result = dispatcher
            .execute(&VoiceAction::Dispatch {
                project: "orrchestrator".to_string(),
                instruction: "Ship the voice loop.".to_string(),
            })
            .unwrap();

        assert!(result.message.contains("appended instruction"));
        let inbox = std::fs::read_to_string(project.join("instructions_inbox.md")).unwrap();
        assert!(inbox.contains("Ship the voice loop."));
    }
}
