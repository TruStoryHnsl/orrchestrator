/// Structured parser for Plan.md files.
/// Extracts phases, features, and status into a navigable tree.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureStatus {
    Pending,
    Done,
    Deprecated,
    InProgress,
}

impl FeatureStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Done => "[x]",
            Self::Pending => "[ ]",
            Self::Deprecated => "[D]",
            Self::InProgress => "[~]",
        }
    }
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
        self.features.iter().filter(|f| f.status == FeatureStatus::Done).count()
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

    // Must have a checkbox
    let (done, after_checkbox) = if rest.starts_with("[x]") || rest.starts_with("[X]") {
        (true, rest[3..].trim_start())
    } else if rest.starts_with("[ ]") {
        (false, rest[3..].trim_start())
    } else {
        return None;
    };

    // Parse title and description
    let (title, description) = parse_title_description(after_checkbox);

    if title.is_empty() {
        return None;
    }

    // Determine status
    let status = if done {
        FeatureStatus::Done
    } else if description.to_uppercase().contains("DEPRECATED")
        || description.contains("MOVED")
        || title.to_uppercase().contains("DEPRECATED")
    {
        FeatureStatus::Deprecated
    } else if description.contains("deferred")
        || description.contains("requires ")
        || description.contains("needs ")
    {
        // Items that mention deferred/requires are blocked, show as pending
        FeatureStatus::Pending
    } else {
        FeatureStatus::Pending
    };

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
        assert_eq!(f.status, FeatureStatus::Pending);
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
        assert_eq!(f.status, FeatureStatus::Pending);
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
