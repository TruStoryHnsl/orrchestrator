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
    Planned,       // [ ] — not started
    Implementing,  // [~] — work in progress
    Implemented,   // [=] — code done, not tested
    Testing,       // [t] — tests running
    Verified,      // [v] — tests passed
    UserConfirmed, // [✓] — user manually confirmed
    Done,          // [x] — shipped
    Deprecated,    // strikethrough text
    Pending,       // alias for Planned (backward compat)
    InProgress,    // alias for Implementing (backward compat)
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
        matches!(
            self,
            Self::Planned
                | Self::Pending
                | Self::Implementing
                | Self::InProgress
                | Self::Implemented
                | Self::Testing
        )
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
            Self::Deprecated => "[ ]", // deprecated indicated by strikethrough, not marker
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
    if s.len() >= 3 && s.is_char_boundary(3) {
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
    /// True when the feature was marked with `[v]` (user-verified). Distinct
    /// from status because a feature can progress past Verified while still
    /// carrying its "user verified" provenance.
    pub user_verified: bool,
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
    let mut region = RoadmapRegion::Outside;

    for line in content.lines() {
        let trimmed = line.trim();

        if let Some((level, heading)) = markdown_heading(trimmed) {
            if level == 2 && is_roadmap_region_heading(heading) {
                flush_phase(&mut phases, &mut current_phase);
                region = RoadmapRegion::Explicit;
                last_feature = None;
                continue;
            }

            if region == RoadmapRegion::Outside && is_implicit_roadmap_phase(level, heading) {
                flush_phase(&mut phases, &mut current_phase);
                region = RoadmapRegion::Implicit;
                current_phase = Some(parse_phase_heading(heading));
                last_feature = None;
                continue;
            }

            if region == RoadmapRegion::Explicit {
                if level == 3 || (level == 2 && is_phase_like_heading(heading)) {
                    flush_phase(&mut phases, &mut current_phase);
                    current_phase = Some(parse_phase_heading(heading));
                    last_feature = None;
                    continue;
                }

                if level == 2 {
                    flush_phase(&mut phases, &mut current_phase);
                    region = RoadmapRegion::Outside;
                    last_feature = None;
                    continue;
                }
            }

            if region == RoadmapRegion::Implicit {
                if is_implicit_roadmap_phase(level, heading) {
                    flush_phase(&mut phases, &mut current_phase);
                    current_phase = Some(parse_phase_heading(heading));
                    last_feature = None;
                    continue;
                }

                if level <= 2 {
                    flush_phase(&mut phases, &mut current_phase);
                    region = RoadmapRegion::Outside;
                    last_feature = None;
                    continue;
                }
            }

            last_feature = None;
            continue;
        }

        // Detect feature lines within a phase
        if region != RoadmapRegion::Outside && current_phase.is_some() {
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

    flush_phase(&mut phases, &mut current_phase);

    phases
}

/// Return read-only warnings for PLAN.md content that will not parse cleanly.
pub fn lint_plan(content: &str) -> Vec<String> {
    use std::collections::BTreeSet;

    let mut warnings = Vec::new();
    let mut region = RoadmapRegion::Outside;
    let mut found_region = false;
    let mut current_phase: Option<(String, usize)> = None;
    let mut non_roadmap_heading: Option<String> = None;
    let mut ignored_numbered_headings = BTreeSet::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if let Some((level, heading)) = markdown_heading(trimmed) {
            if level == 2 && is_roadmap_region_heading(heading) {
                flush_lint_phase(&mut warnings, &mut current_phase);
                found_region = true;
                region = RoadmapRegion::Explicit;
                non_roadmap_heading = None;
                continue;
            }

            if region == RoadmapRegion::Outside && is_implicit_roadmap_phase(level, heading) {
                flush_lint_phase(&mut warnings, &mut current_phase);
                found_region = true;
                region = RoadmapRegion::Implicit;
                current_phase = Some((parse_phase_heading(heading).name, 0));
                non_roadmap_heading = None;
                continue;
            }

            if region == RoadmapRegion::Explicit {
                if level == 3 || (level == 2 && is_phase_like_heading(heading)) {
                    flush_lint_phase(&mut warnings, &mut current_phase);
                    current_phase = Some((parse_phase_heading(heading).name, 0));
                    continue;
                }

                if level == 2 {
                    flush_lint_phase(&mut warnings, &mut current_phase);
                    region = RoadmapRegion::Outside;
                    non_roadmap_heading = Some(heading.to_string());
                    continue;
                }
            }

            if region == RoadmapRegion::Implicit {
                if is_implicit_roadmap_phase(level, heading) {
                    flush_lint_phase(&mut warnings, &mut current_phase);
                    current_phase = Some((parse_phase_heading(heading).name, 0));
                    continue;
                }

                if level <= 2 {
                    flush_lint_phase(&mut warnings, &mut current_phase);
                    region = RoadmapRegion::Outside;
                    non_roadmap_heading = Some(heading.to_string());
                    continue;
                }
            }

            if region == RoadmapRegion::Outside {
                non_roadmap_heading = Some(heading.to_string());
            }
            continue;
        }

        if region != RoadmapRegion::Outside {
            if current_phase.is_some()
                && try_parse_feature_line(trimmed).is_some()
                && let Some((_, count)) = current_phase.as_mut()
            {
                *count += 1;
            }
        } else if is_numbered_list_line(trimmed) {
            let heading = non_roadmap_heading.as_deref().unwrap_or("document");
            if ignored_numbered_headings.insert(heading.to_string()) {
                warnings.push(format!(
                    "numbered list under non-roadmap heading '{heading}' ignored — move under a roadmap phase to track it"
                ));
            }
        }
    }

    flush_lint_phase(&mut warnings, &mut current_phase);

    if !found_region {
        warnings.insert(0, "no roadmap region found".to_string());
    }

    warnings
}

fn flush_lint_phase(warnings: &mut Vec<String>, current_phase: &mut Option<(String, usize)>) {
    if let Some((phase_name, feature_count)) = current_phase.take()
        && feature_count == 0
    {
        warnings.push(format!("phase '{phase_name}' has 0 features"));
    }
}

fn is_numbered_list_line(line: &str) -> bool {
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    digits > 0
        && line
            .get(digits..)
            .is_some_and(|rest| rest.starts_with(". "))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoadmapRegion {
    Outside,
    Explicit,
    Implicit,
}

fn flush_phase(phases: &mut Vec<PlanPhase>, current_phase: &mut Option<PlanPhase>) {
    if let Some(p) = current_phase.take()
        && !p.features.is_empty()
    {
        phases.push(p);
    }
}

fn markdown_heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if !(2..=6).contains(&level) {
        return None;
    }

    let rest = line.get(level..)?;
    if !rest.starts_with(' ') {
        return None;
    }

    Some((level, rest.trim()))
}

fn normalized_heading(heading: &str) -> String {
    heading
        .replace("~~", "")
        .trim()
        .trim_end_matches('#')
        .trim()
        .to_lowercase()
}

fn is_roadmap_region_heading(heading: &str) -> bool {
    const ROADMAP_HEADINGS: &[&str] = &[
        "feature roadmap",
        "roadmap",
        "development phases",
        "phases",
        "plan",
        "milestones",
        "critical path",
    ];

    let normalized = normalized_heading(heading);
    if let Some(pos) = normalized.find("feature roadmap") {
        let prefix = normalized[..pos].trim();
        if !prefix.is_empty() && prefix.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
            return true;
        }
    }

    ROADMAP_HEADINGS.iter().any(|candidate| {
        normalized == *candidate
            || normalized.strip_prefix(candidate).is_some_and(|rest| {
                let rest = rest.trim_start();
                rest.starts_with(':')
                    || rest.starts_with('-')
                    || rest.starts_with('—')
                    || rest.starts_with('–')
                    || rest.starts_with('(')
            })
    })
}

fn is_phase_like_heading(heading: &str) -> bool {
    let lower = normalized_heading(heading);
    lower.starts_with("phase ")
        || lower.contains("critical path")
        || lower.contains("cross-cutting")
        || lower.contains("cross cutting")
}

fn is_implicit_roadmap_phase(level: usize, heading: &str) -> bool {
    (level == 2 || level == 3) && is_phase_like_heading(heading)
}

/// Try to parse a line as a phase/section header.
#[cfg(test)]
fn try_parse_phase_header(line: &str) -> Option<PlanPhase> {
    let (level, rest) = markdown_heading(line)?;
    if level == 2 || level == 3 {
        Some(parse_phase_heading(rest))
    } else {
        None
    }
}

fn parse_phase_heading(heading: &str) -> PlanPhase {
    let clean = heading.replace("~~", "").trim().to_string();
    let lower = clean.to_lowercase();

    if let Some(after_phase) = lower.strip_prefix("phase ") {
        let num_str: String = after_phase
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let number = num_str.parse::<u8>().ok();

        let paren_pos = clean.find('(').unwrap_or(clean.len());
        let name_start = clean
            .find('—')
            .or_else(|| clean.find('–'))
            .or_else(|| clean.find(':').filter(|&p| p < paren_pos));
        let name = if let Some(pos) = name_start {
            clean[pos + clean[pos..].chars().next().unwrap().len_utf8()..]
                .trim()
                .to_string()
        } else {
            clean.clone()
        };

        return PlanPhase {
            name,
            number,
            features: Vec::new(),
        };
    }

    PlanPhase {
        name: clean,
        number: None,
        features: Vec::new(),
    }
}

/// Try to parse a feature line.
fn try_parse_feature_line(line: &str) -> Option<PlanFeature> {
    // Patterns (checkbox form):
    //   N. [x] **Title** — description
    //   N. [ ] **Title** — description
    //   CP-N. [x] **Title** — description
    //   - [x] **Title** — description
    //   - [ ] **Title** — description
    //
    // Plain-bullet fallback (treated as Planned):
    //   - Title text
    //   - **Title** — description
    //   N. Title text
    //
    // The plain-bullet fallback lets projects like porrtal and orradash that
    // use non-checkbox bullets still populate their phase trees. Callers only
    // invoke this inside a recognised phase (see parse_plan), so non-feature
    // bullets in other contexts are not mistaken for features.

    let trimmed = line.trim();

    if let Some(feature) = try_parse_table_feature_line(trimmed) {
        return Some(feature);
    }

    // Strip the leading prefix to get to the checkbox
    let (id, rest) = strip_feature_prefix(trimmed)?;

    // Parse status marker if present; otherwise fall back to a plain bullet
    // treated as a Planned feature.
    let (mut status, after_checkbox): (FeatureStatus, &str) =
        if let Some((s, n)) = parse_status_marker(rest) {
            (s, rest[n..].trim_start())
        } else {
            // Plain bullet: reject lines that look like section metadata
            // (empty, a sub-heading, a horizontal rule, or a table separator).
            if rest.is_empty()
                || rest.starts_with('#')
                || rest.starts_with("---")
                || rest.starts_with('|')
                || rest.starts_with('>')
                || rest.starts_with("```")
            {
                return None;
            }
            (FeatureStatus::Planned, rest)
        };

    // Parse title and description
    let (title, description) = parse_title_description(after_checkbox);

    if title.is_empty() {
        return None;
    }

    // Override status for deprecated/deferred items (text-based detection)
    if status == FeatureStatus::Planned
        && (description.to_uppercase().contains("DEPRECATED")
            || description.contains("MOVED")
            || title.to_uppercase().contains("DEPRECATED"))
    {
        status = FeatureStatus::Deprecated;
    }

    // `[v]` is the only parser path that produces Verified, so any Verified
    // status at this point implies the feature was explicitly user-verified.
    let user_verified = status == FeatureStatus::Verified;

    Some(PlanFeature {
        id,
        title,
        description,
        status,
        user_verified,
    })
}

fn try_parse_table_feature_line(line: &str) -> Option<PlanFeature> {
    if !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }

    let cells: Vec<&str> = line
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim())
        .collect();

    if cells.len() < 3
        || cells
            .iter()
            .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':' || c == ' '))
    {
        return None;
    }

    let status_cell = cells
        .iter()
        .position(|cell| parse_status_cell(cell).is_some())?;
    let (status, _) = parse_status_cell(cells[status_cell])?;

    let title_cell = if cells.len() >= 2 && cells[0].eq_ignore_ascii_case("id") {
        return None;
    } else if status_cell >= 1 {
        cells.get(1).copied().unwrap_or("")
    } else {
        cells
            .iter()
            .enumerate()
            .find(|(idx, cell)| *idx != status_cell && !cell.is_empty())
            .map(|(_, cell)| *cell)
            .unwrap_or("")
    };

    let (title, parsed_description) = parse_title_description(title_cell);
    if title.is_empty() {
        return None;
    }

    let id = cells.first().and_then(|cell| cell.parse::<u32>().ok());
    let description = if parsed_description.is_empty() {
        cells
            .iter()
            .enumerate()
            .filter(|(idx, cell)| *idx != status_cell && *idx != 0 && *idx != 1 && !cell.is_empty())
            .map(|(_, cell)| *cell)
            .collect::<Vec<_>>()
            .join(" | ")
    } else {
        parsed_description
    };
    let user_verified = status == FeatureStatus::Verified;

    Some(PlanFeature {
        id,
        title,
        description,
        status,
        user_verified,
    })
}

fn parse_status_cell(cell: &str) -> Option<(FeatureStatus, usize)> {
    let trimmed = cell.trim().trim_matches('`').trim();
    parse_status_marker(trimmed)
}

/// Strip the leading numbering prefix and return (optional id, remaining text).
fn strip_feature_prefix(line: &str) -> Option<(Option<u32>, &str)> {
    let trimmed = line.trim_start();

    // "CP-N." prefix
    if let Some(after_cp) = trimmed.strip_prefix("CP-") {
        let num_str: String = after_cp
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
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
    if let Some(after_open) = text.strip_prefix("**")
        && let Some(close_pos) = after_open.find("**")
    {
        let title = after_open[..close_pos].to_string();
        let desc = after_open[close_pos + 2..]
            .trim_start_matches([' ', '—', '-', '–'])
            .trim()
            .to_string();
        return (title, desc);
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
                    let insert_at = if source_line < target {
                        target - 1
                    } else {
                        target
                    };
                    new_lines.insert(insert_at, removed);
                } else {
                    let last_feat = prev_phase.features.last().unwrap();
                    let target = match find_feature_line(&lines, &last_feat.title) {
                        Some(idx) => idx,
                        None => return Ok(false),
                    };
                    let removed = new_lines.remove(source_line);
                    let insert_at = if source_line < target {
                        target
                    } else {
                        target + 1
                    };
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
                    let insert_at = if source_line < target {
                        target - 1
                    } else {
                        target
                    };
                    new_lines.insert(insert_at, removed);
                } else {
                    let first_feat = &next_phase.features[0];
                    let target = match find_feature_line(&lines, &first_feat.title) {
                        Some(idx) => idx,
                        None => return Ok(false),
                    };
                    let removed = new_lines.remove(source_line);
                    let insert_at = if source_line < target {
                        target - 1
                    } else {
                        target
                    };
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
    let max_id = phases
        .iter()
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

/// Mark a feature as user-verified in PLAN.md by flipping its `[x]` marker
/// to `[v]`. Loose match on the title (trim + contains). Returns true if a
/// change was written.
///
/// Preserves all other bytes in the file exactly — only the single `[x]`
/// token on the matched line is replaced.
pub fn mark_verified_in_plan(
    plan_path: &std::path::Path,
    feature_title: &str,
) -> std::io::Result<bool> {
    let contents = std::fs::read_to_string(plan_path)?;
    let needle = feature_title.trim();
    if needle.is_empty() {
        return Ok(false);
    }

    // Work on raw bytes / slices so we can reconstruct exactly.
    let mut out = String::with_capacity(contents.len());
    let mut changed = false;

    for line in contents.split_inclusive('\n') {
        // Strip the trailing newline (if any) to inspect the line content,
        // but keep it for the output.
        let (body, nl) = if let Some(stripped) = line.strip_suffix('\n') {
            (stripped, "\n")
        } else {
            (line, "")
        };

        if !changed && body.contains("[x]") && body.contains(needle) {
            // Replace ONLY the first occurrence of `[x]` on this line.
            if let Some(pos) = body.find("[x]") {
                let mut replaced = String::with_capacity(body.len());
                replaced.push_str(&body[..pos]);
                replaced.push_str("[v]");
                replaced.push_str(&body[pos + 3..]);
                out.push_str(&replaced);
                out.push_str(nl);
                changed = true;
                continue;
            }
        }

        out.push_str(body);
        out.push_str(nl);
    }

    if changed {
        std::fs::write(plan_path, out)?;
    }
    Ok(changed)
}

/// Rename a feature in PLAN.md in-place, preserving the status marker and surrounding text.
///
/// Finds the line containing `**old_title**` and replaces it with `**new_title**`.
/// All other bytes are preserved exactly.
pub fn rename_feature_in_plan(
    plan_path: &std::path::Path,
    old_title: &str,
    new_title: &str,
) -> std::io::Result<bool> {
    let contents = std::fs::read_to_string(plan_path)?;
    let old_needle = format!("**{}**", old_title.trim());
    let new_replacement = format!("**{}**", new_title.trim());

    if old_needle == new_replacement {
        return Ok(false);
    }

    let mut out = String::with_capacity(contents.len());
    let mut changed = false;

    for line in contents.split_inclusive('\n') {
        let (body, nl) = if let Some(stripped) = line.strip_suffix('\n') {
            (stripped, "\n")
        } else {
            (line, "")
        };

        if !changed && body.contains(&old_needle) {
            let replaced = body.replacen(&old_needle, &new_replacement, 1);
            out.push_str(&replaced);
            out.push_str(nl);
            changed = true;
        } else {
            out.push_str(body);
            out.push_str(nl);
        }
    }

    if changed {
        std::fs::write(plan_path, out)?;
    }
    Ok(changed)
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
        if (trimmed.starts_with("## ") || trimmed.starts_with("### "))
            && trimmed.contains(phase_name)
        {
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
        let f =
            try_parse_feature_line("1. [x] **Core process manager** — spawn/kill/monitor").unwrap();
        assert_eq!(f.id, Some(1));
        assert_eq!(f.title, "Core process manager");
        assert_eq!(f.status, FeatureStatus::Done);
    }

    #[test]
    fn test_parse_pending_feature() {
        let f =
            try_parse_feature_line("44. [ ] **Plan.md syntax parser** — parse into tree").unwrap();
        assert_eq!(f.id, Some(44));
        assert_eq!(f.title, "Plan.md syntax parser");
        assert_eq!(f.status, FeatureStatus::Planned);
    }

    #[test]
    fn test_parse_deprecated() {
        let f = try_parse_feature_line(
            "15. [ ] **Template selector** — *DEPRECATED. Replaced by CP-4.*",
        )
        .unwrap();
        assert_eq!(f.status, FeatureStatus::Deprecated);
    }

    #[test]
    fn test_parse_cp_feature() {
        let f =
            try_parse_feature_line("CP-1. [x] **Workflow skills** — Convert workflow definitions")
                .unwrap();
        assert_eq!(f.id, None);
        assert_eq!(f.title, "Workflow skills");
        assert_eq!(f.status, FeatureStatus::Done);
    }

    #[test]
    fn test_parse_unnumbered() {
        let f = try_parse_feature_line("- [ ] **Agent profile management** — swappable profiles")
            .unwrap();
        assert_eq!(f.id, None);
        assert_eq!(f.title, "Agent profile management");
        assert_eq!(f.status, FeatureStatus::Planned);
    }

    #[test]
    fn test_parse_plain_bullet_feature() {
        // Plain dash bullet with no checkbox — porrtal-style PLAN.md.
        let f = try_parse_feature_line("- Project scaffolding, tech stack decision").unwrap();
        assert_eq!(f.id, None);
        assert_eq!(f.title, "Project scaffolding, tech stack decision");
        assert_eq!(f.status, FeatureStatus::Planned);
        assert!(!f.user_verified);
    }

    #[test]
    fn test_parse_plain_numbered_feature() {
        let f = try_parse_feature_line("3. Ship the thing").unwrap();
        assert_eq!(f.id, Some(3));
        assert_eq!(f.title, "Ship the thing");
        assert_eq!(f.status, FeatureStatus::Planned);
    }

    #[test]
    fn test_parse_plain_bullet_rejects_headers_and_separators() {
        // After prefix strip these still look like section metadata, so they
        // must not become features.
        assert!(try_parse_feature_line("- ").is_none());
        assert!(try_parse_feature_line("- ---").is_none());
        assert!(try_parse_feature_line("- # nope").is_none());
    }

    #[test]
    fn test_plan_with_plain_bullet_phase() {
        // Porrtal-style: ### Phase N followed by plain bullets. The parser
        // should yield one phase containing all three bullets as Planned
        // features, so the project shows up in Design > Plans.
        let content = r#"# Porrtal — Plan

## Phases

### Phase 1: Shell
- Project scaffolding, tech stack decision
- Basic layout with navigation
- Health status indicators

### Phase 2: Integration
- Embed key views
"#;
        let phases = parse_plan(content);
        assert!(
            phases.len() >= 2,
            "expected both phases to be parsed, got {phases:?}"
        );
        let phase1 = phases
            .iter()
            .find(|p| p.name.contains("Shell"))
            .expect("Phase 1 not found");
        assert_eq!(phase1.features.len(), 3);
        assert!(
            phase1
                .features
                .iter()
                .all(|f| f.status == FeatureStatus::Planned)
        );
    }

    #[test]
    fn test_phase_header() {
        let p = try_parse_phase_header("## Phase 4: Multi-Provider & Resource Management (1.5.0)")
            .unwrap();
        assert_eq!(p.number, Some(4));
        assert!(p.name.contains("Multi-Provider"));
    }

    #[test]
    fn test_critical_path_header() {
        let p = try_parse_phase_header(
            "### CRITICAL PATH — Skill-Based Workflow Execution (blocks all orchestration)",
        )
        .unwrap();
        assert!(p.name.contains("CRITICAL PATH"));
        assert_eq!(p.number, None);
    }

    #[test]
    fn test_parse_verified_sets_user_verified() {
        let content = r#"# Test Plan

## Phase 0: Foundation (1.0.0)

1. [v] **Verified feature** — has been user-verified
2. [x] **Done feature** — shipped
"#;
        let phases = parse_plan(content);
        assert_eq!(phases.len(), 1);
        let feats = &phases[0].features;
        assert_eq!(feats.len(), 2);
        assert_eq!(feats[0].status, FeatureStatus::Verified);
        assert!(
            feats[0].user_verified,
            "[v] feature should be user_verified"
        );
        assert_eq!(feats[1].status, FeatureStatus::Done);
        assert!(
            !feats[1].user_verified,
            "[x] feature must not be user_verified"
        );
    }

    #[test]
    fn test_mark_verified_in_plan_flips_marker() {
        let tmp = std::env::temp_dir().join(format!(
            "orrch_plan_mark_verified_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let plan_path = tmp.join("PLAN.md");

        let original = "# Plan\n\n## Phase 0: Test\n\n1. [x] **My Feature** — did a thing\n2. [x] **Other Feature** — untouched\n";
        std::fs::write(&plan_path, original).unwrap();

        let changed = mark_verified_in_plan(&plan_path, "My Feature").unwrap();
        assert!(changed, "should return true when a change was made");

        let after = std::fs::read_to_string(&plan_path).unwrap();
        let expected = "# Plan\n\n## Phase 0: Test\n\n1. [v] **My Feature** — did a thing\n2. [x] **Other Feature** — untouched\n";
        assert_eq!(
            after, expected,
            "only the matched line's [x] should become [v]"
        );

        // Idempotency: second call should find no `[x]` matching the title and return false.
        let changed2 = mark_verified_in_plan(&plan_path, "My Feature").unwrap();
        assert!(!changed2, "second call should be a no-op");

        let _ = std::fs::remove_dir_all(&tmp);
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

    #[test]
    fn test_parse_status_marker_handles_leading_multibyte_char() {
        // Regression: PLAN.md lines beginning with a 4-byte emoji like 🟢
        // previously panicked at `&s[..3]` because byte 3 lands inside the
        // codepoint. Observed crash on 2026-04-27 against the line
        // "🟢 WebUI port: PM proposes 8492. ..." in PLAN.md.
        assert_eq!(
            parse_status_marker("🟢 WebUI port: PM proposes 8492."),
            None
        );
        assert_eq!(parse_status_marker("é foo"), None);
    }

    fn feature_count(phases: &[PlanPhase]) -> usize {
        phases.iter().map(|phase| phase.features.len()).sum()
    }

    #[test]
    fn test_lint_plan_warns_for_missing_roadmap_and_ignored_numbered_list() {
        let warnings = lint_plan(
            r#"## Open Conflicts

### Native UI Rebuild Decisions
1. **Toolkit selection is reopened.**
2. **Native voice path is unresolved.**
"#,
        );

        assert!(
            warnings
                .iter()
                .any(|warning| warning == "no roadmap region found")
        );
        assert!(warnings.iter().any(|warning| warning.contains(
            "numbered list under non-roadmap heading 'Native UI Rebuild Decisions' ignored"
        )));
    }

    #[test]
    fn test_lint_plan_warns_for_empty_phase() {
        let warnings = lint_plan(
            r#"## Feature Roadmap

### Phase 1: Empty

### Phase 2: Work
- [ ] Ship it
"#,
        );

        assert!(
            warnings
                .iter()
                .any(|warning| warning == "phase 'Empty' has 0 features")
        );
        assert!(
            !warnings
                .iter()
                .any(|warning| warning == "phase 'Work' has 0 features")
        );
    }

    #[test]
    fn fixture_chatapp_open_conflicts_not_scooped() {
        let open_conflicts = r#"# Concord — Master Development Plan

## Open Conflicts

### Native UI Rebuild Decisions — Open (2026-06-01)
1. **Toolkit selection is reopened for the native-shell track.** The older "default to Tauri v2" decision still covers the transitional webview builds and packaging pipeline, but it does not settle the new native app shell plus managed-webview surface strategy. PM must drive evidence-based spikes for the top candidates from `docs/architecture/native-ui-rebuild-scope.md` (Slint / gpui / iced / Flutter + flutter_rust_bridge) before implementation.
2. **Native voice path is unresolved.** Decide between LiveKit Rust SDK (libwebrtc-backed, heavier but highest parity) and finishing the existing `webrtc-rs` path (pure Rust, parity risk) through a live audio spike against a docker deployment.
3. **Platform scope of native-UI v1 is unresolved.** Decide whether v1 is desktop-only first or desktop + mobile from the start; this choice affects toolkit selection, release schedule, and tvOS/mobile reuse.
4. **Parity bar for retiring the webview-only shell is unresolved.** Do not remove the existing webview build for any platform until the required native surfaces are defined, implemented, and observed working surface-by-surface.
5. **Managed webview boundary is unresolved.** Native clients must support interactive webview surfaces for channel apps/extensions. PM must decide which surfaces are native-rendered, which are managed-webview-rendered, and whether the chat display remains a web-rendered extensibility surface.
"#;
        assert_eq!(feature_count(&parse_plan(open_conflicts)), 0);

        let roadmap = r#"## Feature Roadmap

### TOP PRIORITY: P2P-first native architecture (routed 2026-05-27)

**Source of truth: [`docs/architecture/p2p-design.md`](docs/architecture/p2p-design.md).** This is the largest single architectural shift on the roadmap; orrchestrator sequences these phases one at a time, each ending in a shippable state. Earlier roadmap items below remain relevant — voice subsystem health, mobile UI polish, settings, extensions, etc. — but the P2P phases below are the dominant track until they land. Phase 0 already shipped in this branch (2026-05-27); Phases 2–9 are the orrchestrator's queue.

- [x] **Phase 0 — Web-compat hardening (shipped 2026-05-27, branch `chore/architecture-cleanup-7f3a`):** `INSTANCE_DOMAIN` auto-derived from `PUBLIC_BASE_URL`; `TURN_HOST` derives to `turn.<INSTANCE_DOMAIN>` and refuses RFC1918 values with logged warning; `services/voice_health.py` runs a non-blocking background STUN probe every 10 min and caches the snapshot.
- [ ] **Phase 1 — P2P architecture design doc** (shipped 2026-05-27 as `docs/architecture/p2p-design.md`; captured here as a phase rather than separately marked completed for orrchestrator's accounting purposes).
- [x] **Phase 2 — Peer identity scaffolding.** Ed25519 keypair on first launch, persisted in `tauri-plugin-stronghold`. Public-key fingerprint exposed via Tauri command + Settings UI.
"#;
        let phases = parse_plan(&format!("{open_conflicts}\n{roadmap}"));
        assert_eq!(phases.len(), 1);
        assert_eq!(feature_count(&phases), 3);
        assert!(
            !phases[0]
                .features
                .iter()
                .any(|feature| feature.title.contains("Toolkit selection"))
        );
    }

    #[test]
    fn fixture_sampleapp_done_and_planned_are_phases() {
        let content = r#"## Feature Roadmap

### Done
- **Headless Image Generation API** — `POST /orragen/api/generate`, auth-locked with dual-key (`X-Dashboard-Key` + `X-Orragen-Key`). Profiles in `orragen/data/image_profiles.yaml`. (2026-03-27)
- **Model filename display layer** — cleaned descriptive titles, compatibility/expertise info at a glance (2026-03-28)
- **Parameter sweep from main menu** — exploration tab removed, sweeps integrated into image gen with per-LoRA weight control (2026-03-28)

### Planned

#### INS-001: Create nextapp repo for parallel development
- [x] Create a separate private GitHub repo for nextapp (`example-org/nextapp`)
- [x] No changes land on sampleapp main until feature-g is proven stable and recorder is fully replaceable
- [x] nextapp is its own independent application (Rust successor), not a fork — integration points wired in nextapp repo directly
- [x] feature-g code and recorder replacement live in nextapp repo
"#;
        let phases = parse_plan(content);
        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0].name, "Done");
        assert_eq!(phases[0].features.len(), 3);
        assert_eq!(phases[1].name, "Planned");
        assert_eq!(phases[1].features.len(), 4);
    }

    #[test]
    fn fixture_portfolio_fr_headings_are_phases() {
        let content = r#"## Feature Roadmap

Build order is: bug fix first → schema refactor (foundation for everything
else) → individual entry types in dependency order → project workflow
(consumes the typed entries) → promotion + resume-ingest rules.

### FR-001 — Fix de-dup merge 500
- Status: `[x] done` (commit `9768ba1`, 2026-05-05)
- Diagnose root cause via server logs, fix the merge handler in
  `portfolio/web/routes.py` (and helpers), add a regression test that
  exercises the merge endpoint end-to-end and asserts non-500.
- Source: INS-001.

### FR-002 — Typed-entry schema refactor with shared location/time fields
- Status: `[x] done` (commit `85a42b6`, 2026-05-05) — chose option (a) per-type tables; six typed-entry tables added with shared `city/state/date/time`. Polymorphic `project_entry_link` join added (additive — legacy `project_item_display` unchanged). No row migration; all new tables empty.
- Foundation work for FR-003..FR-008. Decide table layout (per-type vs
  parent+child) during design. Add `city`, `state`, `date`, `time` columns to
  every entry type. Migration must preserve existing `portfolio_item`,
  `employment`, and `project` rows.
- Source: INS-002.
"#;
        let phases = parse_plan(content);
        assert_eq!(phases.len(), 2);
        assert!(phases[0].name.starts_with("FR-001"));
        assert!(phases[1].name.starts_with("FR-002"));
        assert_eq!(feature_count(&phases), 6);
    }

    #[test]
    fn fixture_orracle_development_phases_only() {
        let content = r#"## Queued Features

### Headless Image Generation API (for orradash integration)
**Source**: orradash feedback pipeline 2026-03-27 | **Status**: DONE | **PLAN.md**: entry #1

New authenticated API at `/api/image/` for headless image generation:
- [x] `POST /api/image/generate` — batch_size, batch_count, profile, prompt
- [x] `GET /api/image/status/<id>` — job progress
- [x] `GET /api/image/result/<id>` — retrieve images
- [x] `GET /api/image/profiles` — list profiles
- [x] `X-Orracle-Key` header auth (API key in `.env`). Initially only orradash is authorized.

## Development Phases

### Phase A: Job Scheduler + Compute Load Watcher [DONE]
**Source: feedback item 1.5**

1. [x] **Pre-planned job configs** — `plan_job()` in job_queue.py, persists in queue.yaml, visible on dashboard as "waiting"
2. [x] **Suspend/resume** — `suspend()`/`resume()` with SIGSTOP/SIGCONT via SSH
3. [x] **Compute load watcher** — `ComputeWatcher` class in services.py, polls nvidia-smi + CPU load every 15s, auto-throttles jobs with `throttle=True`
4. [x] **Dashboard quick-start** — Start button on waiting-job cards (`templates/dashboard_new.html:231`), `startWaitingJob()` in `static/js/dashboard.js:153`, `/api/queue/<id>/start` endpoint, `job_queue.start_waiting()` handles WAITING→PENDING transition with SSE broadcast
5. [x] **Throttle toggle API** — `POST /api/queue/<id>/throttle`, SSE broadcasts load events

### Phase B: Content Safety Layer [DONE]
**Source: feedback items 3, 4, 6**

1. [x] **Blurred output** — Blur toggle on both `/studio/image` and `/studio/forge`, shared localStorage key
2. [x] **Guardrail training type** — "Safety Guardrail Fine-Tune" option in training type dropdown
3. [x] **Model quarantine** — `visibility: private` on all model_registry.yaml profiles, export page warns before deploying private models
4. [x] **ComfyUI output isolation** — See Phase E
"#;
        let phases = parse_plan(content);
        assert_eq!(phases.len(), 2);
        assert_eq!(feature_count(&phases), 9);
        assert!(
            !phases
                .iter()
                .flat_map(|phase| phase.features.iter())
                .any(|feature| feature.title.contains("POST /api/image/generate"))
        );
    }

    #[test]
    fn fixture_nextapp_tables_parse_without_architecture_leak() {
        let content = r#"## Architecture

### Key Design Decisions

- **One library crate (`glsr`) + one binary crate (`glsr-cli`)** — clean separation of logic and interface
- **Platform trait abstraction** — all backends implement the same interface; new platforms are a single module file
- **URL-first input model** — user submits URLs, platform auto-detected from domain, fallback to generic yt-dlp

## Feature Roadmap

### Phase 1: Workspace Init + Core Abstractions
> Foundation — project scaffolding and the trait that everything builds on.

| ID | Task | Status | Source |
|----|------|--------|--------|
| P1-01 | Initialize Cargo workspace with `glsr` (lib) + `glsr-cli` (bin) members | `[ ]` | INS-001 |
| P1-02 | Create `.scope` (commercial), `.gitignore`, `CLAUDE.md` | `[ ]` | INS-001 |
| P1-03 | Define `Platform` trait in `glsr/src/platform.rs` — `name()`, `check_status()`, `fetch_stream_url()`, `normalize_input()` | `[ ]` | INS-002 |

## Open Conflicts

### 1. Scope mismatch: `.scope` says `public`, INS-001 says `commercial`

The existing `.scope` file contains `public`. INS-001 explicitly requests `commercial`.
"#;
        let phases = parse_plan(content);
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].number, Some(1));
        assert_eq!(phases[0].features.len(), 3);
        assert!(
            !phases[0]
                .features
                .iter()
                .any(|feature| feature.title.contains("One library crate"))
        );
    }

    #[test]
    fn fixture_orrgent_numbered_architecture_not_scooped() {
        let content = r#"# orrgent — Design Plan

## 1. Vision recap

orrgent is two things at once. First, a personal career-guidance program that scrapes listings, parses them, filters them against the user's profile, and dispatches submissions — with the casting-call submitter for Casting Networks / Actors Access as the canonical first capability.

## 2. Architecture overview

### Module boundaries

```
orrgent/
  daemon/                  # long-running scheduler + dispatch engine (Rust eventually; Python for Phase 0/1)
    scheduler.{py,rs}      # twice-daily run loop, retry policy, jitter
    dispatch.{py,rs}       # submission state machine, idempotency
  adapters/                # one subdir per source — fully self-contained
    casting_networks/      # the blueprint
```
"#;
        let phases = parse_plan(content);
        assert!(phases.is_empty());
    }

    #[test]
    fn test_phase_name_ignores_colon_inside_parenthetical() {
        // Em-dash is the title separator; a colon inside "(target: ...)" must not
        // be mistaken for it. Regression for orrgent phases like
        // "Phase 6 — Orrchestrator absorption (target: ongoing)".
        let p = try_parse_phase_header("### Phase 6 — Orrchestrator absorption (target: ongoing)")
            .unwrap();
        assert_eq!(p.name, "Orrchestrator absorption (target: ongoing)");
        assert_eq!(p.number, Some(6));
        // And a colon-style heading still works:
        let q = try_parse_phase_header("## Phase 4: Multi-Provider (1.5.0)").unwrap();
        assert!(q.name.starts_with("Multi-Provider"));
    }
}
