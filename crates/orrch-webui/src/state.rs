use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebAppState {
    pub active_panel: String,
    pub active_sub: String,
    pub ideas: Vec<WebIdea>,
    pub sessions: Vec<WebSession>,
    pub projects: Vec<WebProject>,
    /// Local terminal size — xterm.js must match exactly or content
    /// wrapping will corrupt the display on narrower browser viewports.
    pub term_cols: u16,
    pub term_rows: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebIdea {
    pub filename: String,
    pub progress: u8,
    pub targets_count: usize,
    pub submitted: bool,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSession {
    pub name: String,
    pub category: String,
    pub goal: String,
    pub attach_cmd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebProject {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum WebAction {
    #[serde(rename = "key")]
    Key { key: String },
    #[serde(rename = "retract")]
    Retract { filename: String },
}

pub fn state_hash(state: &WebAppState) -> [u8; 32] {
    let json = serde_json::to_string(state).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    hasher.finalize().into()
}
