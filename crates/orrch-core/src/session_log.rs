//! Read Claude Code session conversation logs for display.

use std::path::{Path, PathBuf};

/// A conversation message from a Claude session log.
#[derive(Debug, Clone)]
pub struct LogMessage {
    pub role: String,  // "human" or "assistant"
    pub text: String,
}

/// Read the conversation log for an external session by PID.
/// Returns the last N messages as displayable text.
pub fn read_session_log(pid: u32, max_messages: usize) -> Vec<LogMessage> {
    let Some((project_dir, session_id)) = session_info(pid) else {
        return Vec::new();
    };

    let log_path = find_log_file(&project_dir, &session_id);
    let Some(path) = log_path else {
        return Vec::new();
    };

    parse_jsonl_log(&path, max_messages)
}

/// Get the last N messages as a formatted string for display.
pub fn format_session_log(pid: u32, max_messages: usize) -> String {
    let messages = read_session_log(pid, max_messages);
    if messages.is_empty() {
        // Diagnostic: show what we tried
        let home = std::env::var("HOME").unwrap_or_default();
        let session_file = format!("{home}/.claude/sessions/{pid}.json");
        let session_exists = std::path::Path::new(&session_file).exists();
        let (cwd, sid) = session_info(pid).unwrap_or_default();
        let log_file = if !sid.is_empty() { find_log_file(&cwd, &sid) } else { None };
        let log_info = match &log_file {
            Some(p) => format!("Log file: {} ({} bytes)", p.display(),
                std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)),
            None => "Log file: NOT FOUND".into(),
        };

        return format!(
            "No messages extracted for pid:{pid}\n\n\
             Session file: {session_file} (exists: {session_exists})\n\
             CWD: {cwd}\n\
             Session ID: {sid}\n\
             {log_info}\n\n\
             This is an external session. The conversation log is read from\n\
             Claude's own session files at ~/.claude/projects/<hash>/<id>.jsonl"
        );
    }

    let mut output = String::new();
    for msg in &messages {
        let prefix = if msg.role == "user" { "▶ You" } else { "◀ Claude" };
        output.push_str(&format!("─── {prefix} ───\n"));
        output.push_str(&msg.text);
        output.push_str("\n\n");
    }
    output
}

fn session_info(pid: u32) -> Option<(String, String)> {
    let home = std::env::var("HOME").ok()?;
    let session_file = Path::new(&home)
        .join(".claude")
        .join("sessions")
        .join(format!("{pid}.json"));

    let contents = std::fs::read_to_string(session_file).ok()?;

    // Extract cwd and sessionId
    let cwd = extract_json_string(&contents, "cwd")?;
    let session_id = extract_json_string(&contents, "sessionId")?;

    Some((cwd, session_id))
}

fn find_log_file(project_dir: &str, session_id: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let projects_base = Path::new(&home).join(".claude").join("projects");

    // Claude uses path-hash for project dirs: /home/corr/projects/concord -> -home-corr-projects-concord
    let path_hash = project_dir.replace('/', "-").trim_start_matches('-').to_string();

    let project_log_dir = projects_base.join(&path_hash);
    let log_file = project_log_dir.join(format!("{session_id}.jsonl"));

    if log_file.exists() {
        return Some(log_file);
    }

    // Try with leading dash
    let log_file = projects_base.join(format!("-{path_hash}")).join(format!("{session_id}.jsonl"));
    if log_file.exists() {
        return Some(log_file);
    }

    // Scan all project dirs for the session file
    if let Ok(entries) = std::fs::read_dir(&projects_base) {
        for entry in entries.flatten() {
            let candidate = entry.path().join(format!("{session_id}.jsonl"));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

fn parse_jsonl_log(path: &Path, max_messages: usize) -> Vec<LogMessage> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut messages = Vec::new();

    for line in contents.lines().rev() {
        if messages.len() >= max_messages {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Simple JSON parsing for role and text content
        let role = if line.contains("\"type\":\"user\"") || line.contains("\"type\": \"user\"")
            || line.contains("\"type\":\"human\"") || line.contains("\"type\": \"human\"") {
            "user"
        } else if line.contains("\"type\":\"assistant\"") || line.contains("\"type\": \"assistant\"") {
            "assistant"
        } else {
            continue;
        };

        // Extract text from content array
        let text = extract_message_text(line);
        if !text.is_empty() {
            messages.push(LogMessage {
                role: role.to_string(),
                text,
            });
        }
    }

    messages.reverse();
    messages
}

fn extract_message_text(json_line: &str) -> String {
    // Try serde_json for robust parsing
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json_line) else {
        return String::new();
    };

    let content = &val["message"]["content"];

    // Case 1: content is a string (some user messages)
    if let Some(s) = content.as_str() {
        return s.to_string();
    }

    // Case 2: content is an array of blocks
    if let Some(arr) = content.as_array() {
        let mut texts = Vec::new();
        for block in arr {
            if block["type"].as_str() == Some("text") {
                if let Some(t) = block["text"].as_str() {
                    texts.push(t.to_string());
                }
            }
        }
        return texts.join("\n");
    }

    String::new()
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let pos = json.find(&pattern)?;
    let rest = &json[pos + pattern.len()..];
    let rest = rest.trim_start().strip_prefix(':')?;
    let rest = rest.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
