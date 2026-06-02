use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use orrch_voice::control_loop::{
    ActionDispatchResult, ActionDispatcher, VoiceControlConfig, VoiceControlLoop,
};
use orrch_voice::intent::{VoiceAction, VoiceInterpreter};

struct DemoInterpreter {
    actions: Mutex<VecDeque<VoiceAction>>,
}

impl DemoInterpreter {
    fn new(actions: Vec<VoiceAction>) -> Self {
        Self {
            actions: Mutex::new(actions.into()),
        }
    }
}

impl VoiceInterpreter for DemoInterpreter {
    fn interpret(&self, _utterance: &str, _recent_context: &[String]) -> Result<VoiceAction> {
        Ok(self
            .actions
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(VoiceAction::None))
    }
}

struct DemoDispatcher;

impl ActionDispatcher for DemoDispatcher {
    fn execute(&self, _action: &VoiceAction) -> Result<ActionDispatchResult> {
        Ok(ActionDispatchResult {
            session_id: None,
            message: "demo dispatcher invoked".to_string(),
        })
    }
}

fn main() {
    let control_loop = VoiceControlLoop::new(
        Arc::new(DemoInterpreter::new(vec![
            VoiceAction::Dispatch {
                project: "orrchestrator".to_string(),
                instruction: "Fix the responsive voice tabs.".to_string(),
            },
            VoiceAction::Confirm,
        ])),
        Arc::new(DemoDispatcher),
        VoiceControlConfig {
            auto_dispatch: false,
            max_concurrent_dispatches: 2,
        },
    );

    control_loop.handle_text_for_test("send this to orrchestrator");
    control_loop.handle_text_for_test("yes, do it");

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2)
        && control_loop.activity_handle().lock().unwrap().len() < 3
    {
        thread::sleep(Duration::from_millis(10));
    }

    for activity in control_loop.activity_handle().lock().unwrap().iter() {
        println!(
            "{:?}: {:?} ({})",
            activity.status, activity.action, activity.utterance
        );
    }
}
