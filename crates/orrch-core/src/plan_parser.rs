/// Structured parser for Plan.md files.
/// Extracts phases, features, and status into a navigable tree.

/// Context for why a feature was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalContext {
    RemovedBeforeImpl,
    RemovedAfterImpl,
    FailingVerification,
}

/// Feature lifecycle state machine.
///
/// Parse markers: `[ ]`=Planned, `[~]`=Implementing, `[=]`=Implemented,
/// `[t]`=Testing, `[v]`=Verified, `[✓]`=UserConfirmed, `[x]`=Done,
/// strikethrough=Deprecated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureStatus {
    Planned,         // [ ] — not started
    Implementing,    // [~] — work in progress
    Implemented,     // [=] — code done, not tested
    Testing,         // [t] — tests running
    Verified,        // [v] — tests passed
    UserConfirmed,   // [✓] — user manually confirmed
    Done,            // [x] — shipped
    Deprecated,      // strikethrough text
    Pending,         // alias for Planned (backward compat)
    InProgress,      // alias for Implementing (backward compat)
    Removed(RemovalContext),
}

impl FeatureStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Planned | Self::Pending => "[ ]",
            Self::Implementing | Self::InProgress => "[~]",
            Self::Implemented => "[=]",
            Self::Testing => "[t]",
            Self::Verified => "[v]",
            Self::UserConfirmed => "[✓]",
            Self::Done => "[x]",
            Self::Deprecated => "[D]",
            Self::Removed(_) => "[R]",
        }
    }

    /// Status icon for TUI display.
    pub fn display_icon(&self) -> &'static str {
        match self {
            Self::Planned | Self::Pending => "○",
            Self::Implementing | Self::InProgress => "◑",
            Self::Implemented => "◉",
            Self::Testing => "⚙",
            Self::Verified => "✔",
            Self::UserConfirmed => "✓",
            Self::Done => "✓",
            Self::Deprecated => "⊘",
            Self::Removed(_) => "✗",
        }
    }

    /// Whether this status counts as "done" (shipped or beyond).
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done | Self::UserConfirmed | Self::Verified)
    }

    /// Whether this status counts as "open" (still needs work).
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Planned | Self::Pending | Self::Implementing | Self::InProgress | Self::Implemented | Self::Testing)
    }

    /// The markdown checkbox marker for write-back.
    pub fn write_marker(&self) -> &'static str {
        match self {
            Self::Planned | Self::Pending => "[ ]",
            Self::Implementing | Self::InProgress => "[~]",
            Self::Implemented => "[=]",
            Self::Testing => "[t]",
            Self::Verified => "[v]",
            Self::UserConfirmed => "[✓]",
            Self::Done => "[x]",
            Self::Deprecated => "[ ]",   // deprecated indicated by strikethrough, not marker
            Self::Removed(_) => "[ ]",
        }
    }

    /// Short label for the status.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Planned | Self::Pending => "planned",
            Self::Implementing | Self::InProgress => "implementing",
            Self::Implemented => "implemented",
            Self::Testing => "testing",
            Self::Verified => "verified",
            Self::UserConfirmed => "confirmed",
            Self::Done => "done",
            Self::Deprecated => "deprecated",
            Self::Removed(_) => "removed",
        }
    }

    /// Cycle forward through the mutable states.
    /// Done, Deprecated, and Removed are terminal.
    pub fn cycle_forward(&self) -> Self {
        match self {
            Self::Planned | Self::Pending => Self::Implementing,
            Self::Implementing | Self::InProgress => Self::Implemented,
            Self::Implemented => Self::Testing,
            Self::Testing => Self::Verified,
            Self::Verified => Self::UserConfirmed,
            Self::UserConfirmed => Self::Done,
            Self::Done => Self::Done,
            Self::Deprecated => Self::Deprecated,
            Self::Removed(ctx) => Self::Removed(*ctx),
        }
    }

    /// Cycle backward through the mutable states.
    pub fn cycle_backward(&self) -> Self {
        match self {
            Self::Done => Self::UserConfirmed,
            Self::UserConfirmed => Self::Verified,
            Self::Verified => Self::Testing,
            Self::Testing => Self::Implemented,
            Self::Implemented => Self::Implementing,
            Self::Implementing | Self::InProgress => Self::Planned,
            Self::Planned | Self::Pending => Self::Planned,
            Self::Deprecated => Self::Deprecated,
            Self::Removed(ctx) => Self::Removed(*ctx),
        }
    }
}

/// Parse a status marker from the start of a string.
/// Returns the status and the number of bytes consumed.
pub fn parse_status_marker(s: &str) -> Option<(FeatureStatus, usize)> {
    // Check multi-byte markers first (✓ is 3 bytes in UTF-8)
    if s.starts_with("[✓]") {
        let len = "[✓]".len(); // 5 bytes
        return Some((FeatureStatus::UserConfirmed, len));
    }
    if s.len() >= 3 {
        let marker = &s[..3];
        let status = match marker {
            "[x]" | "[X]" => FeatureStatus::Done,
            "[ ]" => FeatureStatus::Planned,
            "[~]" => FeatureStatus::Implementing,
            "[=]" => FeatureStatus::Implemented,
            "[t]" | "[T]" => FeatureStatus::Testing,
            "[v]" | "[V]" => FeatureStatus::Verified,
            _ => return None,
        };
        return Some((status, 3));
    }
    None
}

#[derive(Debug, Clone)]
pub struct PlanFeature {
    /// Numeric id prefix if present (e.g., 44 from "44. [ ] ...")
    pub id: Option<u32>,
    pub title: String,
    pub description: String,
    pub status: FeatureStatus,
}

#[derive(Debug, Clone)]
pub struct PlanPhase {
    pub name: String,
    pub number: Option<u8>,
    pub features: Vec<PlanFeature>,
}

impl PlanPhase {
    pub fn done_count(&self) -> usize {
        self.features.iter().filter(|f| f.status.is_done()).count()
    }

    pub fn total_count(&self) -> usize {
        self.features.len()
    }
}

/// Parse a Plan.md file content into structured phases and features.
pub fn parse_plan(content: &str) -> Vec<PlanPhase> {
    let mut phases: Vec<PlanPhase> = Vec::new();
    let mut current_phase: Option<PlanPhase> = None;
    let mut last_feature: Option<usize> = None; // index into current_phase.features for continuation lines

    for line in content.lines() {
        let trimmed = line.trim();

        // Detect phase headers: ## Phase N: Name, ### Phase N: Name,
        // or section headers like "### CRITICAL PATH ...", "### Cross-Cutting: ..."
        if let Some(phase) = try_parse_phase_header(trimmed) {
            // Save previous phase
            if let Some(p) = current_phase.take() {
                if !p.features.is_empty() {
                    phases.push(p);
                }
            }
            current_phase = Some(phase);
            last_feature = None;
            continue;
        }

        // Detect feature lines within a phase
        if current_phase.is_some() {
            if let Some(feature) = try_parse_feature_line(trimmed) {
                let phase = current_phase.as_mut().unwrap();
                phase.features.push(feature);
                last_feature = Some(phase.features.len() - 1);
                continue;
            }

            // Continuation line: non-empty, not a heading, not a separator
            if !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("---")
                && !trimmed.starts_with("```")
                && !trimmed.starts_with('|')
                && !trimmed.starts_with('>')
                && !trimmed.starts_with('_')
            {
                if let Some(idx) = last_feature {
                    let phase = current_phase.as_mut().unwrap();
                    if let Some(feat) = phase.features.get_mut(idx) {
                        if !feat.description.is_empty() {
                            feat.description.push(' ');
                        }
                        feat.description.push_str(trimmed);
                    }
                }
                continue;
            }

            // Empty line or separator resets continuation
            if trimmed.is_empty() || trimmed.starts_with("---") {
                last_feature = None;
            }
        }
    }

    // Don't forget the last phase
    if let Some(p) = current_phase {
        if !p.features.is_empty() {
            phases.push(p);
        }
    }

    phases
}

/// Try to parse a line as a phase/section header.
fn try_parse_phase_header(line: &str) -> Option<PlanPhase> {
    // Must start with ## or ###
    let rest = if let Some(r) = line.strip_prefix("### ") {
        r
    } else if let Some(r) = line.strip_prefix("## ") {
        r
    } else {
        return None;
    };

    // Skip non-phase headings we don't want as phases
    let lower = rest.to_lowercase();
    if lower.starts_with("design decisions")
        || lower.starts_with("architecture")
        || lower.starts_with("key workflows")
        || lower.starts_with("technical stack")
        || lower.starts_with("keybindings")
        || lower.starts_with("q1 ")
        || lower.starts_with("q2 ")
        || lower.starts_with("q3 ")
        || lower.starts_with("q4 ")
        || lower.starts_with("q5 ")
        || lower.starts_with("q6 ")
        || lower.starts_with("q7 ")
        || lower.starts_with("q8 ")
        || lower.starts_with("core concept")
        || lower.starts_with("hard requirements")
        || lower.starts_with("panel layout")
        || lower.starts_with("crate structure")
        || lower.starts_with("agent department")
        || lower.starts_with("operation module")
        || lower.starts_with("multi-backend")
        || lower.starts_with("order of operations")
    {
        return None;
    }

    // Try to extract "Phase N: Name" pattern
    if let Some(after_phase) = lower.strip_prefix("phase ") {
        // Find the number
        let num_str: String = after_phase.chars().take_while(|c| c.is_ascii_digit()).collect();
        let number = num_str.parse::<u8>().ok();

        // Name is everything after "Phase N: " or "Phase N — "
        let name_start = rest.find(':').or_else(|| rest.find('—')).map(|i| i + 1);
        let name = if let Some(start) = name_start {
            rest[start..].trim().to_string()
        } else {
            rest.to_string()
        };

        return Some(PlanPhase {
            name,
            number,
            features: Vec::new(),
        });
    }

    // Section headers like "CRITICAL PATH ...", "Cross-Cutting: ...", "Roadmap ...", etc.
    // Accept them as phases if they contain feature-like content
    if lower.contains("critical path")
        || lower.contains("cross-cutting")
        || lower.contains("roadmap")
        || lower.contains("ui polish")
        || lower.contains("carried forward")
        || lower.starts_with("1.0.0")
    {
        // Clean up the name: strip markdown formatting
        let name = rest
            .replace("~~", "")
            .trim()
            .to_string();

        // Try to extract a version number from names like "1.0.0 Feature Roadmap"
        return Some(PlanPhase {
            name,
            number: None,
            features: Vec::new(),
        });
    }

    None
}

/// Try to parse a feature line.
fn try_parse_feature_line(line: &str) -> Option<PlanFeature> {
    // Patterns:
    //   N. [x] **Title** — description
    //   N. [ ] **Title** — description
    //   CP-N. [x] **Title** — description
    //   - [x] **Title** — description
    //   - [ ] **Title** — description

    let trimmed = line.trim();

    // Strip the leading prefix to get to the checkbox
    let (id, rest) = strip_feature_prefix(trimmed)?;

    // Must have a status marker
    let (mut status, consumed) = if let Some((s, n)) = parse_status_marker(rest) {
        (s, n)
    } else {
        return None;
    };
    let after_checkbox = rest[consumed..].trim_start();

    // Parse title and description
    let (title, description) = parse_title_description(after_checkbox);

    if title.is_empty() {
        return None;
    }

    // Override status for deprecated/deferred items (text-based detection)
    if status == FeatureStatus::Planned {
        if description.to_uppercase().contains("DEPRECATED")
            || description.contains("MOVED")
            || title.to_uppercase().contains("DEPRECATED")
        {
            status = FeatureStatus::Deprecated;
        }
    }

    Some(PlanFeature {
        id,
        title,
        description,
        status,
    })
}

/// Strip the leading numbering prefix and return (optional id, remaining text).
fn strip_feature_prefix(line: &str) -> Option<(Option<u32>, &str)> {
    let trimmed = line.trim_start();

    // "CP-N." prefix
    if let Some(after_cp) = trimmed.strip_prefix("CP-") {
        let num_str: String = after_cp.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !num_str.is_empty() {
            let after_num = &after_cp[num_str.len()..];
            let rest = after_num.trim_start_matches('.').trim_start();
            return Some((None, rest)); // CP items don't get numeric IDs
        }
    }

    // "N." prefix (numbered features)
    let num_str: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !num_str.is_empty() {
        let after_num = &trimmed[num_str.len()..];
        if after_num.starts_with('.') {
            let id = num_str.parse::<u32>().ok();
            let rest = after_num[1..].trim_start();
            return Some((id, rest));
        }
    }

    // "- " prefix (unnumbered)
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return Some((None, rest.trim_start()));
    }

    None
}

/// Parse "**Title** — description" or "**Title** description" patterns.
fn parse_title_description(text: &str) -> (String, String) {
    if text.starts_with("**") {
        let after_open = &text[2..];
        if let Some(close_pos) = after_open.find("**") {
            let title = after_open[..close_pos].to_string();
            let desc = after_open[close_pos + 2..]
                .trim_start_matches(|c: char| c == ' ' || c == '—' || c == '-' || c == '–')
                .trim()
                .to_string();
            return (title, desc);
        }
    }

    // Fallback: split on em-dash
    if let Some(pos) = text.find('—') {
        let title = text[..pos].trim().to_string();
        let desc = text[pos + '—'.len_utf8()..].trim().to_string();
        return (title, desc);
    }

    (text.trim().to_string(), String::new())
}

/// Direction for moving features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveDirection {
    Up,
    Down,
}

/// Move a feature up or down within a Plan.md file.
///
/// Within a phase, swaps adjacent feature lines.
/// Cross-phase: moves the feature to the last position of the previous phase
/// (for Up) or the first position of the next phase (for Down).
///
/// `phase_idx` and `feature_idx` are indices into the parsed PlanPhase/PlanFeature vectors.
pub fn move_feature_in_plan(
    plan_path: &std::path::Path,
    phase_idx: usize,
    feature_idx: usize,
    direction: MoveDirection,
) -> std::io::Result<bool> {
    let contents = std::fs::read_to_string(plan_path)?;
    let phases = parse_plan(&contents);

    let phase = match phases.get(phase_idx) {
        Some(p) => p,
        None => return Ok(false),
    };

    // Find the source feature's line in the file by matching its title
    let feat = match phase.features.get(feature_idx) {
        Some(f) => f,
        None => return Ok(false),
    };

    let lines: Vec<&str> = contents.lines().collect();

    // Find the line index of the feature we want to move
    let source_line = match find_feature_line(&lines, &feat.title) {
        Some(idx) => idx,
        None => return Ok(false),
    };

    let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

    match direction {
        MoveDirection::Up => {
            if feature_idx > 0 {
                // Swap with previous feature in same phase
                let prev_feat = &phase.features[feature_idx - 1];
                let prev_line = match find_feature_line(&lines, &prev_feat.title) {
                    Some(idx) => idx,
                    None => return Ok(false),
                };
                new_lines.swap(source_line, prev_line);
            } else if phase_idx > 0 {
                // Cross-phase: move to end of previous phase
                let prev_phase = &phases[phase_idx - 1];
                if prev_phase.features.is_empty() {
                    // Insert after the phase header
                    let target = match find_phase_header_line(&lines, &prev_phase.name) {
                        Some(idx) => idx + 1,
                        None => return Ok(false),
                    };
                    let removed = new_lines.remove(source_line);
                    let insert_at = if source_line < target { target - 1 } else { target };
                    new_lines.insert(insert_at, removed);
                } else {
                    let last_feat = prev_phase.features.last().unwrap();
                    let target = match find_feature_line(&lines, &last_feat.title) {
                        Some(idx) => idx,
                        None => return Ok(false),
                    };
                    let removed = new_lines.remove(source_line);
                    let insert_at = if source_line < target { target } else { target + 1 };
                    new_lines.insert(insert_at, removed);
                }
            } else {
                return Ok(false); // already at top
            }
        }
        MoveDirection::Down => {
            if feature_idx + 1 < phase.features.len() {
                // Swap with next feature in same phase
                let next_feat = &phase.features[feature_idx + 1];
                let next_line = match find_feature_line(&lines, &next_feat.title) {
                    Some(idx) => idx,
                    None => return Ok(false),
                };
                new_lines.swap(source_line, next_line);
            } else if phase_idx + 1 < phases.len() {
                // Cross-phase: move to start of next phase
                let next_phase = &phases[phase_idx + 1];
                if next_phase.features.is_empty() {
                    let target = match find_phase_header_line(&lines, &next_phase.name) {
                        Some(idx) => idx + 1,
                        None => return Ok(false),
                    };
                    let removed = new_lines.remove(source_line);
                    let insert_at = if source_line < target { target - 1 } else { target };
                    new_lines.insert(insert_at, removed);
                } else {
                    let first_feat = &next_phase.features[0];
                    let target = match find_feature_line(&lines, &first_feat.title) {
                        Some(idx) => idx,
                        None => return Ok(false),
                    };
                    let removed = new_lines.remove(source_line);
                    let insert_at = if source_line < target { target - 1 } else { target };
                    new_lines.insert(insert_at, removed);
                }
            } else {
                return Ok(false); // already at bottom
            }
        }
    }

    let mut output = new_lines.join("\n");
    if contents.ends_with('\n') {
        output.push('\n');
    }
    std::fs::write(plan_path, output)?;
    Ok(true)
}

/// Append a new feature to a specific phase in Plan.md.
///
/// Auto-assigns the next sequential feature number (max existing + 1).
/// Appends `N. [ ] **title** — description` after the last feature in the phase.
pub fn append_feature_to_plan(
    plan_path: &std::path::Path,
    phase_idx: usize,
    title: &str,
    description: &str,
) -> std::io::Result<bool> {
    let contents = std::fs::read_to_string(plan_path)?;
    let phases = parse_plan(&contents);

    let phase = match phases.get(phase_idx) {
        Some(p) => p,
        None => return Ok(false),
    };

    let lines: Vec<&str> = contents.lines().collect();

    // Compute next feature number: max across ALL phases + 1
    let max_id = phases.iter()
        .flat_map(|p| p.features.iter())
        .filter_map(|f| f.id)
        .max()
        .unwrap_or(0);
    let next_id = max_id + 1;

    // Build the new feature line
    let new_line = if description.is_empty() {
        format!("{}. [ ] **{}**", next_id, title)
    } else {
        format!("{}. [ ] **{}** — {}", next_id, title, description)
    };

    // Find where to insert: after the last feature of this phase, or after the phase header
    let insert_after = if phase.features.is_empty() {
        // After the phase header line
        find_phase_header_line(&lines, &phase.name)
    } else {
        // After the last feature in this phase
        let last_feat = phase.features.last().unwrap();
        find_feature_line(&lines, &last_feat.title)
    };

    let insert_after = match insert_after {
        Some(idx) => idx,
        None => return Ok(false),
    };

    let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    new_lines.insert(insert_after + 1, new_line);

    let mut output = new_lines.join("\n");
    if contents.ends_with('\n') {
        output.push('\n');
    }
    std::fs::write(plan_path, output)?;
    Ok(true)
}

/// Find the line index of a feature by its title.
fn find_feature_line(lines: &[&str], title: &str) -> Option<usize> {
    for (i, line) in lines.iter().enumerate() {
        if line.contains(&format!("**{title}**")) {
            return Some(i);
        }
    }
    None
}

/// Find the line index of a phase header by its name.
fn find_phase_header_line(lines: &[&str], phase_name: &str) -> Option<usize> {
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if (trimmed.starts_with("## ") || trimmed.starts_with("### ")) && trimmed.contains(phase_name) {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_done_feature() {
        let f = try_parse_feature_line("1. [x] **Core process manager** — spawn/kill/monitor").unwrap();
        assert_eq!(f.id, Some(1));
        assert_eq!(f.title, "Core process manager");
        assert_eq!(f.status, FeatureStatus::Done);
    }

    #[test]
    fn test_parse_pending_feature() {
        let f = try_parse_feature_line("44. [ ] **Plan.md syntax parser** — parse into tree").unwrap();
        assert_eq!(f.id, Some(44));
        assert_eq!(f.title, "Plan.md syntax parser");
        assert_eq!(f.status, FeatureStatus::Planned);
    }

    #[test]
    fn test_parse_deprecated() {
        let f = try_parse_feature_line("15. [ ] **Template selector** — *DEPRECATED. Replaced by CP-4.*").unwrap();
        assert_eq!(f.status, FeatureStatus::Deprecated);
    }

    #[test]
    fn test_parse_cp_feature() {
        let f = try_parse_feature_line("CP-1. [x] **Workflow skills** — Convert workflow definitions").unwrap();
        assert_eq!(f.id, None);
        assert_eq!(f.title, "Workflow skills");
        assert_eq!(f.status, FeatureStatus::Done);
    }

    #[test]
    fn test_parse_unnumbered() {
        let f = try_parse_feature_line("- [ ] **Agent profile management** — swappable profiles").unwrap();
        assert_eq!(f.id, None);
        assert_eq!(f.title, "Agent profile management");
        assert_eq!(f.status, FeatureStatus::Planned);
    }

    #[test]
    fn test_phase_header() {
        let p = try_parse_phase_header("## Phase 4: Multi-Provider & Resource Management (1.5.0)").unwrap();
        assert_eq!(p.number, Some(4));
        assert!(p.name.contains("Multi-Provider"));
    }

    #[test]
    fn test_critical_path_header() {
        let p = try_parse_phase_header("### CRITICAL PATH — Skill-Based Workflow Execution (blocks all orchestration)").unwrap();
        assert!(p.name.contains("CRITICAL PATH"));
        assert_eq!(p.number, None);
    }

    #[test]
    fn test_full_parse() {
        let content = r#"# Test Plan

## Phase 0: Foundation (1.0.0)

1. [x] **Panel restructuring** — updated panels
2. [ ] **Config migration** — loads from config.json

## Phase 1: Agents (1.1.0)

3. [x] **Agent profiles** — .md files with YAML frontmatter
4. [ ] **Agent binding** — *DEPRECATED. Replaced.*

### Cross-Cutting: Dev Map

44. [ ] **Plan parser** — parse Plan.md into tree
"#;
        let phases = parse_plan(content);
        assert_eq!(phases.len(), 3);
        assert_eq!(phases[0].name, "Foundation (1.0.0)");
        assert_eq!(phases[0].number, Some(0));
        assert_eq!(phases[0].features.len(), 2);
        assert_eq!(phases[0].done_count(), 1);
        assert_eq!(phases[1].features.len(), 2);
        assert_eq!(phases[1].features[1].status, FeatureStatus::Deprecated);
        assert_eq!(phases[2].features.len(), 1);
        assert_eq!(phases[2].features[0].id, Some(44));
    }
}
