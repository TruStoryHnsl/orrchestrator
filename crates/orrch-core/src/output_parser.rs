use std::time::Instant;

use crate::session::SessionState;

/// Signals detected from parsing session output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputSignal {
    /// Session is waiting for user input
    WaitingForInput,
    /// A task was completed
    TaskCompleted(String),
    /// Session is actively working
    Working,
    /// Session is idle (no output for a while)
    Idle,
}

/// Patterns that indicate the AI is waiting for user input.
const WAITING_PATTERNS: &[&str] = &[
    // Claude Code patterns
    "❯",                    // Claude Code prompt
    "> ",                   // generic prompt
    "? ",                   // question prompt
    "Press Enter",
    "Do you want to",
    "Would you like",
    "(y/n)",
    "(Y/n)",
    "[Y/n]",
    "[y/N]",
    "Continue?",
    "Proceed?",
    "approve",
    // Gemini patterns
    ">>>",
];

/// Patterns that indicate task completion.
const COMPLETION_PATTERNS: &[&str] = &[
    "Task completed",
    "All tasks completed",
    "Done!",
    "Successfully",
    "✓",
    "✅",
    "PASSED",
    "Build succeeded",
];

/// Analyze a chunk of output and determine the session signal.
pub fn analyze_output(text: &str) -> OutputSignal {
    // Check the last few lines for waiting patterns
    let last_lines: Vec<&str> = text.lines().rev().take(5).collect();

    for line in &last_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check for waiting-for-input
        for pattern in WAITING_PATTERNS {
            if trimmed.contains(pattern) || trimmed.ends_with(pattern) {
                return OutputSignal::WaitingForInput;
            }
        }

        // Check for task completion
        for pattern in COMPLETION_PATTERNS {
            if trimmed.contains(pattern) {
                return OutputSignal::TaskCompleted(trimmed.to_string());
            }
        }
    }

    OutputSignal::Working
}

/// Determine session state from output timing and content.
pub fn infer_state(
    last_output_time: Option<Instant>,
    last_signal: &OutputSignal,
    idle_threshold_secs: f64,
) -> SessionState {
    match last_signal {
        OutputSignal::WaitingForInput => SessionState::Waiting,
        OutputSignal::TaskCompleted(_) => SessionState::Idle,
        OutputSignal::Working => {
            if let Some(last) = last_output_time {
                if last.elapsed().as_secs_f64() > idle_threshold_secs {
                    SessionState::Idle
                } else {
                    SessionState::Working
                }
            } else {
                SessionState::Idle
            }
        }
        OutputSignal::Idle => SessionState::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_claude_prompt() {
        assert_eq!(
            analyze_output("Some output\n❯"),
            OutputSignal::WaitingForInput
        );
    }

    #[test]
    fn test_detect_yn_prompt() {
        assert_eq!(
            analyze_output("Do you want to continue? (y/n)"),
            OutputSignal::WaitingForInput
        );
    }

    #[test]
    fn test_detect_completion() {
        let signal = analyze_output("Build succeeded\n");
        assert!(matches!(signal, OutputSignal::TaskCompleted(_)));
    }

    #[test]
    fn test_working_output() {
        assert_eq!(
            analyze_output("Compiling orrchestrator v0.1.0\n"),
            OutputSignal::Working
        );
    }

    #[test]
    fn test_infer_idle_after_timeout() {
        let old_time = Instant::now() - std::time::Duration::from_secs(60);
        assert_eq!(
            infer_state(Some(old_time), &OutputSignal::Working, 30.0),
            SessionState::Idle
        );
    }

    #[test]
    fn test_infer_waiting() {
        assert_eq!(
            infer_state(Some(Instant::now()), &OutputSignal::WaitingForInput, 30.0),
            SessionState::Waiting
        );
    }
}
