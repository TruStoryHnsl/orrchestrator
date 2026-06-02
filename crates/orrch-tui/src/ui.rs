use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Row,
    Table, TableState, Wrap,
};
use ratatui::Frame;

use orrch_core::{LifecycleStage, Project, SessionState, FeedbackStatus};
use crate::app::{App, IntakeReviewFocus, Panel, SubView};
use crate::markdown::markdown_to_lines;

// ─── Color Palette (all high-contrast, readable on translucent bg) ────
const ACCENT: Color = Color::Rgb(233, 69, 96);
const TEXT: Color = Color::Rgb(230, 230, 240);      // primary text — always readable
const TEXT_DIM: Color = Color::Rgb(180, 180, 200);   // secondary text — still readable
const TEXT_MUTED: Color = Color::Rgb(130, 130, 155);  // tertiary — used sparingly
const BG_DARK: Color = Color::Rgb(22, 33, 62);
const BG_HIGHLIGHT: Color = Color::Rgb(35, 35, 70);
const WAITING_COLOR: Color = Color::Rgb(255, 200, 50);
const GREEN: Color = Color::Rgb(80, 200, 120);
const CYAN: Color = Color::Rgb(100, 200, 220);

/// Standard scroll padding for all lists — keeps 3 items visible below cursor.
const SCROLL_PAD: usize = 3;

/// Map FeatureStatus to a display style with distinct colors.
fn feature_status_style(status: orrch_core::FeatureStatus) -> Style {
    use orrch_core::FeatureStatus;
    match status {
        FeatureStatus::Planned | FeatureStatus::Pending => Style::default().fg(TEXT_DIM),
        FeatureStatus::Implementing | FeatureStatus::InProgress => Style::default().fg(WAITING_COLOR),
        FeatureStatus::Implemented => Style::default().fg(CYAN),
        FeatureStatus::Testing => Style::default().fg(Color::Rgb(180, 120, 220)),
        FeatureStatus::Verified => Style::default().fg(GREEN),
        FeatureStatus::UserConfirmed => Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        FeatureStatus::Done => Style::default().fg(GREEN),
        FeatureStatus::Deprecated => Style::default().fg(Color::Rgb(90, 90, 110)).add_modifier(Modifier::CROSSED_OUT),
        FeatureStatus::Removed(_) => Style::default().fg(Color::Rgb(200, 60, 60)).add_modifier(Modifier::CROSSED_OUT),
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // Staleness banner: when source files have been edited since the
    // running binary was built, render a 1-line protest banner above the
    // panel tabs. Geometry stays stable in non-stale state (zero rows).
    let stale_state = orrch_core::staleness::snapshot();
    let banner_height: u16 = if stale_state.is_stale() { 1 } else { 0 };

    // Layout: [banner] + panel tabs (1) + content + status bar (1)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(banner_height),
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);

    if banner_height > 0 {
        draw_staleness_banner(frame, layout[0], &stale_state);
    }
    let layout = [layout[1], layout[2], layout[3]];

    draw_panel_tabs(frame, app, layout[0]);

    // Copy sub to avoid borrow conflict
    let sub = app.sub.clone();
    match sub {
        SubView::List => draw_panel_content(frame, app, layout[1]),
        SubView::ProjectDetail(idx) => draw_project_detail(frame, app, layout[1], idx),
        SubView::SessionFocus(idx) => draw_session_focus(frame, app, layout[1], idx),
        SubView::ExternalSessionView(pid) => draw_external_session(frame, app, layout[1], pid),
        SubView::SpawnGoal => { draw_panel_content(frame, app, layout[1]); draw_spawn_goal(frame, app); }
        SubView::SpawnWorkforce => { draw_panel_content(frame, app, layout[1]); draw_spawn_workforce(frame, app); }
        SubView::SpawnAgent => { draw_panel_content(frame, app, layout[1]); draw_spawn_agent(frame, app); }
        SubView::SpawnBackend => { draw_panel_content(frame, app, layout[1]); draw_spawn_backend(frame, app); }
        SubView::SpawnEngine => { draw_panel_content(frame, app, layout[1]); draw_spawn_engine(frame, app); }
        SubView::SpawnHost => { draw_panel_content(frame, app, layout[1]); draw_spawn_host(frame, app); }
        SubView::RoutingSummary => { draw_panel_content(frame, app, layout[1]); draw_routing_summary(frame, app); }
        SubView::ConfirmDeprecate(idx) => { draw_panel_content(frame, app, layout[1]); draw_confirm_deprecate(frame, app, idx); }
        SubView::ConfirmComplete(idx) => { draw_panel_content(frame, app, layout[1]); draw_confirm_complete(frame, app, idx); }
        SubView::ConfirmDeleteFeedback(idx) => { draw_panel_content(frame, app, layout[1]); draw_confirm_delete_feedback(frame, app, idx); }
        SubView::DeprecatedBrowser => draw_deprecated_browser(frame, app, layout[1]),
        SubView::AppMenu => { draw_panel_content(frame, app, layout[1]); draw_app_menu(frame, app); }
        SubView::ActionMenu => { draw_panel_content(frame, app, layout[1]); draw_action_menu(frame, app); }
        SubView::ConfirmDeleteDeprecated => { draw_deprecated_browser(frame, app, layout[1]); draw_confirm_delete_deprecated(frame, app); }
        SubView::NewProjectName => { draw_panel_content(frame, app, layout[1]); draw_new_project_name(frame, app); }
        SubView::NewProjectScope => { draw_panel_content(frame, app, layout[1]); draw_new_project_scope(frame, app); }
        SubView::NewProjectConfirm => { draw_panel_content(frame, app, layout[1]); draw_new_project_confirm(frame, app); }
        SubView::FeedbackConfirm(_) => { draw_panel_content(frame, app, layout[1]); draw_feedback_confirm(frame, app); }
        SubView::CommitReview(_) => { draw_panel_content(frame, app, layout[1]); draw_commit_review(frame, app); }
        SubView::CommitCorrecting(_) => { draw_panel_content(frame, app, layout[1]); draw_commit_correcting(frame, app); }
        SubView::WorkflowPicker => { draw_panel_content(frame, app, layout[1]); draw_workflow_picker(frame, app); }
        SubView::AddFeature(idx) => { draw_project_detail(frame, app, layout[1], idx); draw_add_feature(frame, app); }
        SubView::AddMcpServer => { draw_panel_content(frame, app, layout[1]); draw_add_mcp_server(frame, app); }
        SubView::RenameWorkforce(_) => { draw_panel_content(frame, app, layout[1]); draw_rename_popup(frame, app, "Rename Workforce File"); }
        SubView::RenameIdea(_) => { draw_panel_content(frame, app, layout[1]); draw_rename_popup(frame, app, "Rename Idea"); }
        SubView::ConfirmRollback => { draw_panel_content(frame, app, layout[1]); draw_confirm_rollback(frame, app); }
        SubView::ConfirmKillSession(ref name) => {
            let name = name.clone();
            draw_panel_content(frame, app, layout[1]);
            draw_confirm_kill_session(frame, &name);
        }
        SubView::RenameProject(_) => { draw_panel_content(frame, app, layout[1]); draw_rename_popup(frame, app, "Rename Project"); }
        SubView::RenamePlanFeature { .. } => { draw_panel_content(frame, app, layout[1]); draw_rename_popup(frame, app, "Rename Plan Feature"); }
        SubView::RenameFile { .. } => { draw_panel_content(frame, app, layout[1]); draw_rename_popup(frame, app, "Rename File"); }
        SubView::SteerSession(idx) => {
            draw_panel_content(frame, app, layout[1]);
            draw_steer_session_input(frame, app, idx);
        }
        SubView::SetLogoPath(idx) => {
            draw_project_detail(frame, app, layout[1], idx);
            draw_set_logo_path(frame, app);
        }
        SubView::ExpandedSession(_) => {
            // Routed through draw_panel_content's early-return so the
            // expanded pane fills the panel-content area cleanly.
            draw_panel_content(frame, app, layout[1]);
        }
        SubView::ScopeVisibility => {
            draw_panel_content(frame, app, layout[1]);
            draw_scope_visibility(frame, app);
        }
    }

    draw_status_bar(frame, app, layout[2]);
}

/// Render the staleness protest banner. Red background, bold white text.
/// Caller is responsible for only invoking when `state.is_stale()` is true
/// and the layout has actually allocated a row.
fn draw_staleness_banner(
    frame: &mut Frame,
    area: Rect,
    state: &orrch_core::staleness::StalenessState,
) {
    let text = orrch_core::staleness::banner_text(state);
    let banner = Paragraph::new(Line::from(Span::styled(
        text,
        Style::default()
            .fg(Color::Rgb(255, 255, 255))
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD),
    )))
    .style(Style::default().bg(ACCENT));
    frame.render_widget(banner, area);
}

fn draw_panel_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus_depth == 0;
    let panel_count = Panel::ALL.len();
    let width = area.width as usize;
    // Each slot (label + divider) is exactly width/N chars. Dividers always render.
    let slot_width = if panel_count > 0 { width / panel_count } else { width };
    let remainder = if panel_count > 0 { width % panel_count } else { 0 };

    let spans: Vec<Span> = Panel::ALL.iter().enumerate().flat_map(|(i, p)| {
        let is_last = i == panel_count - 1;
        // Last slot absorbs remainder pixels and has no divider
        let label_width = if is_last {
            slot_width + remainder
        } else {
            slot_width.saturating_sub(1) // 1 char reserved for "│"
        };

        // Pick label tier that fits (need at least 1 char padding each side)
        let label = if label_width >= p.label().len() + 2 {
            p.label()
        } else if label_width >= p.short_label().len() + 2 {
            p.short_label()
        } else {
            p.tiny_label()
        };

        // Center label, truncate if still too wide
        let pad_total = label_width.saturating_sub(label.len());
        let pad_left = pad_total / 2;
        let pad_right = pad_total - pad_left;
        let padded = format!("{}{}{}", " ".repeat(pad_left), label, " ".repeat(pad_right));
        let truncated: String = padded.chars().take(label_width).collect();

        let style = if *p == app.panel {
            if focused {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            }
        } else {
            Style::default().fg(TEXT_MUTED)
        };

        let mut result = vec![Span::styled(truncated, style)];
        if !is_last {
            result.push(Span::styled("│", Style::default().fg(TEXT_MUTED)));
        }
        result
    }).collect();

    let bg = if focused { Color::Rgb(30, 30, 55) } else { BG_DARK };
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}

fn draw_panel_content(frame: &mut Frame, app: &mut App, area: Rect) {
    // Hypervise focused single-session view takes over the panel area.
    if matches!(app.sub, crate::app::SubView::ExpandedSession(_)) {
        let name = if let crate::app::SubView::ExpandedSession(ref n) = app.sub {
            n.clone()
        } else {
            String::new()
        };
        draw_expanded_session(frame, app, area, &name);
        return;
    }
    match app.panel {
        Panel::Design => draw_design(frame, app, area),
        Panel::Oversee => draw_projects(frame, app, area),
        Panel::Hypervise => draw_hypervise(frame, app, area),
        Panel::Analyze => draw_analyze(frame, app, area),
        Panel::Publish => draw_publish(frame, app, area),
    }
}

/// Hypervise panel with sub-tabs (overhaul point 8).
/// Sub-bar above the body: Sessions | Loops | Token Usage. Tab cycles.
fn draw_hypervise(frame: &mut Frame, app: &mut App, area: Rect) {
    use crate::app::HyperviseSub;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    // Sub-tab bar
    let mut spans: Vec<Span> = Vec::new();
    for (i, tab) in HyperviseSub::ALL.iter().enumerate() {
        if i > 0 { spans.push(Span::raw("  ")); }
        let sel = *tab == app.hypervise_sub;
        let style = if sel {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        spans.push(Span::styled(tab.label(), style));
    }
    spans.push(Span::raw("    "));
    spans.push(Span::styled("[Tab]=cycle", Style::default().fg(TEXT_MUTED)));
    frame.render_widget(Paragraph::new(Line::from(spans)), chunks[0]);

    match app.hypervise_sub {
        HyperviseSub::Sessions => draw_sessions_tab(frame, app, chunks[1]),
        HyperviseSub::Loops => draw_hypervise_loops_tab(frame, app, chunks[1]),
        HyperviseSub::TokenUsage => draw_hypervise_token_usage_tab(frame, app, chunks[1]),
    }
}

/// Hypervise > Loops tab body (overhaul point 8).
/// Lists registered loop schedules. Empty-state guidance when none exist.
fn draw_hypervise_loops_tab(frame: &mut Frame, app: &App, area: Rect) {
    if app.loop_schedules.is_empty() {
        let msg = "No loop schedules registered.\n\n\
                   A loop is a sequence of workflows that orrchestrator runs\n\
                   automatically — when one workflow's cleanup team writes\n\
                   `cleanup_summary.md`, the loop closes that workflow's sessions\n\
                   and starts the next workflow in the list, repeating when the\n\
                   final workflow finishes.\n\n\
                   To add a loop, go to Oversee, select a project, open the\n\
                   action menu and choose 'Start Loop'.\n\n\
                   Keys: [t]=toggle enabled  [Del]=remove  [r]=reload";
        let p = Paragraph::new(msg)
            .style(Style::default().fg(TEXT_DIM))
            .wrap(Wrap { trim: false })
            .block(Block::default()
                .title(" Loops (empty) ")
                .borders(Borders::ALL)
                .style(Style::default().fg(TEXT_MUTED)));
        frame.render_widget(p, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("On").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Name").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Project").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Workflows").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
    ]).height(1).bottom_margin(1);

    let rows: Vec<Row> = app
        .loop_schedules
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let on = if s.enabled { "●" } else { "○" };
            let on_color = if s.enabled { GREEN } else { TEXT_DIM };
            let name_color = if i == app.loop_selected { ACCENT } else { TEXT };
            let proj = s.project_dir.file_name().map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "?".into());
            let wfs = s.workflows.join(" → ");
            Row::new(vec![
                Cell::from(on).style(Style::default().fg(on_color).add_modifier(Modifier::BOLD)),
                Cell::from(s.name.clone()).style(Style::default().fg(name_color)),
                Cell::from(proj).style(Style::default().fg(TEXT_DIM)),
                Cell::from(wfs).style(Style::default().fg(TEXT)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(3),
        Constraint::Min(20),
        Constraint::Length(20),
        Constraint::Min(30),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default()
            .title(format!(" Loops ({}) — [t]=toggle  [Del]=remove  [r]=reload ", app.loop_schedules.len()))
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)))
        .column_spacing(2);
    frame.render_widget(table, area);
}

/// Hypervise > Token Usage tab body (overhaul point 10).
/// Renders a per-session table of token usage. Initial cut shows session
/// count + duration aggregated from the existing UsageTracker; per-session
/// token columns show "n/a" pending the token-tracking subsystem (TOK-001).
fn draw_hypervise_token_usage_tab(frame: &mut Frame, app: &App, area: Rect) {
    use orrch_core::usage;

    let summary = app.usage_tracker.summary();

    let header = Row::new(vec![
        Cell::from("Provider").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Sessions").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Duration").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Last Used").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Tokens").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
    ]).height(1).bottom_margin(1);

    let rows: Vec<Row> = if summary.per_provider.is_empty() {
        vec![Row::new(vec![
            Cell::from("(no usage tracked yet)").style(Style::default().fg(TEXT_DIM)),
            Cell::from(""), Cell::from(""), Cell::from(""), Cell::from(""),
        ])]
    } else {
        summary.per_provider.iter().map(|p| {
            let last = p.last_used.as_deref().map(usage::format_ago).unwrap_or_else(|| "—".into());
            Row::new(vec![
                Cell::from(p.provider.clone()).style(Style::default().fg(CYAN)),
                Cell::from(format!("{}", p.session_count)).style(Style::default().fg(TEXT)),
                Cell::from(usage::format_duration(p.total_duration_secs)).style(Style::default().fg(TEXT)),
                Cell::from(last).style(Style::default().fg(TEXT_DIM)),
                Cell::from("n/a (TOK-001)").style(Style::default().fg(TEXT_MUTED)),
            ])
        }).collect()
    };

    let widths = [
        Constraint::Length(14),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(18),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default()
            .title(format!(" Token Usage (last {}h) ", summary.period_hours))
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)))
        .column_spacing(2);
    frame.render_widget(table, area);
}

// ─── Deprecated Panel ─────────────────────────────────────────────────

fn draw_deprecated_browser(frame: &mut Frame, app: &App, area: Rect) {
    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(20), Constraint::Percentage(60)])
        .split(area);

    // Parent column
    let parent_focused = !app.dep_in_child;
    let parent_border = if parent_focused { Style::default().fg(ACCENT) } else { Style::default().fg(TEXT_MUTED) };
    let rel_path = app.dep_path.strip_prefix(&app.dep_root).unwrap_or(&app.dep_path);
    let parent_title = if rel_path.as_os_str().is_empty() { " deprecated/ ".to_string() } else { format!(" {}/ ", rel_path.display()) };

    let parent_items: Vec<ListItem> = app.dep_parent_entries.iter().map(|e| {
        let style = if e.is_dir { Style::default().fg(CYAN) } else { Style::default().fg(TEXT) };
        ListItem::new(format!("{} {}", e.icon(), e.name)).style(style)
    }).collect();
    let parent_list = List::new(parent_items)
        .scroll_padding(SCROLL_PAD)
        .block(Block::default().title(parent_title).borders(Borders::ALL).style(parent_border))
        .highlight_style(Style::default().bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    let mut pstate = ListState::default().with_selected(Some(app.dep_parent_selected));
    frame.render_stateful_widget(parent_list, hsplit[0], &mut pstate);

    // Child column
    let child_focused = app.dep_in_child;
    let child_border = if child_focused { Style::default().fg(ACCENT) } else { Style::default().fg(TEXT_MUTED) };
    let child_title = app.dep_parent_entries.get(app.dep_parent_selected)
        .filter(|e| e.is_dir).map(|e| format!(" {}/ ", e.name)).unwrap_or_else(|| " — ".into());

    let child_items: Vec<ListItem> = app.dep_child_entries.iter().map(|e| {
        let style = if e.is_dir { Style::default().fg(CYAN) } else { Style::default().fg(TEXT) };
        ListItem::new(format!("{} {}", e.icon(), e.name)).style(style)
    }).collect();

    if child_items.is_empty() {
        let empty = Paragraph::new("  (empty or file)").style(Style::default().fg(TEXT_MUTED))
            .block(Block::default().title(child_title).borders(Borders::ALL).style(child_border));
        frame.render_widget(empty, hsplit[1]);
    } else {
        let child_list = List::new(child_items)
            .scroll_padding(SCROLL_PAD)
            .block(Block::default().title(child_title).borders(Borders::ALL).style(child_border))
            .highlight_style(Style::default().bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD))
            .highlight_symbol("▶ ");
        let sel = if child_focused { Some(app.dep_child_selected) } else { None };
        let mut cstate = ListState::default().with_selected(sel);
        frame.render_stateful_widget(child_list, hsplit[1], &mut cstate);
    }

    // Preview
    let preview = Paragraph::new(app.dep_preview.as_str())
        .style(Style::default().fg(TEXT))
        .block(Block::default().title(" Details (read-only) ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(preview, hsplit[2]);
}

// ─── Design Panel ────────────────────────────────────────────────────

#[allow(dead_code)]
fn draw_placeholder(frame: &mut Frame, area: Rect, title: &str, message: &str) {
    let msg = Paragraph::new(message)
        .style(Style::default().fg(TEXT_DIM))
        .block(Block::default().title(format!(" {} ", title)).borders(Borders::ALL));
    frame.render_widget(msg, area);
}

/// Analyze panel — assesses projects for market-ready status (overhaul
/// point 12). Sub-bar above the body cycles through CodeReview / Licensing /
/// Legal / Monetization / Patents.
fn draw_analyze(frame: &mut Frame, app: &App, area: Rect) {
    use crate::app::AnalyzeTab;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    // Sub-tab bar
    let mut spans: Vec<Span> = Vec::new();
    for (i, tab) in AnalyzeTab::ALL.iter().enumerate() {
        if i > 0 { spans.push(Span::raw("  ")); }
        let sel = *tab == app.analyze_tab;
        let style = if sel {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        spans.push(Span::styled(tab.label(), style));
    }
    spans.push(Span::raw("    "));
    spans.push(Span::styled("[Tab]=cycle", Style::default().fg(TEXT_MUTED)));
    frame.render_widget(Paragraph::new(Line::from(spans)), chunks[0]);

    match app.analyze_tab {
        AnalyzeTab::CodeReview => draw_analyze_code_review(frame, app, chunks[1]),
        AnalyzeTab::Licensing => draw_analyze_licensing(frame, app, chunks[1]),
        AnalyzeTab::Legal => draw_analyze_legal(frame, app, chunks[1]),
        AnalyzeTab::Monetization => draw_analyze_placeholder(frame, "Monetization", chunks[1]),
        AnalyzeTab::Patents => draw_analyze_placeholder(frame, "Patents", chunks[1]),
    }
}

/// Analyze > Code Review — original draw_analyze body (provider/project usage).
fn draw_analyze_code_review(frame: &mut Frame, app: &App, area: Rect) {
    use orrch_core::usage;

    let summary = app.usage_tracker.summary();

    // Split vertically: provider summary, per-project breakdown, budget footer.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(53), Constraint::Length(1)])
        .split(area);

    // ── Provider summary ────────────────────────────────────────────────────
    if summary.per_provider.is_empty() {
        let msg = Paragraph::new("No usage data yet. Session metrics will appear here as you spawn sessions.")
            .style(Style::default().fg(TEXT_DIM))
            .block(Block::default()
                .title(format!(" Usage Summary (last {}h) ", summary.period_hours))
                .borders(Borders::ALL)
                .style(Style::default().fg(TEXT_MUTED)));
        frame.render_widget(msg, chunks[0]);
    } else {
        let header = Row::new(vec![
            Cell::from("Provider").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Cell::from("Sessions").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Cell::from("Duration").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Cell::from("Last Used").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        ]).height(1).bottom_margin(1);

        let mut rows: Vec<Row> = Vec::new();
        let mut total_duration: f64 = 0.0;

        for p in &summary.per_provider {
            total_duration += p.total_duration_secs;
            let last = p.last_used.as_deref().map(usage::format_ago).unwrap_or_else(|| "—".into());
            rows.push(Row::new(vec![
                Cell::from(p.provider.clone()).style(Style::default().fg(CYAN)),
                Cell::from(format!("{}", p.session_count)).style(Style::default().fg(TEXT)),
                Cell::from(usage::format_duration(p.total_duration_secs)).style(Style::default().fg(TEXT)),
                Cell::from(last).style(Style::default().fg(TEXT_DIM)),
            ]));
        }

        rows.push(Row::new(vec![
            Cell::from("Total").style(Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Cell::from(format!("{}", summary.total_sessions)).style(Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Cell::from(usage::format_duration(total_duration)).style(Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Cell::from(""),
        ]).top_margin(1));

        let widths = [
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(12),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(Block::default()
                .title(format!(" Usage Summary (last {}h) ", summary.period_hours))
                .borders(Borders::ALL)
                .style(Style::default().fg(TEXT_MUTED)))
            .column_spacing(2);

        frame.render_widget(table, chunks[0]);
    }

    // ── Per-project breakdown ────────────────────────────────────────────────
    let proj_header = Row::new(vec![
        Cell::from("Project").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Sessions").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Max").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Tokens").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Cell::from("Cost").style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
    ]).height(1).bottom_margin(1);

    let proj_rows: Vec<Row> = if app.projects.is_empty() {
        vec![Row::new(vec![
            Cell::from("(no projects loaded)").style(Style::default().fg(TEXT_DIM)),
            Cell::from(""), Cell::from(""), Cell::from(""), Cell::from(""),
        ])]
    } else {
        app.projects.iter().map(|proj| {
            let sess = app.active_session_count(&proj.path);
            let max = proj.max_sessions;
            // Tokens and cost are not tracked per-project yet — show placeholder.
            Row::new(vec![
                Cell::from(proj.name.clone()).style(Style::default().fg(if sess > 0 { CYAN } else { TEXT })),
                Cell::from(format!("{sess}")).style(Style::default().fg(if sess > 0 { GREEN } else { TEXT_DIM })),
                Cell::from(format!("{max}")).style(Style::default().fg(TEXT_DIM)),
                Cell::from("—").style(Style::default().fg(TEXT_DIM)),
                Cell::from("—").style(Style::default().fg(TEXT_DIM)),
            ])
        }).collect()
    };

    let proj_widths = [
        Constraint::Min(18),
        Constraint::Length(9),
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Length(10),
    ];

    let proj_table = Table::new(proj_rows, proj_widths)
        .header(proj_header)
        .block(Block::default()
            .title(" Per-Project Breakdown ")
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)))
        .column_spacing(2);

    frame.render_widget(proj_table, chunks[1]);

    // ── Token budget status bar ──────────────────────────────────────────────
    let total_secs: f64 = summary.per_provider.iter().map(|p| p.total_duration_secs).sum();
    let total_mins = (total_secs / 60.0).round() as u64;
    let hours = total_mins / 60;
    let mins = total_mins % 60;
    let duration_str = if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    };
    let budget_line = format!(
        " Session budget: {} total · {} sessions (last {}h)",
        duration_str, summary.total_sessions, summary.period_hours
    );
    let budget_bar = Paragraph::new(budget_line)
        .style(Style::default().fg(TEXT_MUTED).bg(BG_DARK));
    frame.render_widget(budget_bar, chunks[2]);
}

/// Analyze > Licensing — re-renders the license_report data source from
/// orrch-core::compliance. Same source as Publish > Compliance; both panels
/// can render it during the migration window.
fn draw_analyze_licensing(frame: &mut Frame, app: &App, area: Rect) {
    let rows: Vec<Row> = match &app.license_report {
        None => vec![Row::new(vec![
            Cell::from("—").style(Style::default().fg(TEXT_DIM)),
            Cell::from(""),
            Cell::from("Switch to Publish > Compliance to populate this data, then return.")
                .style(Style::default().fg(TEXT_DIM)),
        ])],
        Some(report) if report.deps.is_empty() => vec![Row::new(vec![
            Cell::from("—").style(Style::default().fg(TEXT_DIM)),
            Cell::from(""),
            Cell::from("No Cargo.lock found").style(Style::default().fg(TEXT_DIM)),
        ])],
        Some(report) => report
            .deps
            .iter()
            .map(|dep| {
                let (status_color, status_label) = match dep.status {
                    orrch_core::LicenseStatus::Permissive => (GREEN, dep.status.label()),
                    orrch_core::LicenseStatus::Copyleft => (WAITING_COLOR, dep.status.label()),
                    orrch_core::LicenseStatus::Unknown => (TEXT_DIM, dep.status.label()),
                };
                Row::new(vec![
                    Cell::from(dep.name.clone()).style(Style::default().fg(TEXT)),
                    Cell::from(dep.spdx.clone()).style(Style::default().fg(TEXT_DIM)),
                    Cell::from(status_label)
                        .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
                ])
            })
            .collect(),
    };

    let title = match &app.license_report {
        Some(r) => format!(
            " Licensing ({} deps, {} permissive, {} copyleft, {} unknown) ",
            r.total, r.permissive, r.copyleft, r.unknown
        ),
        None => " Licensing ".to_string(),
    };
    let table = Table::new(rows, [
        Constraint::Percentage(35),
        Constraint::Percentage(45),
        Constraint::Percentage(20),
    ])
    .block(Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(TEXT_MUTED)))
    .column_spacing(1);
    frame.render_widget(table, area);
}

/// Analyze > Legal — re-renders the copyright_report data source from
/// orrch-core::compliance. Mirrors Publish > Compliance.
fn draw_analyze_legal(frame: &mut Frame, app: &App, area: Rect) {
    let body = match &app.copyright_report {
        None => "No copyright scan available.\n\
                 Switch to Publish > Compliance to populate, then return."
            .to_string(),
        Some(report) => {
            let mut s = format!(
                "Files scanned: {}\nWith copyright header: {}\nMissing header: {}\n\n",
                report.scanned,
                report.with_header,
                report.missing.len()
            );
            if !report.missing.is_empty() {
                s.push_str("Files missing copyright header:\n");
                for entry in report.missing.iter().take(50) {
                    s.push_str("- ");
                    s.push_str(&entry.path);
                    s.push('\n');
                }
                if report.missing.len() > 50 {
                    s.push_str(&format!(
                        "... ({} more)\n",
                        report.missing.len() - 50
                    ));
                }
            }
            s
        }
    };
    let p = Paragraph::new(body)
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: false })
        .block(Block::default()
            .title(" Legal — Copyright Header Audit ")
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)));
    frame.render_widget(p, area);
}

/// Analyze placeholder for Monetization / Patents — these are stubs in this
/// sprint; the deeper analyses (revenue model assessments, patent landscape
/// scans) ship in a follow-up.
fn draw_analyze_placeholder(frame: &mut Frame, label: &str, area: Rect) {
    let msg = format!(
        "{label}\n\nThis sub-tab is a stub.\n\nFollow-up will land in a later \
         sprint as part of the Analyze panel buildout (overhaul point 12)."
    );
    let p = Paragraph::new(msg)
        .style(Style::default().fg(TEXT_DIM))
        .wrap(Wrap { trim: false })
        .block(Block::default()
            .title(format!(" {label} (coming soon) "))
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)));
    frame.render_widget(p, area);
}

/// Publish panel: tab bar + per-tab placeholder content (item 98).
fn draw_publish(frame: &mut Frame, app: &mut App, area: Rect) {
    use crate::app::PublishTab;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    // Tab bar
    let mut spans: Vec<Span> = Vec::new();
    for (i, tab) in PublishTab::ALL.iter().enumerate() {
        if i > 0 { spans.push(Span::raw("  ")); }
        let sel = *tab == app.publish_tab;
        let style = if sel {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        spans.push(Span::styled(tab.label(), style));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), chunks[0]);

    // Populate tab data on first render.
    if app.publish_tab == PublishTab::Packaging && app.release_notes_preview.is_none() {
        app.refresh_packaging_data();
    }
    if app.publish_tab == PublishTab::Compliance && app.license_report.is_none() {
        app.refresh_compliance_data();
    }
    if app.publish_tab == PublishTab::Distribution && app.distribution_status.is_none() {
        let dir = app.projects_dir.join("orrchestrator");
        app.distribution_status = Some(orrch_core::release::detect_distribution_status(&dir));
    }
    if app.publish_tab == PublishTab::History && app.release_history.is_none() {
        let dir = app.projects_dir.join("orrchestrator");
        app.release_history = Some(orrch_core::release::load_release_history(&dir));
    }
    if app.publish_tab == PublishTab::Marketing && app.marketing_metadata.is_none() {
        let dir = app.projects_dir.join("orrchestrator");
        app.marketing_metadata = Some(orrch_core::release::load_marketing_metadata(&dir));
    }

    match app.publish_tab {
        PublishTab::Packaging => draw_packaging_tab(frame, app, chunks[1]),
        PublishTab::Distribution => draw_distribution_tab(frame, app, chunks[1]),
        PublishTab::Brands => draw_brands_tab(frame, app, chunks[1]),
        PublishTab::Compliance => draw_compliance_tab(frame, app, chunks[1]),
        PublishTab::Marketing => draw_marketing_tab(frame, app, chunks[1]),
        PublishTab::History => draw_history_tab(frame, app, chunks[1]),
    }
}

/// Publish > Brands — lists brand profile .md files in `brands/` with a
/// markdown preview pane on the right (overhaul point 11).
fn draw_brands_tab(frame: &mut Frame, app: &App, area: Rect) {
    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    let rows: Vec<Row> = if app.brand_profiles.is_empty() {
        vec![Row::new(vec![
            Cell::from("(no brand profiles)").style(Style::default().fg(TEXT_DIM)),
        ])]
    } else {
        app.brand_profiles
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                let style = if i == app.brand_selected {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                Row::new(vec![Cell::from(name.clone()).style(style)])
            })
            .collect()
    };
    let list_table = Table::new(rows, [Constraint::Min(15)])
        .block(Block::default()
            .title(format!(" Brand Profiles ({}) ", app.brand_profiles.len()))
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)));
    frame.render_widget(list_table, hsplit[0]);

    let preview = if let Some((_, path)) = app.brand_profiles.get(app.brand_selected) {
        std::fs::read_to_string(path).unwrap_or_else(|e| format!("Error reading: {e}"))
    } else {
        "No brand profile selected. Add .md files under `brands/` and press 'r' to reload."
            .to_string()
    };
    let preview_widget = Paragraph::new(crate::markdown::markdown_to_lines(&preview))
        .wrap(Wrap { trim: false })
        .block(Block::default()
            .title(" Brand Style Guide ")
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)));
    frame.render_widget(preview_widget, hsplit[1]);
}

fn draw_packaging_tab(frame: &mut Frame, app: &App, area: Rect) {
    // Split horizontally: left=release notes, right=checklist+build targets
    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    // ── Release Notes (left) ────────────────────────────────────────────
    // 108: show rollback advisory if one is pending
    let notes_text = if let Some(ref advisory) = app.rollback_advisory {
        advisory.as_str()
    } else {
        app.release_notes_preview.as_deref().unwrap_or(
            "Release notes not yet generated.\nNavigate to this tab to load.\n\n[v] preview next version changelog  [b] build artifacts  [D] rollback selected tag",
        )
    };
    let notes_title = if app.rollback_advisory.is_some() {
        " Rollback Advisory  [r]=refresh to clear "
    } else {
        " Release Notes  [v]=preview version  [b]=build  [D]=rollback "
    };
    let notes = Paragraph::new(notes_text)
        .style(Style::default().fg(if app.rollback_advisory.is_some() { Color::Red } else { TEXT }))
        .wrap(Wrap { trim: false })
        .block(Block::default()
            .title(notes_title)
            .borders(Borders::ALL)
            .style(Style::default().fg(if app.rollback_advisory.is_some() { Color::Red } else { TEXT_MUTED })));
    frame.render_widget(notes, hsplit[0]);

    // ── Right pane: checklist (top) + build targets (bottom) ───────────
    let right_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(hsplit[1]);

    // Pre-release Checklist
    let checklist_rows: Vec<Row> = if app.checklist_results.is_empty() {
        vec![Row::new(vec![
            Cell::from("—").style(Style::default().fg(TEXT_DIM)),
            Cell::from("Navigate here to run checks").style(Style::default().fg(TEXT_DIM)),
        ])]
    } else {
        app.checklist_results.iter().map(|(label, passed)| {
            let (icon, color) = if *passed { ("✓", GREEN) } else { ("✗", Color::Red) };
            Row::new(vec![
                Cell::from(icon).style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
                Cell::from(label.clone()).style(Style::default().fg(if *passed { TEXT } else { Color::Red })),
            ])
        }).collect()
    };

    let all_pass = !app.checklist_results.is_empty()
        && app.checklist_results.iter().all(|(_, p)| *p);
    let checklist_title = if all_pass { " Pre-release ✓ " } else { " Pre-release Checklist " };

    let checklist = Table::new(checklist_rows, [Constraint::Length(3), Constraint::Min(30)])
        .block(Block::default()
            .title(checklist_title)
            .borders(Borders::ALL)
            .style(Style::default().fg(if all_pass { GREEN } else { TEXT_MUTED })))
        .column_spacing(1);
    frame.render_widget(checklist, right_split[0]);

    // Build Targets
    let build_rows: Vec<Row> = if app.build_targets.is_empty() {
        vec![Row::new(vec![
            Cell::from("—").style(Style::default().fg(TEXT_DIM)),
            Cell::from("No project files detected").style(Style::default().fg(TEXT_DIM)),
        ])]
    } else {
        app.build_targets.iter().enumerate().map(|(i, target)| {
            let result = app.build_results.get(i);
            let (icon, icon_color) = match result {
                Some(r) => match r.status {
                    orrch_core::release::BuildStatus::Success => ("✓", GREEN),
                    orrch_core::release::BuildStatus::Failed => ("✗", Color::Red),
                    orrch_core::release::BuildStatus::Running => ("⏳", WAITING_COLOR),
                    orrch_core::release::BuildStatus::Pending => ("·", TEXT_DIM),
                },
                None => ("·", TEXT_DIM),
            };
            Row::new(vec![
                Cell::from(icon).style(Style::default().fg(icon_color).add_modifier(Modifier::BOLD)),
                Cell::from(target.label.clone()).style(Style::default().fg(TEXT)),
            ])
        }).collect()
    };

    let build_title = if app.build_running { " Build Targets ⏳ " } else { " Build Targets  [b]=run " };
    let build_table = Table::new(build_rows, [Constraint::Length(3), Constraint::Min(30)])
        .block(Block::default()
            .title(build_title)
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)))
        .column_spacing(1);
    frame.render_widget(build_table, right_split[1]);
}

fn draw_compliance_tab(frame: &mut Frame, app: &App, area: Rect) {
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    // ── License Report (top) ───────────────────────────────────────────
    let lic_rows: Vec<Row> = match &app.license_report {
        None => vec![Row::new(vec![
            Cell::from("—").style(Style::default().fg(TEXT_DIM)),
            Cell::from("").style(Style::default().fg(TEXT_DIM)),
            Cell::from("Loading...").style(Style::default().fg(TEXT_DIM)),
        ])],
        Some(report) => {
            if report.deps.is_empty() {
                vec![Row::new(vec![
                    Cell::from("—").style(Style::default().fg(TEXT_DIM)),
                    Cell::from("").style(Style::default().fg(TEXT_DIM)),
                    Cell::from("No Cargo.lock found").style(Style::default().fg(TEXT_DIM)),
                ])]
            } else {
                report.deps.iter().map(|dep| {
                    let (status_color, status_label) = match dep.status {
                        orrch_core::LicenseStatus::Permissive => (GREEN, dep.status.label()),
                        orrch_core::LicenseStatus::Copyleft => (WAITING_COLOR, dep.status.label()),
                        orrch_core::LicenseStatus::Unknown => (TEXT_DIM, dep.status.label()),
                    };
                    Row::new(vec![
                        Cell::from(dep.name.clone()).style(Style::default().fg(TEXT)),
                        Cell::from(dep.spdx.clone()).style(Style::default().fg(TEXT_DIM)),
                        Cell::from(status_label).style(Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
                    ])
                }).collect()
            }
        }
    };

    let lic_title = match &app.license_report {
        Some(r) => format!(" Licenses ({} deps, {} permissive, {} copyleft, {} unknown) ", r.total, r.permissive, r.copyleft, r.unknown),
        None => " Licenses ".to_string(),
    };
    let lic_table = Table::new(lic_rows, [Constraint::Percentage(35), Constraint::Percentage(45), Constraint::Percentage(20)])
        .block(Block::default()
            .title(lic_title)
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)))
        .column_spacing(1);
    frame.render_widget(lic_table, vsplit[0]);

    // ── Copyright Report (bottom) ─────────────────────────────────────
    let copy_rows: Vec<Row> = match &app.copyright_report {
        None => vec![Row::new(vec![
            Cell::from("—").style(Style::default().fg(TEXT_DIM)),
            Cell::from("Loading...").style(Style::default().fg(TEXT_DIM)),
        ])],
        Some(report) => {
            if report.missing.is_empty() {
                vec![Row::new(vec![
                    Cell::from("✓").style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
                    Cell::from(format!("All {} files have copyright headers", report.scanned)).style(Style::default().fg(GREEN)),
                ])]
            } else {
                report.missing.iter().map(|m| {
                    Row::new(vec![
                        Cell::from("✗").style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                        Cell::from(m.path.clone()).style(Style::default().fg(TEXT_DIM)),
                    ])
                }).collect()
            }
        }
    };

    let copy_title = match &app.copyright_report {
        Some(r) => format!(" Copyright Headers ({:.0}% coverage, {} missing) ", r.coverage_pct(), r.missing.len()),
        None => " Copyright Headers ".to_string(),
    };
    let copy_table = Table::new(copy_rows, [Constraint::Length(3), Constraint::Min(40)])
        .block(Block::default()
            .title(copy_title)
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)))
        .column_spacing(1);
    frame.render_widget(copy_table, vsplit[1]);
}

// ─── Distribution tab (item 101) ─────────────────────────────────────────────

fn draw_distribution_tab(frame: &mut Frame, app: &App, area: Rect) {
    let rows: Vec<Row> = match &app.distribution_status {
        None => vec![Row::new(vec![
            Cell::from("Loading…").style(Style::default().fg(TEXT_DIM)),
            Cell::from(""),
            Cell::from(""),
        ])],
        Some(statuses) => statuses
            .iter()
            .enumerate()
            .map(|(i, (platform, status))| {
                let selected = i == app.distribution_selected;
                let row_style = if selected {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };

                let (status_str, status_color) = match status {
                    orrch_core::release::PlatformStatus::NotConfigured => ("—  Not configured", TEXT_DIM),
                    orrch_core::release::PlatformStatus::NotPublished => ("·  Not published", WAITING_COLOR),
                    orrch_core::release::PlatformStatus::Published(_) => ("✓  Published", GREEN),
                };
                let version_str = match status {
                    orrch_core::release::PlatformStatus::Published(v) => v.clone(),
                    _ => String::new(),
                };

                Row::new(vec![
                    Cell::from(platform.label()).style(row_style),
                    Cell::from(status_str).style(Style::default().fg(status_color)),
                    Cell::from(version_str).style(Style::default().fg(TEXT_DIM)),
                ])
            })
            .collect(),
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(22),
            Constraint::Min(10),
        ],
    )
    .header(
        Row::new(vec!["Platform", "Status", "Version"])
            .style(Style::default().fg(TEXT_DIM).add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .title(" Distribution Platforms  [j/k]=select ")
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)),
    )
    .column_spacing(2);
    frame.render_widget(table, area);
}

// ─── History tab (item 107) ───────────────────────────────────────────────────

fn draw_history_tab(frame: &mut Frame, app: &App, area: Rect) {
    let entries: &[orrch_core::release::ReleaseHistoryEntry] = match &app.release_history {
        None => &[],
        Some(v) => v.as_slice(),
    };

    if entries.is_empty() {
        let msg = if app.release_history.is_none() {
            "Loading…"
        } else {
            "No releases found. Create an annotated git tag to start tracking history."
        };
        frame.render_widget(
            Paragraph::new(msg)
                .style(Style::default().fg(TEXT_DIM))
                .block(
                    Block::default()
                        .title(" Release History ")
                        .borders(Borders::ALL)
                        .style(Style::default().fg(TEXT_MUTED)),
                ),
            area,
        );
        return;
    }

    let rows: Vec<Row> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let selected = i == app.history_selected;
            let (tag_style, summary_style) = if i == 0 {
                // Most recent: highlight
                (
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                )
            } else if selected {
                (
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    Style::default().fg(TEXT),
                )
            } else {
                (
                    Style::default().fg(ACCENT),
                    Style::default().fg(TEXT_DIM),
                )
            };
            Row::new(vec![
                Cell::from(entry.tag.clone()).style(tag_style),
                Cell::from(entry.date.clone()).style(Style::default().fg(TEXT_DIM)),
                Cell::from(entry.summary.clone()).style(summary_style),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(vec!["Tag", "Date", "Summary"])
            .style(Style::default().fg(TEXT_DIM).add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .title(format!(" Release History ({} releases)  [j/k]=select ", entries.len()))
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)),
    )
    .column_spacing(2);
    frame.render_widget(table, area);
}

// ─── Marketing tab (item 105) ─────────────────────────────────────────────────

fn draw_marketing_tab(frame: &mut Frame, app: &App, area: Rect) {
    let meta = match &app.marketing_metadata {
        None => {
            frame.render_widget(
                Paragraph::new("Loading…")
                    .style(Style::default().fg(TEXT_DIM))
                    .block(
                        Block::default()
                            .title(" Marketing ")
                            .borders(Borders::ALL)
                            .style(Style::default().fg(TEXT_MUTED)),
                    ),
                area,
            );
            return;
        }
        Some(m) => m,
    };

    // Split into 3 vertical sections
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Description
            Constraint::Min(6),     // Features
            Constraint::Length(6),  // Badges
        ])
        .split(area);

    // ── Description ────────────────────────────────────────────────────
    let desc_title = if meta.version.is_empty() {
        format!(" {} ", meta.project_name)
    } else {
        format!(" {} v{} ", meta.project_name, meta.version)
    };
    let desc_text = if meta.description.is_empty() {
        "(no description in Cargo.toml)".to_string()
    } else {
        meta.description.clone()
    };
    let extra = match (&meta.repository, &meta.license) {
        (Some(repo), Some(lic)) => format!("\n{repo}  •  {lic}"),
        (Some(repo), None) => format!("\n{repo}"),
        (None, Some(lic)) => format!("\nLicense: {lic}"),
        (None, None) => String::new(),
    };
    frame.render_widget(
        Paragraph::new(format!("{desc_text}{extra}"))
            .style(Style::default().fg(TEXT))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(desc_title)
                    .borders(Borders::ALL)
                    .style(Style::default().fg(TEXT_MUTED)),
            ),
        vsplit[0],
    );

    // ── Feature Highlights ─────────────────────────────────────────────
    let feat_lines: Vec<Line> = if meta.features.is_empty() {
        vec![Line::from(Span::styled(
            "No feat: commits found in git log.",
            Style::default().fg(TEXT_DIM),
        ))]
    } else {
        meta.features
            .iter()
            .map(|f| {
                Line::from(vec![
                    Span::styled("  • ", Style::default().fg(ACCENT)),
                    Span::styled(f.clone(), Style::default().fg(TEXT)),
                ])
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(feat_lines)
            .scroll((app.marketing_scroll, 0))
            .block(
                Block::default()
                    .title(format!(" Feature Highlights ({}) ", meta.features.len()))
                    .borders(Borders::ALL)
                    .style(Style::default().fg(TEXT_MUTED)),
            ),
        vsplit[1],
    );

    // ── Badges ─────────────────────────────────────────────────────────
    let badge_text = if meta.badge_snippet.is_empty() {
        "(no badge data available)".to_string()
    } else {
        meta.badge_snippet.clone()
    };
    frame.render_widget(
        Paragraph::new(badge_text)
            .style(Style::default().fg(TEXT_DIM))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" README Badges ")
                    .borders(Borders::ALL)
                    .style(Style::default().fg(TEXT_MUTED)),
            ),
        vsplit[2],
    );
}

fn draw_design(frame: &mut Frame, app: &mut App, area: Rect) {
    use crate::app::DesignSub;

    // Sub-panel selector bar: Intentions │ Workforce │ Library (left-justified)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let bar_focused = app.focus_depth == 1;
    let mut spans: Vec<Span> = Vec::new();
    for (i, sub) in DesignSub::ALL.iter().enumerate() {
        let sel = *sub == app.design_sub;
        let style = if sel {
            let mut s = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
            if bar_focused { s = s.add_modifier(Modifier::UNDERLINED); }
            s
        } else {
            Style::default().fg(TEXT_MUTED)
        };
        spans.push(Span::styled(format!(" {} ", sub.label()), style));
        if i < DesignSub::ALL.len() - 1 {
            spans.push(Span::styled("│", Style::default().fg(TEXT_MUTED)));
        }
    }

    let bg = if bar_focused { Color::Rgb(30, 30, 55) } else { BG_DARK };
    frame.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)), chunks[0]);

    // Poll for pending intake reviews when viewing Intentions and none is loaded.
    // (The main loop also polls every 3s; this gives an immediate refresh on
    // panel switch so the user doesn't have to wait for the next tick.)
    if app.design_sub == DesignSub::Intentions && app.intake_review.is_none() {
        let vault = orrch_core::vault::vault_dir(&app.projects_dir);
        app.intake_review = orrch_core::intake_review::load_intake_review(&vault, &app.projects);
    }

    match app.design_sub {
        DesignSub::Intentions => draw_ideas(frame, app, chunks[1]),
        DesignSub::Workforce => draw_workforce_editor(frame, app, chunks[1]),
        DesignSub::Library => draw_library(frame, app, chunks[1]),
        DesignSub::Plans => draw_plans(frame, app, chunks[1]),
    }
}

// ─── Design > Plans (INS-001) ────────────────────────────────────────

fn draw_plans(frame: &mut Frame, app: &mut App, area: Rect) {
    use orrch_core::FeatureStatus;

    // Lazily populate on first render
    if app.plans_project_indices.is_empty() {
        app.plans_refresh_project_list();
    }

    // Two-column layout: project list (left, 30%) | phase/feature tree (right, 70%)
    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    // ── Left pane: project list ──
    let left_focused = !app.plans_focus_right && app.focus_depth >= app.content_depth();
    let left_border = if left_focused { Style::default().fg(ACCENT) } else { Style::default().fg(TEXT_MUTED) };

    let proj_items: Vec<ListItem> = app.plans_project_indices.iter().enumerate().map(|(i, &pidx)| {
        let proj = &app.projects[pidx];
        let done: usize = proj.plan_phases.iter().map(|p| p.done_count()).sum();
        let total: usize = proj.plan_phases.iter().map(|p| p.total_count()).sum();
        let color = if done == total && total > 0 { GREEN } else if done > 0 { TEXT_DIM } else { TEXT };
        let sel = i == app.plans_project_selected;
        let style = if sel && left_focused {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };
        ListItem::new(Line::from(vec![
            Span::styled(format!(" {} ", proj.name), style),
            Span::styled(format!("({done}/{total})"), Style::default().fg(TEXT_MUTED)),
        ]))
    }).collect();

    let proj_list = List::new(proj_items)
        .scroll_padding(SCROLL_PAD)
        .block(Block::default().title(" Projects ").borders(Borders::ALL).style(left_border))
        .highlight_style(Style::default().bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    let left_sel = if left_focused { Some(app.plans_project_selected) } else { None };
    let mut left_state = ListState::default().with_selected(left_sel);
    frame.render_stateful_widget(proj_list, hsplit[0], &mut left_state);

    // ── Right pane: phase/feature tree ──
    let right_focused = app.plans_focus_right && app.focus_depth >= app.content_depth();
    let right_border = if right_focused { Style::default().fg(ACCENT) } else { Style::default().fg(TEXT_MUTED) };

    let proj_idx = app.plans_current_project_idx();
    let Some(pidx) = proj_idx else {
        let empty = Paragraph::new("No projects with PLAN.md found")
            .style(Style::default().fg(TEXT_MUTED))
            .block(Block::default().title(" Plan ").borders(Borders::ALL).style(right_border));
        frame.render_widget(empty, hsplit[1]);
        return;
    };
    let Some(proj) = app.projects.get(pidx) else { return; };

    let mut items: Vec<ListItem> = Vec::new();
    for (pi, phase) in proj.plan_phases.iter().enumerate() {
        let expanded = app.plans_phase_expanded == pi;
        let arrow = if expanded { "▾" } else { "▸" };
        let done = phase.done_count();
        let total = phase.total_count();
        let progress = if total > 0 { format!(" ({done}/{total})") } else { String::new() };

        let phase_color = if done == total && total > 0 {
            GREEN
        } else if done > 0 {
            TEXT_DIM
        } else {
            TEXT
        };

        let phase_name = if let Some(num) = phase.number {
            format!("{arrow} Phase {num}: {}{progress}", phase.name)
        } else {
            format!("{arrow} {}{progress}", phase.name)
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(phase_name, Style::default().fg(phase_color).add_modifier(Modifier::BOLD)),
        ])));

        if expanded {
            for feat in &phase.features {
                let icon = feat.status.display_icon();
                let style = feature_status_style(feat.status);
                let color = style.fg.unwrap_or(TEXT);
                let id_str = feat.id.map(|n| format!("{n}. ")).unwrap_or_default();
                let title = format!("  {icon} {id_str}{}", feat.title);

                let mut spans: Vec<Span> = vec![Span::styled(title, Style::default().fg(color))];

                if feat.user_verified || feat.status == FeatureStatus::Verified {
                    spans.push(Span::styled(" ✓", Style::default().fg(GREEN)));
                }

                // Status label for non-trivial statuses
                if !matches!(feat.status, FeatureStatus::Planned | FeatureStatus::Pending | FeatureStatus::Done) {
                    spans.push(Span::styled(
                        format!(" [{}]", feat.status.label()),
                        Style::default().fg(TEXT_MUTED),
                    ));
                }

                items.push(ListItem::new(Line::from(spans)));
            }
        }
    }

    let total_done: usize = proj.plan_phases.iter().map(|p| p.done_count()).sum();
    let total_all: usize = proj.plan_phases.iter().map(|p| p.total_count()).sum();
    let block_title = format!(" {} — Plan ({total_done}/{total_all}) ", proj.name);

    // Footer hint
    let footer = " Enter=expand v=verify s/S=cycle d=deprecate k/j=move e=edit r=refresh ";
    let right_block = Block::default()
        .title(block_title)
        .title_bottom(Line::from(Span::styled(footer, Style::default().fg(TEXT_MUTED))))
        .borders(Borders::ALL)
        .style(right_border);

    let list = List::new(items)
        .scroll_padding(SCROLL_PAD)
        .block(right_block)
        .highlight_style(Style::default().bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    let right_sel = if right_focused { Some(app.plans_tree_selected) } else { None };
    let mut right_state = ListState::default().with_selected(right_sel);
    frame.render_stateful_widget(list, hsplit[1], &mut right_state);
}

fn draw_workforce_editor(frame: &mut Frame, app: &App, area: Rect) {
    use crate::app::WorkforceTab;

    // Layout: tab bar (1 line) + content (split list + preview)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    // Tab bar for workforce sub-tabs
    let bar_focused = app.focus_depth == 2 && app.design_sub == crate::app::DesignSub::Workforce;
    let tab_spans: Vec<Span> = WorkforceTab::ALL.iter()
        .flat_map(|tab| {
            let sel = *tab == app.workforce_tab;
            let style = if sel {
                let mut s = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
                if bar_focused { s = s.add_modifier(Modifier::UNDERLINED); }
                s
            } else {
                Style::default().fg(TEXT_MUTED)
            };
            vec![
                Span::styled(format!(" {} ", tab.label()), style),
                Span::styled("│", Style::default().fg(TEXT_MUTED)),
            ]
        })
        .collect();
    let bg = if bar_focused { Color::Rgb(30, 30, 55) } else { BG_DARK };
    frame.render_widget(Paragraph::new(Line::from(tab_spans)).style(Style::default().bg(bg)), outer[0]);

    // "Coming soon" tabs
    if matches!(app.workforce_tab, WorkforceTab::TrainingData | WorkforceTab::Models) {
        let msg = Paragraph::new(format!("{} — coming soon.", app.workforce_tab.label()))
            .style(Style::default().fg(TEXT_DIM))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(msg, outer[1]);
        return;
    }

    // Split: list (40%) + preview (60%)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(outer[1]);

    // Harnesses tab: availability-aware list + structured preview
    if app.workforce_tab == WorkforceTab::Harnesses {
        draw_workforce_harnesses(frame, app, chunks[0], chunks[1]);
        return;
    }

    let items_data = app.wf_items_for_tab();
    let visible_rows = chunks[0].height.saturating_sub(2) as usize;
    let scroll_offset = if app.wf_selected >= visible_rows { app.wf_selected - visible_rows + 1 } else { 0 };

    let mut list_items = Vec::new();
    for (i, (name, _)) in items_data.iter().enumerate().skip(scroll_offset) {
        let sel = app.wf_selected == i;
        let style = if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) };
        let marker = if sel { "■ " } else { "  " };
        list_items.push(ListItem::new(Line::styled(format!("{marker}{name}"), style)));
    }
    if list_items.is_empty() {
        list_items.push(ListItem::new(Line::styled("  (empty — press n to create)", Style::default().fg(TEXT_MUTED))));
    }

    // Scroll indicators
    let has_above = scroll_offset > 0;
    let has_below = items_data.len() > scroll_offset + visible_rows;
    let scroll_hint = match (has_above, has_below) {
        (true, true) => " [..v^..]",
        (true, false) => " [..^]",
        (false, true) => " [v..]",
        (false, false) => "",
    };

    let title = if app.workforce_tab == WorkforceTab::Workflows {
        format!(
            " {} ({}) — n=new N=AI Enter=edit r=rename d=del x=export i=import R=refresh{}",
            app.workforce_tab.label(), items_data.len(), scroll_hint,
        )
    } else {
        format!(
            " {} ({}) — n=new N=AI Enter=edit r=rename d=del R=refresh{}",
            app.workforce_tab.label(), items_data.len(), scroll_hint,
        )
    };
    frame.render_widget(List::new(list_items).block(Block::default().title(title).borders(Borders::ALL)), chunks[0]);

    // Preview: show file contents with markdown rendering
    let preview = if let Some((_, path)) = items_data.get(app.wf_selected) {
        if let Ok(content) = std::fs::read_to_string(path) {
            markdown_to_lines(&content)
        } else {
            vec![Line::styled("Cannot read file", Style::default().fg(TEXT_MUTED))]
        }
    } else {
        vec![Line::styled("Select an item to preview", Style::default().fg(TEXT_MUTED))]
    };

    frame.render_widget(Paragraph::new(preview)
        .block(Block::default().title(" Preview (PgUp/PgDn) ").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((app.wf_preview_scroll as u16, 0)), chunks[1]);
}

fn draw_workforce_harnesses(frame: &mut Frame, app: &App, list_area: Rect, preview_area: Rect) {
    // Known repo URLs for the 5 standard harnesses
    fn repo_url(name: &str) -> &'static str {
        match name {
            "claude_code" | "claude-code" => "github.com/anthropics/claude-code",
            "opencode" => "github.com/sst/opencode",
            "crush" => "N/A",
            "codex" | "codex-cli" => "github.com/openai/codex-cli",
            "gemini_cli" | "gemini-cli" => "github.com/google-gemini/gemini-cli",
            _ => "N/A",
        }
    }

    let visible_rows = list_area.height.saturating_sub(2) as usize;
    let scroll_offset = if app.wf_selected >= visible_rows { app.wf_selected - visible_rows + 1 } else { 0 };

    let mut list_items = Vec::new();
    for (i, h) in app.library_harnesses.iter().enumerate().skip(scroll_offset) {
        let sel = app.wf_selected == i;
        let style = if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) };
        let marker = if sel { "■ " } else { "  " };
        let (indicator, ind_style) = if h.available {
            ("● ", Style::default().fg(GREEN))
        } else {
            ("○ ", Style::default().fg(TEXT_MUTED).add_modifier(Modifier::DIM))
        };
        list_items.push(ListItem::new(Line::from(vec![
            Span::styled(marker.to_owned(), style),
            Span::styled(indicator.to_owned(), ind_style),
            Span::styled(h.name.clone(), style),
        ])));
    }
    if list_items.is_empty() {
        list_items.push(ListItem::new(Line::styled(
            "  No harnesses in library/harnesses/",
            Style::default().fg(TEXT_MUTED),
        )));
    }

    let title = format!(" Harnesses ({}) ", app.library_harnesses.len());
    frame.render_widget(
        List::new(list_items).block(Block::default().title(title).borders(Borders::ALL)),
        list_area,
    );

    let preview = if let Some(h) = app.library_harnesses.get(app.wf_selected) {
        let status_line = if h.available {
            Line::styled("● Available", Style::default().fg(GREEN))
        } else {
            Line::styled("○ Not Found", Style::default().fg(WAITING_COLOR))
        };
        let repo = repo_url(&h.name);
        let mut lines = vec![
            Line::styled(h.name.clone(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            status_line,
            Line::styled(format!("Command: {}", h.command), Style::default().fg(TEXT)),
            Line::styled(format!("Repo:    {}", repo), Style::default().fg(CYAN)),
            Line::raw(""),
        ];
        if !h.notes.is_empty() {
            lines.extend(markdown_to_lines(&h.notes));
            lines.push(Line::raw(""));
        }
        lines.push(Line::styled("Source: [not indexed yet]", Style::default().fg(TEXT_MUTED)));
        lines
    } else {
        vec![Line::styled("Select a harness to preview", Style::default().fg(TEXT_MUTED))]
    };

    frame.render_widget(
        Paragraph::new(preview)
            .block(Block::default().title(" Preview (PgUp/PgDn) ").borders(Borders::ALL))
            .wrap(Wrap { trim: false })
            .scroll((app.wf_preview_scroll as u16, 0)),
        preview_area,
    );
}

// ─── Library Panel ───────────────────────────────────────────────────

fn draw_library(frame: &mut Frame, app: &mut App, area: Rect) {
    use crate::app::{DesignSub, LibrarySub};

    // Layout: sub-panel selector (1 line) + content
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    // Sub-panel selector bar
    let bar_focused = app.focus_depth == 2 && app.design_sub == DesignSub::Library;
    let sub_labels: Vec<Span> = LibrarySub::ALL.iter()
        .flat_map(|sub| {
            let sel = *sub == app.library_sub;
            let count = match sub {
                LibrarySub::Fit => app.fit_results.len(),
                LibrarySub::Agents => app.agent_profiles.len(),
                LibrarySub::Models => app.library_models.len(),
                LibrarySub::Harnesses => app.library_harnesses.len(),
                LibrarySub::McpServers => app.library_mcp_servers.len(),
                LibrarySub::Skills => app.library_skills.len(),
                LibrarySub::Tools => app.library_tools.len(),
                LibrarySub::PiExtensions => app.library_pi_extensions.len(),
            };
            let style = if sel {
                let mut s = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
                if bar_focused { s = s.add_modifier(Modifier::UNDERLINED); }
                s
            } else {
                Style::default().fg(TEXT_MUTED)
            };
            vec![
                Span::styled(format!(" {} ({}) ", sub.label(), count), style),
                Span::styled(" │ ", Style::default().fg(TEXT_MUTED)),
            ]
        })
        .collect();
    let bg = if bar_focused { Color::Rgb(30, 30, 55) } else { BG_DARK };
    frame.render_widget(Paragraph::new(Line::from(sub_labels)).style(Style::default().bg(bg)), outer[0]);

    // Split content: list (40%) + preview (60%)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(outer[1]);

    // HWF-005: lazy-probe the selected machine on first view of the Fit tab and
    // whenever the selected machine changes. refresh_fit reuses the per-host
    // 30-min probe cache (fresh=false), so this is NOT a per-frame re-probe.
    if app.library_sub == crate::app::LibrarySub::Fit {
        let cur = app.fit_registry.all().get(app.fit_machine_idx).map(|m| m.name.clone());
        if app.fit_probed_machine != cur {
            app.refresh_fit(false);
        }
    }

    match app.library_sub {
        LibrarySub::Fit => draw_library_fit(frame, app, chunks[0], chunks[1]),
        LibrarySub::Agents => draw_library_agents(frame, app, chunks[0], chunks[1]),
        LibrarySub::Models => draw_library_models(frame, app, chunks[0], chunks[1]),
        LibrarySub::Harnesses => draw_library_harnesses(frame, app, chunks[0], chunks[1]),
        LibrarySub::McpServers => draw_library_mcp(frame, app, chunks[0], chunks[1]),
        LibrarySub::Skills => draw_library_generic(frame, app, &app.library_skills, "Skills", "x=export to PI", chunks[0], chunks[1]),
        LibrarySub::Tools => draw_library_generic(frame, app, &app.library_tools, "Tools", "x=export to PI", chunks[0], chunks[1]),
        LibrarySub::PiExtensions => draw_library_pi_extensions(frame, app, chunks[0], chunks[1]),
    }
}

fn draw_library_agents(frame: &mut Frame, app: &App, list_area: Rect, preview_area: Rect) {
    let visible_rows = list_area.height.saturating_sub(2) as usize; // minus borders
    let scroll_offset = if app.library_selected >= visible_rows {
        app.library_selected - visible_rows + 1
    } else { 0 };

    let mut items = Vec::new();
    for (i, profile) in app.agent_profiles.iter().enumerate().skip(scroll_offset) {
        let sel = app.library_selected == i;
        let style = if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) };
        let marker = if sel { "■ " } else { "  " };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{marker}{}", profile.name), style),
            Span::styled(format!(" [{}]", profile.department), Style::default().fg(TEXT_MUTED)),
        ])));
    }
    let title = format!(" Agents ({}) — n=new N=AI-assisted Enter=edit d=del ", app.agent_profiles.len());
    frame.render_widget(List::new(items).block(Block::default().title(title).borders(Borders::ALL)), list_area);

    let preview = if let Some(p) = app.agent_profiles.get(app.library_selected) {
        let mut lines = vec![
            Line::styled(&p.name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Line::styled(format!("Role: {}", p.role), Style::default().fg(TEXT)),
            Line::styled(format!("Dept: {}", p.department), Style::default().fg(TEXT_DIM)),
            Line::raw(""),
        ];
        lines.extend(markdown_to_lines(&p.prompt));
        lines
    } else { vec![Line::styled("No agents loaded — press n to create", Style::default().fg(TEXT_MUTED))] };
    frame.render_widget(Paragraph::new(preview)
        .block(Block::default().title(" Preview (PgUp/PgDn) ").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((app.library_preview_scroll as u16, 0)), preview_area);
}

fn draw_library_models(frame: &mut Frame, app: &App, list_area: Rect, preview_area: Rect) {
    let visible_rows = list_area.height.saturating_sub(2) as usize;
    let scroll_offset = if app.library_selected >= visible_rows { app.library_selected - visible_rows + 1 } else { 0 };
    let mut items = Vec::new();
    for (i, model) in app.library_models.iter().enumerate().skip(scroll_offset) {
        let sel = app.library_selected == i;
        let blocked = app.valve_store.is_blocked(&model.provider);
        let style = if blocked {
            Style::default().fg(TEXT_MUTED).add_modifier(Modifier::DIM)
        } else if sel {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        let marker = if sel { "■ " } else { "  " };
        let tier_color = match model.tier {
            orrch_library::ModelTier::Enterprise => ACCENT,
            orrch_library::ModelTier::MidTier => CYAN,
            orrch_library::ModelTier::Local => GREEN,
        };
        let throttled = app.usage_tracker.is_throttled(&model.provider);
        let status_badge = if blocked {
            Span::styled(" ⊘ BLOCKED", Style::default().fg(ACCENT))
        } else if throttled {
            Span::styled(" [THROTTLED]", Style::default().fg(WAITING_COLOR))
        } else {
            Span::raw("")
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{marker}{}", model.name), style),
            Span::styled(format!(" {}", model.tier.label()), Style::default().fg(tier_color)),
            status_badge,
        ])));
    }
    if items.is_empty() { items.push(ListItem::new(Line::styled("  No models in library/models/", Style::default().fg(TEXT_MUTED)))); }
    let title = format!(" Models ({}) — v=valve n=new Enter=edit ", app.library_models.len());
    frame.render_widget(List::new(items).block(Block::default().title(title).borders(Borders::ALL)), list_area);

    let preview = if let Some(m) = app.library_models.get(app.library_selected) {
        let blocked = app.valve_store.is_blocked(&m.provider);
        let throttled = app.usage_tracker.is_throttled(&m.provider);
        let status_info = if blocked {
            let valve = app.valve_store.valves.get(&m.provider);
            let reason = valve.map(|v| v.reason.as_str()).unwrap_or("unknown");
            let reopen = valve.map(|v| v.reopen_display()).unwrap_or_else(|| "manual".into());
            vec![
                Line::styled(format!("⊘ VALVE CLOSED — {}", reason), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                Line::styled(format!("  Reopens: {}", reopen), Style::default().fg(WAITING_COLOR)),
                Line::raw(""),
            ]
        } else if throttled {
            let reason = app.usage_tracker.throttle_reason(&m.provider).unwrap_or("rate limited");
            vec![
                Line::styled(format!("[THROTTLED] — {}", reason), Style::default().fg(WAITING_COLOR).add_modifier(Modifier::BOLD)),
                Line::raw(""),
            ]
        } else {
            vec![]
        };
        let mut lines = status_info;
        lines.extend(vec![
            Line::styled(&m.name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Line::styled(format!("Provider: {}", m.provider), Style::default().fg(TEXT)),
            Line::styled(format!("Model ID: {}", m.model_id), Style::default().fg(TEXT)),
            Line::styled(format!("Tier: {}", m.tier.label()), Style::default().fg(TEXT)),
            Line::styled(format!("Pricing: {}", m.pricing.display()), Style::default().fg(TEXT)),
            Line::styled(format!("Context: {}",
                m.max_context.map(|c| if c >= 1_000_000 { format!("{}M", c / 1_000_000) } else { format!("{}K", c / 1000) }).unwrap_or("unknown".into())),
                Style::default().fg(TEXT)),
            Line::styled(format!("API Key: {}", m.api_key_env.as_deref().unwrap_or("none")), Style::default().fg(TEXT_DIM)),
            Line::raw(""),
            Line::styled("Capabilities:", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Line::styled(format!("  {}", m.capabilities.join(", ")), Style::default().fg(GREEN)),
            Line::raw(""),
            Line::styled("Limitations:", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Line::styled(format!("  {}", m.limitations.join(", ")), Style::default().fg(WAITING_COLOR)),
            Line::raw(""),
            Line::styled(&m.notes, Style::default().fg(TEXT_DIM)),
        ]);
        lines
    } else { vec![Line::styled("No model selected", Style::default().fg(TEXT_MUTED))] };
    frame.render_widget(Paragraph::new(preview).block(Block::default().title(" Details (PgUp/PgDn) ").borders(Borders::ALL)).wrap(Wrap { trim: false }).scroll((app.library_preview_scroll as u16, 0)), preview_area);
}

/// HWF-005: hardware-fit assessment panel. LIST area = machine selector header +
/// ranked model table; PREVIEW area = details for the selected row.
fn draw_library_fit(frame: &mut Frame, app: &App, list_area: Rect, preview_area: Rect) {
    // Color helper for fit_level → severity color.
    fn fit_color(level: &str) -> Color {
        match level {
            "perfect" => GREEN,
            "good" => CYAN,
            "marginal" => WAITING_COLOR,
            _ => ACCENT, // "too_tight" / "no_fit"
        }
    }

    let machines = app.fit_registry.all();
    let machine_name = machines
        .get(app.fit_machine_idx)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "localhost".into());

    // ── header (3 lines) ───────────────────────────────────────────────
    let mut header: Vec<Line> = Vec::new();
    header.push(Line::from(vec![
        Span::styled(" Machine: ", Style::default().fg(TEXT)),
        Span::styled(machine_name.clone(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(" (m/M switch · R rescan)", Style::default().fg(TEXT_DIM)),
    ]));
    // probe summary line
    let summary_line = match &app.fit_system {
        Some(sys) if sys.error.is_some() => Line::styled(
            format!(" {}", sys.error.as_deref().unwrap_or("probe error")),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Some(sys) => {
            let gpu = match (&sys.gpu_name, sys.gpu_vram_gb) {
                (Some(name), Some(v)) => format!("{} {:.1}GB", name, v),
                (Some(name), None) => name.clone(),
                _ => "no GPU".to_string(),
            };
            Line::styled(
                format!(" {} · {:.1}GB RAM · {}", sys.backend, sys.total_ram_gb, gpu),
                Style::default().fg(TEXT),
            )
        }
        None => Line::styled(" probing…", Style::default().fg(TEXT_DIM)),
    };
    header.push(summary_line);
    // column headers
    header.push(Line::styled(
        format!(
            " {:<20}{:<10}{:<12}{:<10}{:<8}{:<10}{:<8}{:<6}",
            "model", "fit", "run_mode", "quant", "ctx", "req_gb", "tok/s", "score"
        ),
        Style::default().fg(TEXT_DIM).add_modifier(Modifier::BOLD),
    ));

    // ── table rows (scrolled by fit_row) ───────────────────────────────
    // Available height = list_area minus 2 borders minus 3 header lines.
    let body_rows = list_area.height.saturating_sub(2).saturating_sub(3) as usize;
    let scroll_offset = if app.fit_row >= body_rows && body_rows > 0 {
        app.fit_row - body_rows + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = header;
    if app.fit_results.is_empty() {
        lines.push(Line::styled(
            "  No fit results — press R to scan",
            Style::default().fg(TEXT_MUTED),
        ));
    } else {
        for (i, r) in app.fit_results.iter().enumerate().skip(scroll_offset).take(body_rows.max(1)) {
            let sel = app.fit_row == i;
            let marker = if sel { "■ " } else { "  " };
            let base = if sel {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            let name_trunc: String = if r.name.chars().count() > 17 {
                format!("{}…", r.name.chars().take(16).collect::<String>())
            } else {
                r.name.clone()
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker}{:<18}", name_trunc), base),
                Span::styled(format!("{:<10}", r.fit_level), Style::default().fg(fit_color(&r.fit_level))),
                Span::styled(format!("{:<12}", r.run_mode), Style::default().fg(TEXT_DIM)),
                Span::styled(format!("{:<10}", r.quant), Style::default().fg(TEXT)),
                Span::styled(format!("{:<8}", r.context), Style::default().fg(TEXT)),
                Span::styled(format!("{:<10.1}", r.required_gb), Style::default().fg(TEXT)),
                Span::styled(format!("{:<8.1}", r.speed_tps), Style::default().fg(TEXT)),
                Span::styled(format!("{:<6.1}", r.score), base),
            ]));
        }
    }

    let title = format!(" Fit — {} ({} models) ", machine_name, app.fit_results.len());
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().title(title).borders(Borders::ALL)),
        list_area,
    );

    // ── preview: details for selected row ──────────────────────────────
    let preview = if let Some(r) = app.fit_results.get(app.fit_row) {
        vec![
            Line::styled(&r.name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Line::styled(format!("Provider: {}", r.provider), Style::default().fg(TEXT)),
            Line::styled(format!("Params: {:.1}B", r.params_b), Style::default().fg(TEXT)),
            Line::styled(format!("MoE: {}", r.is_moe), Style::default().fg(TEXT)),
            Line::styled(format!("Use case: {}", r.use_case), Style::default().fg(TEXT)),
            Line::raw(""),
            Line::styled(format!("Fit level: {}", r.fit_level), Style::default().fg(fit_color(&r.fit_level)).add_modifier(Modifier::BOLD)),
            Line::styled(format!("Run mode: {}", r.run_mode), Style::default().fg(TEXT)),
            Line::styled(format!("Quant: {}", r.quant), Style::default().fg(TEXT)),
            Line::styled(format!("Context length: {}", r.context_length), Style::default().fg(TEXT)),
            Line::styled(format!("Required: {:.1}GB", r.required_gb), Style::default().fg(TEXT)),
            Line::styled(format!("Speed: {:.1} tok/s", r.speed_tps), Style::default().fg(TEXT)),
            Line::raw(""),
            Line::styled("Scores:", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Line::styled(format!("  quality: {:.1}", r.scores.quality), Style::default().fg(TEXT_DIM)),
            Line::styled(format!("  speed:   {:.1}", r.scores.speed), Style::default().fg(TEXT_DIM)),
            Line::styled(format!("  fit:     {:.1}", r.scores.fit), Style::default().fg(TEXT_DIM)),
            Line::styled(format!("  context: {:.1}", r.scores.context), Style::default().fg(TEXT_DIM)),
            Line::raw(""),
            Line::styled(format!("Composite score: {:.1}", r.score), Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        ]
    } else {
        vec![Line::styled("No machine probed", Style::default().fg(TEXT_MUTED))]
    };
    frame.render_widget(
        Paragraph::new(preview)
            .block(Block::default().title(" Fit Details (PgUp/PgDn) ").borders(Borders::ALL))
            .wrap(Wrap { trim: false })
            .scroll((app.library_preview_scroll as u16, 0)),
        preview_area,
    );
}

fn draw_library_harnesses(frame: &mut Frame, app: &App, list_area: Rect, preview_area: Rect) {
    let visible_rows = list_area.height.saturating_sub(2) as usize;
    let scroll_offset = if app.library_selected >= visible_rows { app.library_selected - visible_rows + 1 } else { 0 };
    let mut items = Vec::new();
    for (i, h) in app.library_harnesses.iter().enumerate().skip(scroll_offset) {
        let sel = app.library_selected == i;
        let style = if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) };
        let marker = if sel { "■ " } else { "  " };
        let status = if h.available { Span::styled(" ●", Style::default().fg(GREEN)) } else { Span::styled(" ○", Style::default().fg(TEXT_MUTED)) };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{marker}{}", h.name), style),
            status,
        ])));
    }
    if items.is_empty() { items.push(ListItem::new(Line::styled("  No harnesses in library/harnesses/", Style::default().fg(TEXT_MUTED)))); }
    frame.render_widget(List::new(items).block(Block::default().title(" Harnesses ").borders(Borders::ALL)), list_area);

    let preview = if let Some(h) = app.library_harnesses.get(app.library_selected) {
        let status_line = if h.available {
            Line::styled("● Installed", Style::default().fg(GREEN))
        } else {
            Line::styled("○ Not found", Style::default().fg(WAITING_COLOR))
        };
        vec![
            Line::styled(&h.name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            status_line,
            Line::styled(format!("Command: {}", h.command), Style::default().fg(TEXT)),
            Line::styled(&h.description, Style::default().fg(TEXT_DIM)),
            Line::raw(""),
            Line::styled("Capabilities:", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Line::styled(format!("  {}", h.capabilities.join(", ")), Style::default().fg(GREEN)),
            Line::raw(""),
            Line::styled("Supported Models:", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Line::styled(format!("  {}", h.supported_models.join(", ")), Style::default().fg(CYAN)),
            Line::raw(""),
            Line::styled(format!("Flags: {}", if h.flags.is_empty() { "(none)".into() } else { h.flags.join(" ") }), Style::default().fg(TEXT_DIM)),
            Line::raw(""),
            Line::styled(&h.notes, Style::default().fg(TEXT_DIM)),
        ]
    } else { vec![Line::styled("No harness selected", Style::default().fg(TEXT_MUTED))] };
    frame.render_widget(Paragraph::new(preview).block(Block::default().title(" Details (PgUp/PgDn) ").borders(Borders::ALL)).wrap(Wrap { trim: false }).scroll((app.library_preview_scroll as u16, 0)), preview_area);
}

fn draw_library_mcp(frame: &mut Frame, app: &App, list_area: Rect, preview_area: Rect) {
    let mut items = Vec::new();
    for (i, server) in app.library_mcp_servers.iter().enumerate() {
        let sel = app.library_selected == i;
        let style = if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) };
        let marker = if sel { "■ " } else { "  " };
        let status = if server.enabled {
            Span::styled(" ●", Style::default().fg(GREEN))
        } else {
            Span::styled(" ○", Style::default().fg(TEXT_MUTED))
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{marker}{}", server.name), style),
            status,
        ])));
    }
    if items.is_empty() {
        items.push(ListItem::new(Line::styled("  No MCP servers configured", Style::default().fg(TEXT_MUTED))));
        items.push(ListItem::new(Line::styled("  Add .md files to library/mcp_servers/", Style::default().fg(TEXT_MUTED))));
    }
    frame.render_widget(List::new(items).block(Block::default().title(" MCP Servers (e=toggle) ").borders(Borders::ALL)), list_area);

    let preview = if let Some(s) = app.library_mcp_servers.get(app.library_selected) {
        let transport_info = match &s.transport {
            orrch_library::McpTransport::Stdio { command, args, .. } => {
                format!("stdio: {} {}", command, args.join(" "))
            }
            orrch_library::McpTransport::Sse { url } => format!("sse: {}", url),
        };
        let mut lines = vec![
            Line::styled(&s.name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Line::styled(if s.enabled { "● Enabled" } else { "○ Disabled" },
                Style::default().fg(if s.enabled { GREEN } else { TEXT_MUTED })),
            Line::styled(&s.description, Style::default().fg(TEXT_DIM)),
            Line::raw(""),
            Line::styled(format!("Transport: {}", transport_info), Style::default().fg(TEXT)),
        ];
        if !s.assigned_roles.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::styled("Assigned to:", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)));
            lines.push(Line::styled(format!("  {}", s.assigned_roles.join(", ")), Style::default().fg(CYAN)));
        } else {
            lines.push(Line::styled("  Available to all agents", Style::default().fg(TEXT_DIM)));
        }
        if !s.notes.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::styled(&s.notes, Style::default().fg(TEXT_DIM)));
        }
        lines
    } else {
        vec![
            Line::styled("orrch-mcp (planned)", Style::default().fg(TEXT_MUTED)),
            Line::raw(""),
            Line::styled("Unified MCP server exposing:", Style::default().fg(TEXT_DIM)),
            Line::styled("  library_search, library_get", Style::default().fg(TEXT_DIM)),
            Line::styled("  project_state, inbox_append", Style::default().fg(TEXT_DIM)),
            Line::styled("  operation_status, session_list", Style::default().fg(TEXT_DIM)),
        ]
    };
    frame.render_widget(Paragraph::new(preview).block(Block::default().title(" Details (PgUp/PgDn) ").borders(Borders::ALL)).wrap(Wrap { trim: false }).scroll((app.library_preview_scroll as u16, 0)), preview_area);
}

fn draw_library_generic(frame: &mut Frame, app: &App, items_data: &[(String, std::path::PathBuf)], label: &str, extra_hint: &str, list_area: Rect, preview_area: Rect) {
    let visible_rows = list_area.height.saturating_sub(2) as usize;
    let scroll_offset = if app.library_selected >= visible_rows { app.library_selected - visible_rows + 1 } else { 0 };
    let mut items = Vec::new();
    for (i, (name, _)) in items_data.iter().enumerate().skip(scroll_offset) {
        let sel = app.library_selected == i;
        let style = if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) };
        let marker = if sel { "■ " } else { "  " };
        items.push(ListItem::new(Line::styled(format!("{marker}{name}"), style)));
    }
    if items.is_empty() {
        items.push(ListItem::new(Line::styled(format!("  No {label} — create in Workforce editor"), Style::default().fg(TEXT_MUTED))));
    }
    // Scroll indicators
    let has_above = scroll_offset > 0;
    let has_below = items_data.len() > scroll_offset + visible_rows;
    let scroll_hint = match (has_above, has_below) {
        (true, true) => " [..v^..]",
        (true, false) => " [..^]",
        (false, true) => " [v..]",
        (false, false) => "",
    };
    let hint_part = if extra_hint.is_empty() { String::new() } else { format!(" {extra_hint}") };
    let title = format!(" {} ({}) r=refresh{}{} ", label, items_data.len(), hint_part, scroll_hint);
    frame.render_widget(List::new(items).block(Block::default().title(title).borders(Borders::ALL)), list_area);

    let preview = if let Some((_, path)) = items_data.get(app.library_selected) {
        if let Ok(content) = std::fs::read_to_string(path) {
            markdown_to_lines(&content)
        } else {
            vec![Line::styled("Cannot read file", Style::default().fg(TEXT_MUTED))]
        }
    } else {
        vec![Line::styled("Select an item to preview", Style::default().fg(TEXT_MUTED))]
    };
    frame.render_widget(Paragraph::new(preview)
        .block(Block::default().title(" Preview (PgUp/PgDn) ").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((app.library_preview_scroll as u16, 0)), preview_area);
}

fn draw_library_pi_extensions(frame: &mut Frame, app: &App, list_area: Rect, preview_area: Rect) {
    let items_data = &app.library_pi_extensions;
    let visible_rows = list_area.height.saturating_sub(2) as usize;
    let scroll_offset = if app.library_selected >= visible_rows { app.library_selected - visible_rows + 1 } else { 0 };
    let mut items = Vec::new();
    for (i, item) in items_data.iter().enumerate().skip(scroll_offset) {
        let sel = app.library_selected == i;
        let style = if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) };
        let marker = if sel { "■ " } else { "  " };
        items.push(ListItem::new(Line::styled(format!("{marker}{}.ts", item.name), style)));
    }
    if items.is_empty() {
        items.push(ListItem::new(Line::styled("  No PI extensions — press 'n' to create, or 'x' on a Skill/Tool to export", Style::default().fg(TEXT_MUTED))));
    }
    let has_above = scroll_offset > 0;
    let has_below = items_data.len() > scroll_offset + visible_rows;
    let scroll_hint = match (has_above, has_below) {
        (true, true) => " [..v^..]",
        (true, false) => " [..^]",
        (false, true) => " [v..]",
        (false, false) => "",
    };
    let title = format!(" PI Extensions ({}) n=new e=edit r=refresh{} ", items_data.len(), scroll_hint);
    frame.render_widget(List::new(items).block(Block::default().title(title).borders(Borders::ALL)), list_area);

    let preview = if let Some(item) = items_data.get(app.library_selected) {
        let lines: Vec<Line> = item.content.lines()
            .map(|l| Line::styled(l.to_string(), Style::default().fg(TEXT)))
            .collect();
        if lines.is_empty() {
            vec![Line::styled("(empty)", Style::default().fg(TEXT_MUTED))]
        } else {
            lines
        }
    } else {
        vec![Line::styled("Select an extension to preview", Style::default().fg(TEXT_MUTED))]
    };
    frame.render_widget(Paragraph::new(preview)
        .block(Block::default().title(" Preview (PgUp/PgDn) ").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((app.library_preview_scroll as u16, 0)), preview_area);
}

// ─── Ideas (Design > Intentions) ────────────────────────────────────

fn draw_ideas(frame: &mut Frame, app: &App, area: Rect) {
    // Intake review takes over the full area
    if app.intake_review.is_some() {
        draw_intake_review(frame, app, area);
        return;
    }
    if app.ideas.is_empty() {
        let msg = Paragraph::new("No ideas yet. Press 'n' to create one.\n\nWrite feedback, ideas, or instructions here.\nPress 's' to submit through the instruction intake pipeline.")
            .style(Style::default().fg(TEXT_DIM))
            .block(Block::default().title(" Intentions — n=new s=submit Enter=edit ").borders(Borders::ALL));
        frame.render_widget(msg, area);
        return;
    }

    // Color constants for gradient
    let default_rgb = (230, 230, 240); // TEXT
    let yellow_rgb = (255, 200, 50);   // WAITING_COLOR
    let green_rgb = (80, 200, 120);    // GREEN

    let items: Vec<ListItem> = app.ideas.iter().enumerate().map(|(idx, idea)| {
        let (r, g, b) = idea.pipeline.gradient_color(default_rgb, yellow_rgb, green_rgb);
        let title_style = Style::default().fg(Color::Rgb(r, g, b)).add_modifier(Modifier::BOLD);

        // Build status badge
        let badge = if idea.pipeline.is_complete() {
            " ✓ 100%".to_string()
        } else if idea.pipeline.is_submitted() {
            let pct = idea.pipeline.progress;
            if pct >= 50 {
                let impl_ratio = idea.pipeline.implementation_ratio();
                format!(" {}% impl", (impl_ratio * 100.0) as u8)
            } else {
                format!(" {}% intake", pct)
            }
        } else {
            String::new()
        };

        // Package name header (shown when instructions distributed, progress >= 50)
        let package_line = if let Some(ref pkg) = idea.pipeline.package_name {
            let counts: Vec<String> = idea.pipeline.targets.iter()
                .map(|t| {
                    let remaining = t.instruction_count.saturating_sub(t.implemented_count);
                    if t.implemented_count > 0 {
                        format!("{}:{} remaining ({} done)", t.project, remaining, t.implemented_count)
                    } else {
                        format!("{}:{}", t.project, t.instruction_count)
                    }
                })
                .collect();
            format!("  ⟦{}⟧ → {}", pkg, counts.join(", "))
        } else {
            String::new()
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled(&idea.title, title_style),
                Span::styled(badge, Style::default().fg(Color::Rgb(r, g, b))),
            ]),
        ];
        if !package_line.is_empty() {
            lines.push(Line::styled(package_line, Style::default().fg(CYAN)));
        }
        lines.push(Line::styled(format!("  {}", idea.preview), Style::default().fg(TEXT_DIM)));

        // Inline audit trail expansion (toggled with 'i')
        if app.ideas_audit_expanded == Some(idx) {
            let idea_filename = idea.path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            // Load audit entries for orrchestrator project dir
            let project_dir = app.projects_dir.join("orrchestrator");
            let all_entries = orrch_core::load_audit_entries(&project_dir);
            let matching: Vec<_> = all_entries.iter()
                .filter(|e| e.source_file.contains(&idea_filename))
                .collect();

            lines.push(Line::raw(""));
            lines.push(Line::styled("── Audit Trail ──", Style::default().fg(TEXT_MUTED)));

            if matching.is_empty() {
                lines.push(Line::styled("  No audit records for this idea", Style::default().fg(TEXT_MUTED)));
            } else {
                for entry in &matching {
                    let raw_preview = if entry.raw_text.chars().count() > 80 {
                        format!("{}...", entry.raw_text.chars().take(80).collect::<String>())
                    } else {
                        entry.raw_text.clone()
                    };
                    let opt_preview = if entry.optimized_text.chars().count() > 80 {
                        format!("{}...", entry.optimized_text.chars().take(80).collect::<String>())
                    } else {
                        entry.optimized_text.clone()
                    };
                    let hash_short: String = entry.source_hash.chars().take(8).collect();
                    lines.push(Line::styled(
                        format!("  Source: {}", entry.source_file),
                        Style::default().fg(TEXT_DIM),
                    ));
                    lines.push(Line::styled(
                        format!("  Range: line {}–{}, chars {}–{}",
                            entry.coordinate.line_start, entry.coordinate.line_end,
                            entry.coordinate.char_start, entry.coordinate.char_end),
                        Style::default().fg(TEXT_DIM),
                    ));
                    lines.push(Line::styled(
                        format!("  Raw: {}", raw_preview),
                        Style::default().fg(TEXT_MUTED),
                    ));
                    lines.push(Line::styled(
                        format!("  Optimized: {}", opt_preview),
                        Style::default().fg(TEXT_MUTED),
                    ));
                    lines.push(Line::styled(
                        format!("  Hash: {}", hash_short),
                        Style::default().fg(TEXT_MUTED),
                    ));
                    lines.push(Line::raw(""));
                }
            }
            lines.push(Line::styled(
                "  Press 'i' or Esc to collapse",
                Style::default().fg(TEXT_DIM),
            ));
        }

        ListItem::new(lines)
    }).collect();

    // Split area: if open editors exist, carve out a bottom section for them
    let (list_area, editors_area) = if !app.split_off_editors.is_empty() {
        let editor_lines = (app.split_off_editors.len() + 3) as u16; // separator + entries + help line
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(editor_lines)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let title = format!(" Intentions ({}) — n=new s=submit X=retract Enter=edit ", app.ideas.len());
    let list = List::new(items)
        .scroll_padding(SCROLL_PAD)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(Style::default().bg(BG_HIGHLIGHT))
        .highlight_symbol("■ ");
    let mut state = ListState::default().with_selected(Some(app.idea_selected));
    frame.render_stateful_widget(list, list_area, &mut state);

    // Open Editors section (only rendered when split_off_editors is non-empty)
    if let Some(editors_rect) = editors_area {
        let mut editor_lines: Vec<Line> = Vec::new();
        editor_lines.push(Line::styled(
            "─── Open Editors ───",
            Style::default().fg(TEXT_MUTED),
        ));
        for name in &app.split_off_editors {
            editor_lines.push(Line::styled(
                format!("  ▸ {}", name),
                Style::default().fg(TEXT_MUTED),
            ));
        }
        editor_lines.push(Line::styled(
            "Jump to editor: Hypervise > orrch-edit",
            Style::default().fg(TEXT_DIM),
        ));
        frame.render_widget(
            Paragraph::new(editor_lines).style(Style::default()),
            editors_rect,
        );
    }
}

// ─── Intake Review Overlay ──────────────────────────────────────────

fn draw_intake_review(frame: &mut Frame, app: &App, area: Rect) {
    // Layout: banner (2 lines) + body (split 50/50 horizontal)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    // Banner
    let banner = Paragraph::new(Line::styled(
        " Intake Review Pending — y=confirm  e=edit  N=reject  Tab=switch pane ",
        Style::default().fg(WAITING_COLOR).add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(banner, chunks[0]);

    // Side-by-side panes
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    if let Some(review) = &app.intake_review {
        let raw_focused = app.intake_review_focus == IntakeReviewFocus::Raw;
        let opt_focused = app.intake_review_focus == IntakeReviewFocus::Optimized;

        // Raw pane (left, read-only)
        let raw_border = if raw_focused { Style::default().fg(ACCENT) } else { Style::default().fg(TEXT_MUTED) };
        let raw_block = Block::default()
            .title(" Raw (read-only) ")
            .borders(Borders::ALL)
            .border_style(raw_border);
        let raw_para = Paragraph::new(review.raw.as_str())
            .style(Style::default().fg(TEXT_DIM))
            .block(raw_block)
            .wrap(Wrap { trim: false })
            .scroll((app.intake_review_scroll_raw, 0));
        frame.render_widget(raw_para, panes[0]);

        // Optimized pane (right, editable)
        let opt_border = if opt_focused { Style::default().fg(ACCENT) } else { Style::default().fg(TEXT_MUTED) };
        let opt_block = Block::default()
            .title(" Optimized (e=edit) ")
            .borders(Borders::ALL)
            .border_style(opt_border);
        let opt_para = Paragraph::new(review.optimized.as_str())
            .style(Style::default().fg(TEXT))
            .block(opt_block)
            .wrap(Wrap { trim: false })
            .scroll((app.intake_review_scroll_opt, 0));
        frame.render_widget(opt_para, panes[1]);
    }
}

// ─── Projects Panel (Hot / Cold / Facilities) ────────────────────────

fn draw_projects(frame: &mut Frame, app: &App, area: Rect) {
    let mut items: Vec<ListItem> = Vec::new();

    // Helper: render a project as list item lines
    let render_project = |proj: &Project, idx: usize, app: &App| -> Vec<Line<'_>> {
        let session_count = app.active_session_count(&proj.path);
        let waiting = app.pm.sessions().iter()
            .filter(|s| s.project_dir == proj.path && s.state == SessionState::Waiting).count();
        let tag_color = match proj.color_tag {
            orrch_core::ColorTag::Red => Color::Red,
            orrch_core::ColorTag::Yellow => Color::Yellow,
            orrch_core::ColorTag::Green => Color::Green,
            orrch_core::ColorTag::None => TEXT_MUTED,
        };
        // OPT-001: prefer plan_phases counts when flat roadmap is absent
        let (done, total) = {
            let rd = proj.done_count();
            let rt = proj.roadmap.len();
            if rt > 0 {
                (rd, rt)
            } else {
                let pd: usize = proj.plan_phases.iter().map(|p| p.done_count()).sum();
                let pt: usize = proj.plan_phases.iter().map(|p| p.total_count()).sum();
                (pd, pt)
            }
        };
        // OPT-006: show "no plan" indicator for projects without PLAN.md
        let goals_str = if total > 0 {
            format!(" {done}/{total}")
        } else if !proj.has_plan {
            " [no plan]".to_string()
        } else {
            String::new()
        };
        let plan_str = String::new(); // covered by goals_str above
        let pipeline_count = app.pipelines_for_project(&proj.path).len();
        let max_sess = proj.max_sessions;
        let sess_str = if session_count > 0 {
            if pipeline_count > 1 {
                // Show pipeline count for parallel work
                if waiting > 0 { format!(" {pipeline_count}/{max_sess}⊞⚠") } else { format!(" {pipeline_count}/{max_sess}⊞") }
            } else {
                if waiting > 0 { format!(" {session_count}/{max_sess}⚠") } else { format!(" {session_count}/{max_sess}▶") }
            }
        } else { String::new() };
        // OPT-002: self-explanatory label for inbox queue count
        let queued_str = if proj.queued_prompts > 0 { format!(" Inbox:{}", proj.queued_prompts) } else { String::new() };

        // OPT-013: lifecycle badge color
        let lifecycle_color = match proj.lifecycle_stage {
            LifecycleStage::Active => GREEN,
            LifecycleStage::Maintenance => WAITING_COLOR,
            LifecycleStage::Archived => TEXT_MUTED,
            LifecycleStage::Deprecated => Color::Rgb(200, 80, 80),
        };
        // Only show badge for non-active stages to avoid clutter
        let lifecycle_span = if proj.lifecycle_stage != LifecycleStage::Active {
            Span::styled(format!(" [{}]", proj.lifecycle_stage.badge()), Style::default().fg(lifecycle_color))
        } else {
            Span::raw("")
        };

        let mut lines = vec![Line::from(vec![
            Span::styled(proj.color_tag.icon(), Style::default().fg(tag_color)),
            Span::styled(format!(" {}", proj.name), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" [{}]", proj.scope.badge()), Style::default().fg(CYAN)),
            lifecycle_span,
            Span::styled(goals_str, Style::default().fg(
                if done == total && total > 0 { GREEN }
                else if !proj.has_plan && total == 0 { TEXT_MUTED }
                else { TEXT_DIM }
            )),
            Span::styled(sess_str, Style::default().fg(if waiting > 0 { WAITING_COLOR } else { GREEN })),
            Span::styled(queued_str, Style::default().fg(WAITING_COLOR)),
            Span::styled(plan_str, Style::default().fg(WAITING_COLOR)),
            Span::styled(format!("  [{}]", proj.default_action()), Style::default().fg(TEXT_MUTED)),
            if proj.meta.apple_target { Span::styled(" 🍎", Style::default()) } else { Span::raw("") },
        ])];
        if !proj.description.is_empty() {
            let desc: String = proj.description.chars().take(60).collect();
            lines.push(Line::styled(format!("    {desc}"), Style::default().fg(TEXT_DIM)));
        }
        if let Some(next) = proj.next_priority() {
            lines.push(Line::from(vec![
                Span::styled("    → ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(next.title.clone(), Style::default().fg(TEXT)),
            ]));
        }
        // Only show expanded contents when cursor is inside this project
        let is_active = app.tree_browsing && app.tree_project == Some(idx);
        if is_active {
            // Count sessions for selection tracking
            let managed_sessions = app.sessions_for_project(&proj.path);
            let ext_sessions = app.external_sessions_for_project(&proj.path);
            let session_count = managed_sessions.len() + ext_sessions.len();
            let mut item_idx: usize = 0;

            // Sessions section (selectable)
            let _pipelines = app.pipelines_for_project(&proj.path);
            for s in &managed_sessions {
                let sc = match s.state {
                    SessionState::Working => GREEN, SessionState::Waiting => WAITING_COLOR,
                    SessionState::Idle => TEXT_MUTED, SessionState::Dead => Color::Red,
                };
                let sel = app.tree_selected == item_idx;
                let marker = if sel { "  ▶ " } else { "    " };
                let style = if sel { Style::default().fg(TEXT).bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) };
                lines.push(Line::from(vec![
                    Span::styled(format!("{marker}{} ", s.state.icon()), Style::default().fg(sc)),
                    Span::styled(s.goal_display().to_string(), style),
                    Span::styled(format!(" {}", s.backend.badge()), Style::default().fg(CYAN)),
                ]));
                item_idx += 1;
            }
            for ext in &ext_sessions {
                let sel = app.tree_selected == item_idx;
                let marker = if sel { "  ▶ " } else { "    " };
                let host_badge = ext.host_badge();
                let style = if sel { Style::default().fg(TEXT).bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) };
                lines.push(Line::from(vec![
                    Span::styled(format!("{marker}👁 "), Style::default().fg(CYAN)),
                    Span::styled(ext.display_name().to_string(), style),
                    if !host_badge.is_empty() {
                        Span::styled(format!(" {host_badge}"), Style::default().fg(Color::Rgb(180, 140, 255)))
                    } else {
                        Span::raw("")
                    },
                ]));
                item_idx += 1;
            }

            // Feedback processing sessions targeting this project
            for fb_item in &app.feedback_items {
                if fb_item.status == FeedbackStatus::Processing {
                    if let Some(ref session) = fb_item.tmux_session {
                        // Check if this session's routes include this project
                        let targets_this = fb_item.routes.iter().any(|r| r == &proj.name)
                            || fb_item.routes.is_empty(); // workspace-level targets all
                        if targets_this {
                            let live = orrch_core::tmux_session_status(session)
                                .unwrap_or_else(|| "processing...".into());
                            lines.push(Line::from(vec![
                                Span::styled("    ⏳ ", Style::default().fg(WAITING_COLOR)),
                                Span::styled(format!("feedback: {}", fb_item.preview.chars().take(25).collect::<String>()), Style::default().fg(WAITING_COLOR)),
                            ]));
                            lines.push(Line::styled(
                                format!("       └─ {live}"),
                                Style::default().fg(TEXT_MUTED),
                            ));
                        }
                    }
                }
            }

            // Separator between sessions and files
            if session_count > 0 {
                lines.push(Line::styled("    ────────────────────────", Style::default().fg(Color::Rgb(50, 50, 70))));
            }

            // Directory tree (selectable, with depth indentation)
            let tree_nodes = {
                let proj_path = proj.path.clone();
                let expanded_dirs = app.tree_expanded.get(&idx).cloned().unwrap_or_default();
                let mut nodes = Vec::new();
                build_tree_for_render(&proj_path, &proj_path, &expanded_dirs, 0, &mut nodes);
                nodes
            };
            for (ti, node) in tree_nodes.iter().enumerate() {
                let sel = app.tree_selected == session_count + ti;
                let indent = "    ".to_string() + &"  ".repeat(node.2);
                let arrow = if node.1 { if node.4 { "▾ " } else { "▸ " } } else { "  " };
                let sel_marker = if sel { "▶" } else { " " };
                let style = if sel {
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD).bg(BG_HIGHLIGHT)
                } else if node.1 {
                    Style::default().fg(CYAN)
                } else {
                    Style::default().fg(TEXT_DIM)
                };
                lines.push(Line::styled(
                    format!("{indent}{sel_marker}{arrow}{} {}", node.3, node.0),
                    style,
                ));
            }
        }
        lines
    };

    // Helper to build tree for rendering (non-method to avoid borrow issues)
    fn build_tree_for_render(
        dir: &std::path::Path,
        root: &std::path::Path,
        expanded: &std::collections::HashSet<std::path::PathBuf>,
        depth: usize,
        out: &mut Vec<(String, bool, usize, &'static str, bool)>, // (name, is_dir, depth, icon, expanded)
    ) {
        let entries = orrch_core::list_directory(dir);
        for entry in entries {
            let rel = entry.path.strip_prefix(root).unwrap_or(&entry.path).to_path_buf();
            let is_expanded = entry.is_dir && expanded.contains(&rel);
            out.push((entry.name.clone(), entry.is_dir, depth, entry.icon(), is_expanded));
            if is_expanded {
                build_tree_for_render(&entry.path, root, expanded, depth + 1, out);
            }
        }
    }

    // ── HOT section ──
    if !app.hot_indices.is_empty() {
        items.push(ListItem::new(Line::styled(
            "── HOT ─────────────────────────────────────",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
        for &idx in &app.hot_indices {
            if let Some(proj) = app.projects.get(idx) {
                items.push(ListItem::new(render_project(proj, idx, app)));
            }
        }
    }

    // ── COLD section ──
    if !app.cold_indices.is_empty() {
        items.push(ListItem::new(Line::styled(
            "── COLD ────────────────────────────────────",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        )));
        for &idx in &app.cold_indices {
            if let Some(proj) = app.projects.get(idx) {
                items.push(ListItem::new(render_project(proj, idx, app)));
            }
        }
    }

    // ── IGNORED section ──
    if !app.ignored_indices.is_empty() {
        items.push(ListItem::new(Line::styled(
            "── IGNORED ─────────────────────────────────",
            Style::default().fg(TEXT_MUTED),
        )));
        for &idx in &app.ignored_indices {
            if let Some(proj) = app.projects.get(idx) {
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(format!("  {} ", proj.name), Style::default().fg(TEXT_MUTED)),
                    if !proj.description.is_empty() {
                        Span::styled(proj.description.chars().take(40).collect::<String>(), Style::default().fg(TEXT_MUTED))
                    } else { Span::raw("") },
                ])));
            }
        }
    }

    // ── PRODUCTION section ──
    if !app.production_versions.is_empty() {
        items.push(ListItem::new(Line::styled(
            "── PRODUCTION ──────────────────────────────",
            Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        )));
        for v in &app.production_versions {
            let status_color = if v.working { GREEN } else { Color::Red };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(if v.working { "  🟢 " } else { "  🔴 " }, Style::default().fg(status_color)),
                Span::styled(&v.project_name, Style::default().fg(TEXT)),
                Span::styled(format!(" {}", v.version), Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
            ])));
        }
    }

    // ── FACILITIES section ──
    // NOTE: This section must be 1:1 with build_list_map() — every item pushed here
    // must correspond to exactly one entry in the map. No extra rows allowed.
    if !app.facilities.is_empty() || app.projects_dir.join("deprecated").is_dir() {
        items.push(ListItem::new(Line::styled(
            "── FACILITIES ──────────────────────────────",
            Style::default().fg(TEXT_MUTED).add_modifier(Modifier::BOLD),
        )));
        if app.projects_dir.join("deprecated").is_dir() {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  📦 ", Style::default().fg(TEXT_MUTED)),
                Span::styled("deprecated/", Style::default().fg(TEXT_DIM)),
            ])));
        }
        for facility in &app.facilities {
            items.push(ListItem::new(vec![
                Line::from(vec![
                    Span::styled("  📦 ", Style::default().fg(TEXT_DIM)),
                    Span::styled(&facility.name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("  ({} sub-projects)", facility.sub_projects.len()), Style::default().fg(TEXT_MUTED)),
                ]),
            ]));
            for sub in &facility.sub_projects {
                items.push(ListItem::new(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(&sub.name, Style::default().fg(TEXT_DIM)),
                    if !sub.description.is_empty() {
                        Span::styled(format!(" — {}", sub.description.chars().take(40).collect::<String>()), Style::default().fg(TEXT_MUTED))
                    } else {
                        Span::raw("")
                    },
                ])));
            }
        }
    }

    // Split: project list | preview pane (when tree browsing)
    if app.tree_browsing && !app.tree_preview.is_empty() {
        let hsplit = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        let list = List::new(items)
            .scroll_padding(SCROLL_PAD)
            .block(Block::default().title(projects_title(app)).borders(Borders::ALL).style(Style::default().fg(ACCENT)))
            .highlight_style(Style::default().bg(BG_HIGHLIGHT))
            .highlight_symbol("▶ ")
            .highlight_spacing(HighlightSpacing::Always);
        let mut state = ListState::default().with_selected(Some(app.project_selected));
        frame.render_stateful_widget(list, hsplit[0], &mut state);

        let preview = Paragraph::new(app.tree_preview.as_str())
            .style(Style::default().fg(TEXT))
            .block(Block::default().title(" Preview ").borders(Borders::ALL).style(Style::default().fg(TEXT_DIM)))
            .wrap(Wrap { trim: false });
        frame.render_widget(preview, hsplit[1]);
    } else {
        let list = List::new(items)
            .scroll_padding(SCROLL_PAD)
            .block(Block::default().title(projects_title(app)).borders(Borders::ALL).style(Style::default().fg(TEXT_DIM)))
            .highlight_style(Style::default().bg(BG_HIGHLIGHT))
            .highlight_symbol("▶ ")
            .highlight_spacing(HighlightSpacing::Always);
        let mut state = ListState::default().with_selected(Some(app.project_selected));
        frame.render_stateful_widget(list, area, &mut state);
    }
}

// ─── Production Panel ─────────────────────────────────────────────────

#[allow(dead_code)]
fn draw_production(frame: &mut Frame, app: &App, area: Rect) {
    if app.production_versions.is_empty() {
        let msg = Paragraph::new("No versioned releases found.\nProjects with v1/, v2/ directories appear here.")
            .style(Style::default().fg(TEXT_DIM))
            .block(Block::default().title(" Production ").borders(Borders::ALL));
        frame.render_widget(msg, area);
        return;
    }

    let rows: Vec<Row> = app.production_versions.iter().map(|v| {
        let color = if v.working { GREEN } else { Color::Red };
        Row::new(vec![
            Cell::from(if v.working { "🟢" } else { "🔴" }),
            Cell::from(v.project_name.as_str()).style(Style::default().fg(TEXT)),
            Cell::from(v.version.as_str()).style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Cell::from(v.path.display().to_string()).style(Style::default().fg(TEXT_DIM)),
        ])
    }).collect();

    let table = Table::new(rows, [
        Constraint::Length(3), Constraint::Length(18), Constraint::Length(6), Constraint::Min(20),
    ])
    .header(Row::new(vec!["", "Project", "Ver", "Path"]).style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
    .block(Block::default().title(" Production ").borders(Borders::ALL))
    .row_highlight_style(Style::default().bg(BG_HIGHLIGHT))
    .highlight_symbol("▶ ");

    let mut state = TableState::default().with_selected(Some(app.production_selected));
    frame.render_stateful_widget(table, area, &mut state);
}

// ─── Project Detail ───────────────────────────────────────────────────

fn draw_project_detail(frame: &mut Frame, app: &mut App, area: Rect, proj_idx: usize) {
    use crate::app::{DetailFocus, SectionCursor};
    let Some(proj) = app.projects.get(proj_idx) else { return; };
    let in_section_select = app.detail_focus == DetailFocus::SectionSelect;
    let in_sessions = app.detail_focus == DetailFocus::Sessions;
    let in_browser = app.detail_focus == DetailFocus::Browser;

    // Roadmap height: capped at 12 visible items (scrollable)
    let roadmap_height = proj.roadmap.len().min(12) as u16 + 3;
    let constraints = vec![
        Constraint::Length(2),              // header
        Constraint::Length(roadmap_height), // roadmap (scrollable)
        Constraint::Length(8),             // sessions (compact)
        Constraint::Min(5),                // file browser
    ];

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let browser_slot = 3;

    // Header — OPT-013: show lifecycle stage when not Active
    let lifecycle_detail = if proj.lifecycle_stage != LifecycleStage::Active {
        format!("  [{}]", proj.lifecycle_stage.label())
    } else {
        String::new()
    };
    // OPT-004: show nav hint in SectionSelect mode
    let nav_hint = if in_section_select {
        Span::styled("  ↑↓ section  → drill in", Style::default().fg(TEXT_MUTED))
    } else {
        Span::raw("")
    };
    // OPT-005: append logo path span when set
    let logo_span = if let Some(ref lp) = proj.logo_path {
        Span::styled(format!("  · logo:{lp}"), Style::default().fg(TEXT_MUTED))
    } else {
        Span::raw("")
    };
    let header = Paragraph::new(Line::from(vec![
        Span::styled(&proj.name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  [{}] {}/{} goals", proj.scope.badge(), proj.done_count(), proj.roadmap.len()), Style::default().fg(TEXT_DIM)),
        Span::styled(lifecycle_detail, Style::default().fg(Color::Rgb(200, 200, 100))),
        logo_span,
        nav_hint,
    ])).style(Style::default().bg(BG_DARK));
    frame.render_widget(header, layout[0]);

    // Roadmap — color-coded by feature status, scrollable via PgUp/PgDn
    let in_roadmap = app.detail_focus == crate::app::DetailFocus::Roadmap;
    // OPT-004: highlight section header when section_cursor points here in SectionSelect mode
    let roadmap_section_hover = in_section_select && app.section_cursor == SectionCursor::Roadmap;
    let scroll_offset = app.roadmap_scroll;
    let all_roadmap_items: Vec<ListItem> = proj.roadmap.iter().enumerate().map(|(i, item)| {
        let style = feature_status_style(item.status);
        let sel_prefix = if in_roadmap && i == app.roadmap_selected { "▸" } else { " " };
        ListItem::new(format!("{}{} {}", sel_prefix, item.status_icon(), item.title)).style(style)
    }).collect();
    // Slice to visible window
    let visible_roadmap: Vec<ListItem> = all_roadmap_items.into_iter().skip(scroll_offset).collect();
    let roadmap_border = if in_roadmap {
        Style::default().fg(ACCENT)
    } else if roadmap_section_hover {
        Style::default().fg(CYAN)
    } else {
        Style::default().fg(TEXT_DIM)
    };
    // OPT-003: show both up and down scroll indicators when content overflows.
    // Visible capacity = roadmap_height - 2 border rows.
    let visible_capacity = (roadmap_height as usize).saturating_sub(2).max(1);
    let items_below = proj.roadmap.len().saturating_sub(scroll_offset + visible_capacity);
    let scroll_hint = match (scroll_offset > 0, items_below > 0) {
        (true, true) => format!(" Roadmap ↑{scroll_offset} ↓{items_below} "),
        (true, false) => format!(" Roadmap ↑{scroll_offset} "),
        (false, true) => format!(" Roadmap ↓{items_below} "),
        (false, false) => " Roadmap ".to_string(),
    };
    let roadmap = List::new(visible_roadmap)
        .scroll_padding(SCROLL_PAD)
        .block(Block::default().title(scroll_hint).borders(Borders::ALL).style(roadmap_border));
    frame.render_widget(roadmap, layout[1]);

    // Sessions — selectable, shows managed + external, with duplicate-goal badges
    let proj_path = proj.path.clone();
    let pipelines = app.pipelines_for_project(&proj_path);
    let mut session_rows: Vec<(String, String, String, SessionState, String, String)> = app
        .sessions_for_project(&proj_path).iter()
        .map(|s| {
            let goal = s.goal_display().to_string();
            // Check if multiple sessions share this goal
            let dupes = pipelines.iter().find(|(g, _, _)| g == &goal).map(|(_, c, _)| *c).unwrap_or(0);
            let goal_display = if dupes > 1 { format!("{goal} ⚠ ×{dupes}") } else { goal };
            (s.state.icon().into(), s.sid.clone(), goal_display, s.state, s.uptime(), s.backend.badge().into())
        })
        .collect();
    for ext in app.external_sessions_for_project(&proj_path) {
        let host_tag = if ext.is_remote() {
            format!("[{}]", ext.host)
        } else {
            "[external]".into()
        };
        session_rows.push((
            "👁".into(),
            ext.display_name().to_string(),
            format!("pid:{}", ext.pid),
            SessionState::Working,
            String::new(),
            host_tag,
        ));
    }

    // OPT-004: highlight sessions section header when hovered in SectionSelect mode
    let sess_section_hover = in_section_select && app.section_cursor == SectionCursor::Sessions;
    let sess_border = if in_sessions {
        Style::default().fg(ACCENT)
    } else if sess_section_hover {
        Style::default().fg(CYAN)
    } else {
        Style::default().fg(TEXT_MUTED)
    };
    let sess_title = if in_sessions {
        " Sessions (Enter:brief  x:kill) "
    } else {
        " Sessions (Enter:brief  x:kill) "
    };
    if session_rows.is_empty() {
        let msg = Paragraph::new("  No sessions. Press 'n' to spawn.")
            .style(Style::default().fg(TEXT_MUTED))
            .block(Block::default().title(" Sessions (Enter:brief  x:kill  n:spawn) ").borders(Borders::ALL).style(sess_border));
        frame.render_widget(msg, layout[2]);
    } else {
        let max = session_rows.len().saturating_sub(1);
        if app.session_selected > max { app.session_selected = max; }
        let rows: Vec<Row> = session_rows.iter().map(|(icon, sid, goal, state, uptime, backend)| {
            let sc = match state {
                SessionState::Working => GREEN, SessionState::Waiting => WAITING_COLOR,
                SessionState::Idle => TEXT_MUTED, SessionState::Dead => Color::Red,
            };
            Row::new(vec![
                Cell::from(icon.as_str()), Cell::from(sid.as_str()),
                Cell::from(goal.as_str()).style(Style::default().fg(TEXT)),
                Cell::from(state.label()).style(Style::default().fg(sc)),
                Cell::from(uptime.as_str()), Cell::from(backend.as_str()).style(Style::default().fg(CYAN)),
            ])
        }).collect();
        let table = Table::new(rows, [
            Constraint::Length(3), Constraint::Length(8), Constraint::Min(15),
            Constraint::Length(8), Constraint::Length(8), Constraint::Length(10),
        ])
        .header(Row::new(vec!["", "ID", "Goal", "State", "Uptime", "Backend"]).style(Style::default().fg(ACCENT)))
        .block(Block::default().title(sess_title).borders(Borders::ALL).style(sess_border))
        .row_highlight_style(Style::default().bg(BG_HIGHLIGHT))
        .highlight_symbol("▶ ");
        let mut state = TableState::default().with_selected(if in_sessions { Some(app.session_selected) } else { None });
        frame.render_stateful_widget(table, layout[2], &mut state);

        // 90e: inline session brief overlay when expanded
        if in_sessions && app.session_detail_expanded {
            let pm_sessions = app.sessions_for_project(&proj_path);
            if let Some(sess) = pm_sessions.get(app.session_selected) {
                // Look up matching ManagedSession for cwd + last_output
                let managed_info = app.managed_sessions.iter()
                    .find(|ms| ms.name.contains(&sess.sid) || sess.sid.contains(&ms.name));
                let proj_dir_str = sess.project_dir.to_string_lossy().into_owned();
                let cwd = managed_info.map(|ms| ms.cwd.as_str()).unwrap_or(&proj_dir_str);
                let last_output = managed_info.map(|ms| ms.last_output.as_str()).unwrap_or("");
                let output_preview: String = last_output.lines()
                    .filter(|l| !l.trim().is_empty())
                    .take(2)
                    .collect::<Vec<_>>()
                    .join("\n");
                let brief_text = format!(
                    "Name:   {}\nCwd:    {}\nStatus: {}\n{}",
                    sess.sid,
                    cwd,
                    sess.state.label(),
                    if output_preview.is_empty() { String::new() } else { format!("Output: {}", output_preview) }
                );
                // Position the brief as a floating panel overlapping the browser area
                let brief_area = layout[browser_slot];
                let brief_height = 6u16;
                let brief = Rect {
                    x: brief_area.x,
                    y: brief_area.y,
                    width: brief_area.width,
                    height: brief_height.min(brief_area.height),
                };
                frame.render_widget(Clear, brief);
                frame.render_widget(
                    Paragraph::new(brief_text)
                        .style(Style::default().fg(TEXT))
                        .block(
                            Block::default()
                                .title(" Session Brief (Enter/Space to collapse) ")
                                .borders(Borders::ALL)
                                .style(Style::default().bg(Color::Rgb(10, 25, 40)).fg(CYAN)),
                        )
                        .wrap(Wrap { trim: false }),
                    brief,
                );
                return; // skip drawing file browser (brief overlays it)
            }
        }
    }


    // File browser — single tree column + preview pane
    // OPT-004: highlight browser section header when hovered in SectionSelect mode
    let browser_section_hover = in_section_select && app.section_cursor == SectionCursor::Browser;
    let browser_border = if in_browser {
        Style::default().fg(ACCENT)
    } else if browser_section_hover {
        Style::default().fg(CYAN)
    } else {
        Style::default().fg(TEXT_MUTED)
    };
    let unfocused_border = Style::default().fg(TEXT_MUTED);

    // Two-column split: tree (35%) | preview (65%)
    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(layout[3]);

    // Build tree items: current directory entries, with child entries expanded inline
    let mut tree_items: Vec<ListItem> = Vec::new();
    let _tree_index: usize = 0; // tracks which item is the "real" selected one
    let selected_entry = if app.browser_in_child {
        // In child: parent entry at parent_selected, then child entries
        // Selected is parent_selected + 1 + child_selected
        app.browser_parent_selected + 1 + app.browser_child_selected
    } else {
        app.browser_parent_selected
    };

    for (i, entry) in app.browser_parent_entries.iter().enumerate() {
        let style = if entry.is_dir { Style::default().fg(CYAN) } else { Style::default().fg(TEXT) };
        let expanded = i == app.browser_parent_selected && !app.browser_child_entries.is_empty() && entry.is_dir;
        let arrow = if entry.is_dir { if expanded { "▾ " } else { "▸ " } } else { "  " };
        tree_items.push(ListItem::new(format!("{}{} {}", arrow, entry.icon(), entry.name)).style(style));

        if expanded {
            // Show child entries indented
            for child in &app.browser_child_entries {
                let cs = if child.is_dir { Style::default().fg(CYAN) } else { Style::default().fg(TEXT_DIM) };
                let child_arrow = if child.is_dir { "▸ " } else { "  " };
                tree_items.push(ListItem::new(format!("    {}{} {}", child_arrow, child.icon(), child.name)).style(cs));
            }
        }
    }

    let rel_path = app.browser_path.strip_prefix(&app.browser_root).unwrap_or(&app.browser_path);
    let tree_title = if rel_path.as_os_str().is_empty() { " ./ ".to_string() } else { format!(" {}/ ", rel_path.display()) };

    let tree_list = List::new(tree_items)
        .scroll_padding(SCROLL_PAD)
        .block(Block::default().title(tree_title).borders(Borders::ALL).style(browser_border))
        .highlight_style(Style::default().bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    let sel = if in_browser { Some(selected_entry) } else { None };
    let mut tstate = ListState::default().with_selected(sel);
    frame.render_stateful_widget(tree_list, hsplit[0], &mut tstate);

    // Preview pane — file content or directory info
    if app.browser_preview.is_empty() {
        let empty = Paragraph::new("  Select a file to preview")
            .style(Style::default().fg(TEXT_MUTED))
            .block(Block::default().title(" Preview ").borders(Borders::ALL).style(unfocused_border));
        frame.render_widget(empty, hsplit[1]);
    } else {
        let preview = Paragraph::new(app.browser_preview.as_str())
            .style(Style::default().fg(TEXT))
            .block(Block::default().title(" Preview ").borders(Borders::ALL).style(unfocused_border))
            .wrap(Wrap { trim: false });
        frame.render_widget(preview, hsplit[1]);
    }
}

// ─── Dev Map ─────────────────────────────────────────────────────────

#[allow(dead_code)]
fn draw_dev_map(frame: &mut Frame, app: &mut App, area: Rect, proj_idx: usize, focused: bool) {
    

    let Some(proj) = app.projects.get(proj_idx) else { return; };
    let border_style = if focused { Style::default().fg(ACCENT) } else { Style::default().fg(TEXT_MUTED) };

    // Build flat list items from phases + expanded features
    let mut items: Vec<ListItem> = Vec::new();

    for (pi, phase) in proj.plan_phases.iter().enumerate() {
        let expanded = app.devmap_phase_idx == pi;
        let arrow = if expanded { "▾" } else { "▸" };
        let done = phase.done_count();
        let total = phase.total_count();
        let progress = if total > 0 {
            format!(" ({done}/{total})")
        } else {
            String::new()
        };

        // Phase header color: all done = green, some done = dim, none = text
        let phase_color = if done == total && total > 0 {
            GREEN
        } else if done > 0 {
            TEXT_DIM
        } else {
            TEXT
        };

        let phase_name = if let Some(num) = phase.number {
            format!("{arrow} Phase {num}: {}{progress}", phase.name)
        } else {
            format!("{arrow} {}{progress}", phase.name)
        };

        items.push(
            ListItem::new(Line::from(vec![
                Span::styled(phase_name, Style::default().fg(phase_color).add_modifier(Modifier::BOLD)),
            ]))
        );

        if expanded {
            for feat in &phase.features {
                let icon = feat.status.icon();
                let style = feature_status_style(feat.status);
                let color = style.fg.unwrap_or(TEXT);
                let id_str = feat.id.map(|n| format!("{n}. ")).unwrap_or_default();
                let title = format!("  {icon} {id_str}{}", feat.title);

                let mut spans: Vec<Span> = vec![Span::styled(title, Style::default().fg(color))];

                // Feature id used for diff/commit lookup — numeric id as string,
                // falling back to the title when no numeric id is present.
                let lookup_id = feat
                    .id
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| feat.title.clone());

                if feat.user_verified || feat.status == orrch_core::FeatureStatus::Verified {
                    spans.push(Span::styled(" ✓", Style::default().fg(GREEN)));
                }

                let diff_count = orrch_core::diff_log::load_diffs(&proj.path, &lookup_id).len();
                if diff_count > 0 {
                    spans.push(Span::styled(
                        format!(" +{diff_count}"),
                        Style::default().fg(CYAN),
                    ));
                }

                let commits =
                    orrch_core::git::commits_for_feature(&proj.path, &lookup_id);
                let commit_count = commits.len();
                if commit_count > 0 {
                    spans.push(Span::styled(
                        format!(" ●{commit_count}"),
                        Style::default().fg(TEXT_MUTED),
                    ));
                }

                // Build the multi-line ListItem: header line + up to 3 commit
                // child lines. Child lines are part of the same ListItem so they
                // don't affect the flat selection index used by devmap_item_at.
                let mut lines: Vec<Line> = vec![Line::from(spans)];
                if !commits.is_empty() {
                    // Reserve space for the indent ("    "), 7-char short sha,
                    // and a separating space. Subject gets whatever's left.
                    let max_subject = (area.width as usize)
                        .saturating_sub(2)  // list border padding
                        .saturating_sub(4)  // indent
                        .saturating_sub(8); // "abcdef1 "
                    for c in commits.iter().take(3) {
                        let short = c.sha.chars().take(7).collect::<String>();
                        let subject: String = if c.subject.chars().count() > max_subject {
                            let truncated: String =
                                c.subject.chars().take(max_subject.saturating_sub(1)).collect();
                            format!("{truncated}…")
                        } else {
                            c.subject.clone()
                        };
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("    {short} "),
                                Style::default().fg(TEXT_MUTED),
                            ),
                            Span::styled(subject, Style::default().fg(TEXT_DIM)),
                        ]));
                    }
                }

                items.push(ListItem::new(lines));
            }
        }
    }

    let total_done: usize = proj.plan_phases.iter().map(|p| p.done_count()).sum();
    let total_all: usize = proj.plan_phases.iter().map(|p| p.total_count()).sum();
    let block_title = format!(" Dev Map ({total_done}/{total_all}) ");

    let list = List::new(items)
        .scroll_padding(SCROLL_PAD)
        .block(Block::default().title(block_title).borders(Borders::ALL).style(border_style))
        .highlight_style(Style::default().bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    let sel = if focused { Some(app.devmap_selected) } else { None };
    let mut state = ListState::default().with_selected(sel);
    frame.render_stateful_widget(list, area, &mut state);
}

// ─── Session Focus ────────────────────────────────────────────────────

fn draw_session_focus(frame: &mut Frame, app: &App, area: Rect, idx: usize) {
    let data = {
        let sessions = app.pm.sessions();
        sessions.get(idx).map(|s| (
            s.display_name().to_string(), s.sid.clone(), s.backend.label().to_string(),
            s.goal_display().to_string(), String::from_utf8_lossy(&s.output_buffer).to_string(),
        ))
    };
    let Some((name, sid, backend, goal, text)) = data else {
        frame.render_widget(Paragraph::new("Session not found.").style(Style::default().fg(Color::Red)), area);
        return;
    };
    let layout = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let lines: Vec<&str> = text.lines().collect();
    let visible = layout[0].height as usize;
    let start = lines.len().saturating_sub(visible);
    let terminal = Paragraph::new(lines[start..].join("\n"))
        .style(Style::default().fg(TEXT).bg(Color::Rgb(16, 16, 30)));
    frame.render_widget(terminal, layout[0]);
    let bar_text = if goal == "(no goal)" { format!(" {name} [{sid}] ({backend}) — Esc") }
        else { format!(" {name} [{sid}] ({backend}) goal: {goal} — Esc") };
    frame.render_widget(Paragraph::new(bar_text).style(Style::default().fg(Color::White).bg(ACCENT)), layout[1]);
}

// ─── Editor ───────────────────────────────────────────────────────────

fn draw_external_session(frame: &mut Frame, app: &App, area: Rect, pid: u32) {
    let layout = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1), Constraint::Length(1)]).split(area);

    let session_name = orrch_core::session::read_session_name(pid);
    let display_name = if session_name.is_empty() { format!("pid:{pid}") } else { session_name };

    let lines: Vec<&str> = app.ext_log_cache.lines().collect();
    let total = lines.len();
    let visible = layout[1].height.saturating_sub(2) as usize;
    let max_scroll = total.saturating_sub(visible);
    let scroll = app.ext_log_scroll.min(max_scroll);
    let scroll_pct = if max_scroll > 0 { (scroll * 100) / max_scroll } else { 100 };

    let header = Paragraph::new(Line::from(vec![
        Span::styled("  👁 ", Style::default().fg(CYAN)),
        Span::styled(&display_name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  pid:{pid}"), Style::default().fg(TEXT_MUTED)),
        Span::styled(format!("  {scroll_pct}%  [{}/{total} lines]", scroll + visible.min(total)), Style::default().fg(TEXT_DIM)),
    ])).style(Style::default().bg(BG_DARK));
    frame.render_widget(header, layout[0]);

    let visible_text: String = lines.iter().skip(scroll).take(visible).copied().collect::<Vec<_>>().join("\n");
    let log_widget = Paragraph::new(visible_text)
        .style(Style::default().fg(TEXT).bg(Color::Rgb(16, 16, 30)))
        .block(Block::default().borders(Borders::ALL).style(Style::default().fg(TEXT_MUTED)))
        .wrap(Wrap { trim: false });
    frame.render_widget(log_widget, layout[1]);

    let bar = Paragraph::new(" j/k:scroll  Home/End:jump  r:refresh  Esc:back")
        .style(Style::default().fg(TEXT_DIM).bg(BG_DARK));
    frame.render_widget(bar, layout[2]);
}

// ─── Feedback Tab ────────────────────────────────────────────────────

// ─── Sessions Tab ────────────────────────────────────────────────────

// ─── Cockpit (Hypervise > Sessions) ────────────────────────────────────
//
// Layout is the "three-pane cockpit" recommended by the frontend-design
// advisory:
//
//   ┌─ workflow strip (3-4 rows, conditional) ──────────────────────────┐
//   ├─ triage strip (1-5 rows, conditional, only when WAIT/DEAD present)┤
//   ├─ ROSTER (fixed 42 cols) │ INSPECTOR (rest) ────────────────────────┤
//   │ ▶ ◆ ▅ │ session-name…    │ ▣ session-name  [WORKING]               │
//   │   ✕   │ another-name     │   /home/.../cwd/path                    │
//   │   · ▆ │ ...              │ ┌─ live pane ──────────────────────────┐│
//   │       │                  │ │ pane bytes from tmux capture-pane    ││
//   │       │                  │ └──────────────────────────────────────┘│
//   │       │                  │ ┌─ ✎ prompt (i to activate) ───────────┐│
//   │       │                  │ │ ...                                  ││
//   └───────┴──────────────────┴────────────────────────────────────────┘
//
// Geometry is sacred — rows never reflow. Selection drives the inspector
// instantly because the inspector is always rendered. The roster row
// encodes state in dedicated visual cells:
//   • cursor (▶ / space)
//   • demand glyph (◆ waiting, ✕ dead, · idle, space working)
//   • pulse glyph (animated block, 8 levels, blank when no output growth
//     for >2s) — see `pulse_char` and `update_session_pulses`.
//   • category tick (red/cyan/yellow bar)
//   • name (truncated)
//   • status badge (WORK/IDLE/WAIT/DEAD)

const PULSE_GLYPHS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const PULSE_FADE: std::time::Duration = std::time::Duration::from_secs(2);

/// Update the pulse tracker: drop entries for sessions that no longer
/// exist, advance each surviving session's tick, and record growth of
/// the `last_output` byte count.
fn update_session_pulses(
    map: &mut std::collections::HashMap<String, crate::app::SessionPulse>,
    sessions: &[orrch_core::windows::ManagedSession],
) {
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let alive: std::collections::HashSet<String> =
        sessions.iter().map(|s| s.name.clone()).collect();
    map.retain(|name, _| alive.contains(name));
    for s in sessions {
        let len = s.last_output.len();
        let entry = map
            .entry(s.name.clone())
            .or_insert_with(|| crate::app::SessionPulse {
                last_len: len,
                last_growth: now - Duration::from_secs(10),
                tick: 0,
            });
        if len > entry.last_len {
            entry.last_growth = now;
        }
        entry.last_len = len;
        entry.tick = entry.tick.wrapping_add(1);
    }
}

fn pulse_char(p: Option<&crate::app::SessionPulse>) -> char {
    use std::time::Instant;
    match p {
        Some(p) if Instant::now().duration_since(p.last_growth) <= PULSE_FADE => {
            PULSE_GLYPHS[(p.tick as usize) % PULSE_GLYPHS.len()]
        }
        _ => ' ',
    }
}

fn short_status(s: orrch_core::windows::SessionStatus) -> &'static str {
    use orrch_core::windows::SessionStatus;
    match s {
        SessionStatus::Working => "WORK",
        SessionStatus::Idle => "IDLE",
        SessionStatus::WaitingForInput => "WAIT",
        SessionStatus::Dead => "DEAD",
    }
}

fn truncate_visible(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let mut t: String = chars.into_iter().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

fn draw_sessions_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.session_log_view {
        draw_session_log_browser(frame, app, area);
        return;
    }
    use orrch_core::windows::SessionStatus;

    // Refresh and clamp selection.
    app.managed_sessions = orrch_core::windows::list_all_sessions();
    orrch_core::windows::cleanup_stale_sessions(&mut app.managed_sessions);
    update_session_pulses(&mut app.session_pulse, &app.managed_sessions);

    app.workflow_status = app
        .managed_sessions
        .iter()
        .filter(|s| matches!(s.status, SessionStatus::Working | SessionStatus::WaitingForInput))
        .find_map(|s| orrch_core::load_workflow_status(std::path::Path::new(&s.cwd)));

    let total = app.managed_sessions.len();
    if total > 0 && app.session_tab_selected >= total {
        app.session_tab_selected = total - 1;
    }

    // Triage strip: WaitingForInput + Dead sessions, capped at 5 rows.
    let triage_indices: Vec<usize> = app
        .managed_sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| matches!(s.status, SessionStatus::WaitingForInput | SessionStatus::Dead))
        .map(|(i, _)| i)
        .collect();
    let triage_count = triage_indices.len();
    let triage_h: u16 = if triage_count == 0 {
        0
    } else {
        // up to 5 rows + 2 (top + bottom border)
        (triage_count.min(5) as u16) + 2
    };

    // Workflow strip: header + up to 2 agent lines + spillover hint = 4 rows.
    let workflow_h: u16 = if app.workflow_status.is_some() { 4 } else { 0 };

    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(workflow_h),
            Constraint::Length(triage_h),
            Constraint::Min(5),
        ])
        .split(area);

    if workflow_h > 0 {
        draw_cockpit_workflow_strip(frame, v[0], app);
    }
    if triage_h > 0 {
        draw_cockpit_triage_strip(frame, v[1], app, &triage_indices);
    }

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(42), Constraint::Min(30)])
        .split(v[2]);

    draw_cockpit_roster(frame, body[0], app);
    draw_cockpit_inspector(frame, body[1], app);
}

fn draw_cockpit_workflow_strip(frame: &mut Frame, area: Rect, app: &App) {
    let Some(ws) = &app.workflow_status else {
        return;
    };
    let mut lines: Vec<Line> = Vec::new();
    let status_color = match ws.status.as_str() {
        "running" => CYAN,
        "paused" => WAITING_COLOR,
        "failed" => Color::Red,
        "complete" => GREEN,
        _ => TEXT_MUTED,
    };
    lines.push(Line::from(vec![
        Span::styled("▣ ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(&ws.workflow, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(" · step {}/{}", ws.step, ws.total_steps),
            Style::default().fg(TEXT_DIM),
        ),
        Span::styled(format!(" · {}", ws.status), Style::default().fg(status_color)),
    ]));
    let shown = ws.agents.len().min(2);
    for (i, agent) in ws.agents.iter().take(shown).enumerate() {
        let is_last = i + 1 == shown && ws.agents.len() <= 2;
        let connector = if is_last { "  └─ " } else { "  ├─ " };
        let agent_color = match agent.status.as_str() {
            "complete" | "running" => GREEN,
            "waiting" => WAITING_COLOR,
            "failed" => ACCENT,
            _ => TEXT_MUTED,
        };
        lines.push(Line::from(vec![
            Span::styled(connector, Style::default().fg(TEXT_DIM)),
            Span::styled(&agent.role, Style::default().fg(TEXT)),
            Span::styled(format!("  [{}]", agent.status), Style::default().fg(agent_color)),
        ]));
    }
    if ws.agents.len() > 2 {
        lines.push(Line::styled(
            format!("  + {} more agents", ws.agents.len() - 2),
            Style::default().fg(TEXT_MUTED),
        ));
    }
    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .style(Style::default().fg(TEXT_MUTED)),
    );
    frame.render_widget(p, area);
}

fn draw_cockpit_triage_strip(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    indices: &[usize],
) {
    use orrch_core::windows::SessionStatus;
    let mut lines: Vec<Line> = Vec::new();
    for &idx in indices.iter().take(5) {
        let Some(s) = app.managed_sessions.get(idx) else {
            continue;
        };
        let (glyph, color) = match s.status {
            SessionStatus::WaitingForInput => ('◆', WAITING_COLOR),
            SessionStatus::Dead => ('✕', Color::Red),
            _ => ('·', TEXT_DIM),
        };
        let last_line = s
            .last_output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .last()
            .unwrap_or("");
        let snippet: String = last_line.chars().take(80).collect();
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", glyph),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<28}", truncate_visible(&s.name, 28)),
                Style::default().fg(TEXT),
            ),
            Span::styled(
                format!("  {:<6}", short_status(s.status)),
                Style::default().fg(color),
            ),
            Span::styled(format!("  {}", snippet), Style::default().fg(TEXT_DIM)),
        ]));
    }
    let title = format!(" ⚠ Needs attention ({}) ", indices.len());
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(WAITING_COLOR).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .style(Style::default().fg(WAITING_COLOR));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_cockpit_roster(frame: &mut Frame, area: Rect, app: &App) {
    use orrch_core::windows::{SessionCategory, SessionStatus};
    let mut lines: Vec<Line> = Vec::new();
    for (idx, s) in app.managed_sessions.iter().enumerate() {
        let selected = idx == app.session_tab_selected;
        let cursor = if selected { "▶" } else { " " };
        let (demand, demand_color) = match s.status {
            SessionStatus::WaitingForInput => ('◆', WAITING_COLOR),
            SessionStatus::Dead => ('✕', Color::Red),
            _ => (' ', TEXT_MUTED),
        };
        let pulse = pulse_char(app.session_pulse.get(&s.name));
        let cat_color = match s.category {
            SessionCategory::Dev => ACCENT,
            SessionCategory::Edit => CYAN,
            SessionCategory::Proc => WAITING_COLOR,
        };
        let status_color = match s.status {
            SessionStatus::Working => CYAN,
            SessionStatus::Idle => TEXT_MUTED,
            SessionStatus::WaitingForInput => WAITING_COLOR,
            SessionStatus::Dead => Color::Red,
        };
        let name_style = if selected {
            Style::default()
                .fg(TEXT)
                .bg(BG_HIGHLIGHT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        let name_str = truncate_visible(&s.name, 22);
        lines.push(Line::from(vec![
            Span::styled(format!("{cursor} "), Style::default().fg(ACCENT)),
            Span::styled(
                format!("{demand} "),
                Style::default()
                    .fg(demand_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{pulse} "), Style::default().fg(GREEN)),
            Span::styled("│ ", Style::default().fg(cat_color)),
            Span::styled(format!("{:<22}", name_str), name_style),
            Span::styled(
                format!(" {:>4}", short_status(s.status)),
                Style::default().fg(status_color),
            ),
        ]));
    }
    if app.managed_sessions.is_empty() {
        lines.push(Line::styled(
            "  no sessions",
            Style::default().fg(TEXT_MUTED),
        ));
    }
    let title = format!(" Roster ({}) ", app.managed_sessions.len());
    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(ACCENT)))
        .borders(Borders::ALL)
        .style(Style::default().fg(TEXT_MUTED));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_cockpit_inspector(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(s) = app
        .managed_sessions
        .get(app.session_tab_selected)
        .cloned()
    else {
        let p = Paragraph::new("  no session selected")
            .style(Style::default().fg(TEXT_MUTED))
            .block(
                Block::default()
                    .title(" Inspector ")
                    .borders(Borders::ALL)
                    .style(Style::default().fg(TEXT_MUTED)),
            );
        frame.render_widget(p, area);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);

    // Header: name + status (line 1), cwd (line 2)
    let status_color = match s.status {
        orrch_core::windows::SessionStatus::Working => CYAN,
        orrch_core::windows::SessionStatus::Idle => TEXT_MUTED,
        orrch_core::windows::SessionStatus::WaitingForInput => WAITING_COLOR,
        orrch_core::windows::SessionStatus::Dead => Color::Red,
    };
    let cwd_display: String = if s.cwd.is_empty() {
        String::from("—")
    } else {
        let chars: Vec<char> = s.cwd.chars().collect();
        if chars.len() > 80 {
            let tail: String = chars.into_iter().rev().take(80).collect::<Vec<_>>().into_iter().rev().collect();
            format!("…{tail}")
        } else {
            s.cwd.clone()
        }
    };
    let header_lines = vec![
        Line::from(vec![
            Span::styled("▣ ", Style::default().fg(ACCENT)),
            Span::styled(&s.name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  [{}]", s.status.label()), Style::default().fg(status_color)),
            Span::styled("    i prompt · Enter expand · o open · x kill", Style::default().fg(TEXT_MUTED)),
        ]),
        Line::styled(format!("  {cwd_display}"), Style::default().fg(TEXT_DIM)),
    ];
    frame.render_widget(Paragraph::new(header_lines), chunks[0]);

    // Live pane
    let pane_inner_height = chunks[1].height.saturating_sub(2) as usize;
    let pane_lines = inspector_pane_lines(
        &mut app.inline_pane_cache,
        s.category,
        s.index,
        &s.name,
        pane_inner_height.max(1),
    );
    let pane_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(TEXT_MUTED));
    let pane = Paragraph::new(pane_lines)
        .block(pane_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(pane, chunks[1]);

    // Prompt input — always present so user knows it's there.
    let prompt_active = app.session_prompt_active;
    let prompt_text = if prompt_active {
        let mut shown = app.session_prompt_buffer.clone();
        let caret_byte = shown
            .char_indices()
            .nth(app.session_prompt_caret)
            .map(|(b, _)| b)
            .unwrap_or(shown.len());
        shown.insert(caret_byte, '█');
        shown
    } else {
        format!("press i to send a prompt to {}", s.name)
    };
    let prompt_text_style = if prompt_active {
        Style::default().fg(TEXT).bg(BG_HIGHLIGHT)
    } else {
        Style::default().fg(TEXT_DIM)
    };
    let prompt_title = if prompt_active {
        format!(" ✎ → {}  ·  Enter sends · Esc cancels ", s.name)
    } else {
        " ✎ Prompt ".to_string()
    };
    let prompt = Paragraph::new(prompt_text)
        .style(prompt_text_style)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(Span::styled(prompt_title, Style::default().fg(ACCENT)))
                .borders(Borders::ALL)
                .style(Style::default().fg(if prompt_active { ACCENT } else { TEXT_MUTED })),
        );
    frame.render_widget(prompt, chunks[2]);
}

/// Capture the bottom `height` lines of a session's tmux pane, with a
/// ~150ms TTL cache to avoid hammering tmux. Unlike `inline_pane_lines`,
/// the viewport size is supplied by the caller so the inspector can fill
/// whatever vertical space it has.
fn inspector_pane_lines(
    cache: &mut std::collections::HashMap<String, (std::time::Instant, String)>,
    cat: orrch_core::windows::SessionCategory,
    index: u32,
    name: &str,
    height: usize,
) -> Vec<Line<'static>> {
    use std::time::{Duration, Instant};
    const TTL: Duration = Duration::from_millis(150);
    const SCROLLBACK: u32 = 400;
    let now = Instant::now();
    let needs_refresh = cache
        .get(name)
        .map(|(t, _)| now.duration_since(*t) > TTL)
        .unwrap_or(true);
    if needs_refresh {
        let raw = orrch_core::windows::capture_pane_ansi(cat, index, SCROLLBACK);
        cache.insert(name.to_string(), (now, raw));
    }
    let raw = cache.get(name).map(|(_, c)| c.clone()).unwrap_or_default();
    let parsed = crate::ansi::parse(&raw);
    let total = parsed.len();
    let start = total.saturating_sub(height);
    parsed[start..total].to_vec()
}

/// Strip CSI / OSC ANSI escape sequences while preserving multi-byte
/// UTF-8 verbatim. The inline-expand preview text uses this; the focused
/// full-screen view uses the full ANSI parser instead.
#[allow(dead_code)] // kept as a fallback ANSI stripper for any future plain-text path.
fn strip_ansi_simple(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'[' => {
                    i += 2;
                    while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                        i += 1;
                    }
                    if i < bytes.len() { i += 1; }
                    continue;
                }
                b']' => {
                    i += 2;
                    while i < bytes.len() && bytes[i] != 0x07 && bytes[i] != 0x1b {
                        i += 1;
                    }
                    if i < bytes.len() && bytes[i] == 0x1b { i += 1; }
                    if i < bytes.len() { i += 1; }
                    continue;
                }
                _ => { i += 2; continue; }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    // Outside ESC sequences we copied bytes verbatim, so the result is
    // still a valid UTF-8 substring of the input.
    String::from_utf8(out).unwrap_or_default()
}

/// Render the focused single-session "IDE" view: live pane on top, an
/// always-available prompt input at the bottom. `Left Left` (within 500ms)
/// exits back to the Hypervise list; Enter sends the buffer to the
/// session via `tmux send-keys`.
///
/// The session is looked up by name, not index, so adding or removing
/// sessions while the view is open never displays the wrong pane.
fn draw_expanded_session(frame: &mut Frame, app: &mut App, area: Rect, name: &str) {
    use std::time::{Duration, Instant};
    use orrch_core::windows::capture_pane_ansi;

    const REFRESH_INTERVAL: Duration = Duration::from_millis(100);

    let session = app.managed_sessions.iter().find(|s| s.name == name).cloned();
    let Some(session) = session else {
        // Session vanished — drop back to the list cleanly.
        app.sub = crate::app::SubView::List;
        return;
    };

    let now = Instant::now();
    let needs_refresh = match app.expanded_pane_last_capture {
        Some(t) => now.duration_since(t) >= REFRESH_INTERVAL,
        None => true,
    };
    if needs_refresh {
        // Visible pane only (scrollback=0). Showing scrollback caused the
        // "wrong rows" complaint — pre-agent shell history bleeding in.
        app.expanded_pane_content = capture_pane_ansi(session.category, session.index, 0);
        app.expanded_pane_last_capture = Some(now);
    }

    // Layout: header strip (1 line) + pane (flex) + prompt input (3 lines).
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);

    // Header strip
    let header = Line::from(vec![
        Span::styled("▣ ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(&session.name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  [{}]", session.status.label()), Style::default().fg(match session.status {
            orrch_core::windows::SessionStatus::Working => CYAN,
            orrch_core::windows::SessionStatus::Idle => TEXT_MUTED,
            orrch_core::windows::SessionStatus::WaitingForInput => WAITING_COLOR,
            orrch_core::windows::SessionStatus::Dead => Color::Red,
        })),
        Span::styled("    ", Style::default()),
        Span::styled("← ← to exit · Enter sends · Esc clears", Style::default().fg(TEXT_DIM)),
    ]);
    frame.render_widget(Paragraph::new(header), chunks[0]);

    // Pane block
    let pane_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(TEXT_DIM));
    let pane_inner = pane_block.inner(chunks[1]);
    frame.render_widget(pane_block, chunks[1]);
    let lines = crate::ansi::parse(&app.expanded_pane_content);
    let pane = Paragraph::new(lines)
        .style(Style::default().bg(BG_DARK).fg(TEXT));
    frame.render_widget(pane, pane_inner);

    // Prompt input
    let mut shown = app.session_prompt_buffer.clone();
    let caret_byte = shown
        .char_indices()
        .nth(app.session_prompt_caret)
        .map(|(b, _)| b)
        .unwrap_or(shown.len());
    shown.insert(caret_byte, '█');
    let prompt_title = format!(" ✎ Send to {} — Enter to submit ", session.name);
    let prompt_block = Block::default()
        .title(Span::styled(prompt_title, Style::default().fg(ACCENT)))
        .borders(Borders::ALL)
        .style(Style::default().fg(TEXT_DIM));
    let prompt = Paragraph::new(Line::from(shown))
        .style(Style::default().fg(TEXT).bg(BG_HIGHLIGHT))
        .wrap(Wrap { trim: false })
        .block(prompt_block);
    frame.render_widget(prompt, chunks[2]);
}

fn draw_session_log_browser(frame: &mut Frame, app: &App, area: Rect) {
    if app.session_logs.is_empty() {
        let msg = Paragraph::new("No session logs found.\nSessions are logged to orrchestrator/.session-logs/ as they run.")
            .block(Block::default().title(" Session Logs — Esc=close ").borders(Borders::ALL).style(Style::default().fg(TEXT_DIM)));
        frame.render_widget(msg, area);
        return;
    }

    // Split: left = log list, right = head+tail viewer
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    // ── Left: log list ──
    let list_items: Vec<ListItem> = app.session_logs.iter().enumerate().map(|(i, log)| {
        let selected = i == app.session_logs_selected;
        let age = {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let secs = now.saturating_sub(log.started);
            if secs < 3600 { format!("{}m ago", secs / 60) }
            else if secs < 86400 { format!("{}h ago", secs / 3600) }
            else { format!("{}d ago", secs / 86400) }
        };
        let style = if selected {
            Style::default().fg(TEXT).bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_MUTED)
        };
        ListItem::new(Line::from(vec![
            Span::styled(if selected { " ▶ " } else { "   " }, Style::default().fg(ACCENT)),
            Span::styled(&log.name, style),
            Span::styled(format!("  {age}"), Style::default().fg(TEXT_DIM)),
        ]))
    }).collect();

    let list = List::new(list_items)
        .block(Block::default().title(format!(" Session Logs ({}) — Esc=close ↑↓=select ", app.session_logs.len())).borders(Borders::ALL).style(Style::default().fg(TEXT_DIM)));
    frame.render_widget(list, chunks[0]);

    // ── Right: head+tail viewer ──
    let Some(log) = app.session_logs.get(app.session_logs_selected) else { return; };
    let (head, tail) = orrch_core::windows::read_session_log_head_tail(&log.path, 50);

    let mut lines: Vec<Line> = Vec::new();

    // Header block
    lines.push(Line::styled(format!("Session:  {}", log.name), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)));
    lines.push(Line::styled(format!("Category: {}", log.category), Style::default().fg(TEXT_MUTED)));
    lines.push(Line::styled(format!("Attach:   {}", log.attach_cmd), Style::default().fg(CYAN)));
    lines.push(Line::styled(format!("Goal:     {}", log.goal), Style::default().fg(TEXT_DIM)));
    lines.push(Line::styled("─".repeat(60), Style::default().fg(TEXT_DIM)));

    // Head
    lines.push(Line::styled("── First 50 lines ─────────────────────────────────────────", Style::default().fg(ACCENT)));
    for l in &head {
        lines.push(Line::styled(l.clone(), Style::default().fg(TEXT)));
    }
    if head.is_empty() {
        lines.push(Line::styled("  (no output yet)", Style::default().fg(TEXT_DIM)));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled("── Last 50 lines ──────────────────────────────────────────", Style::default().fg(ACCENT)));
    for l in &tail {
        lines.push(Line::styled(l.clone(), Style::default().fg(TEXT)));
    }

    let total = lines.len();
    let scroll = app.session_log_scroll.min(total.saturating_sub(1)) as u16;
    let viewer = Paragraph::new(lines)
        .scroll((scroll, 0))
        .block(Block::default()
            .title(format!(" Log Viewer — PgUp/PgDn=scroll ({} lines) ", total))
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_DIM)));
    frame.render_widget(viewer, chunks[1]);
}

#[allow(dead_code)]
fn draw_feedback_tab(frame: &mut Frame, app: &App, area: Rect) {

    let drafts: Vec<(usize, &orrch_core::FeedbackItem)> = app.feedback_items.iter().enumerate()
        .filter(|(_, i)| i.status == FeedbackStatus::Draft).collect();
    let processing: Vec<(usize, &orrch_core::FeedbackItem)> = app.feedback_items.iter().enumerate()
        .filter(|(_, i)| i.status == FeedbackStatus::Processing || i.status == FeedbackStatus::Processed).collect();
    let routed: Vec<(usize, &orrch_core::FeedbackItem)> = app.feedback_items.iter().enumerate()
        .filter(|(_, i)| i.status == FeedbackStatus::Routed).collect();

    let pending_count = app.pending_editors.len();

    let mut lines: Vec<Line> = Vec::new();

    // Editing indicator
    if pending_count > 0 {
        lines.push(Line::styled(
            format!("  {pending_count} editor(s) open..."),
            Style::default().fg(WAITING_COLOR).add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::raw(""));
    }

    // Drafts section
    lines.push(Line::styled(
        format!("  DRAFTS ({})", drafts.len()),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    ));
    if drafts.is_empty() {
        lines.push(Line::styled("    No drafts — press f to write feedback", Style::default().fg(TEXT_MUTED)));
    }
    for (global_idx, item) in &drafts {
        let selected = *global_idx == app.feedback_selected;
        let marker = if selected { " > " } else { "   " };
        let time_display = if item.modified != item.created {
            format!("{} (edited {})", item.created, item.modified)
        } else {
            item.created.clone()
        };
        let style = if selected {
            Style::default().fg(TEXT).bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        let plan_badge = if item.feedback_type == orrch_core::FeedbackType::Plan { " 📋" } else { "" };
        if item.is_empty {
            lines.push(Line::styled(format!("{marker}{time_display}{plan_badge} — (empty)"), style));
        } else {
            lines.push(Line::styled(format!("{marker}{time_display}{plan_badge} — {}", item.preview), style));
        }
    }

    lines.push(Line::raw(""));

    // Processing section
    if !processing.is_empty() {
        lines.push(Line::styled(
            format!("  PROCESSING ({})", processing.len()),
            Style::default().fg(WAITING_COLOR).add_modifier(Modifier::BOLD),
        ));
        for (global_idx, item) in &processing {
            let selected = *global_idx == app.feedback_selected;
            let marker = if selected { " > " } else { "   " };

            if item.status == FeedbackStatus::Processed {
                // Done — ready to commit
                let style = if selected {
                    Style::default().fg(TEXT).bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(GREEN)
                };
                lines.push(Line::styled(
                    format!("{marker}✓ {} — {} [c to commit]", item.created, item.preview.chars().take(40).collect::<String>()),
                    style,
                ));
            } else {
                // Still processing — show file info
                let style = if selected {
                    Style::default().fg(TEXT).bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(WAITING_COLOR)
                };
                lines.push(Line::styled(
                    format!("{marker}⏳ {} — {}", item.created, item.preview.chars().take(40).collect::<String>()),
                    style,
                ));

                // Show live tmux session status underneath
                if let Some(ref session) = item.tmux_session {
                    let live_status = orrch_core::tmux_session_status(session)
                        .unwrap_or_else(|| "waiting...".into());
                    lines.push(Line::styled(
                        format!("      └─ {session}: {live_status}"),
                        Style::default().fg(TEXT_MUTED),
                    ));
                }
            }
        }
        lines.push(Line::raw(""));
    }

    // Routed section
    lines.push(Line::styled(
        format!("  ROUTED ({})", routed.len()),
        Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
    ));
    if routed.is_empty() {
        lines.push(Line::styled("    No routed feedback yet", Style::default().fg(TEXT_MUTED)));
    }
    for (global_idx, item) in &routed {
        let selected = *global_idx == app.feedback_selected;
        let marker = if selected { " > " } else { "   " };
        let style = if selected {
            Style::default().fg(TEXT).bg(BG_HIGHLIGHT)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        lines.push(Line::styled(format!("{marker}{} — {}", item.created, item.preview), style));
        // Show routing targets
        if !item.routes.is_empty() {
            let route_str = item.routes.iter()
                .map(|r| r.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled("-> ", Style::default().fg(CYAN)),
                Span::styled(route_str, Style::default().fg(CYAN)),
            ]));
        }
    }

    let widget = Paragraph::new(lines)
        .block(Block::default().title(" Feedback Pipeline ").borders(Borders::ALL)
            .style(Style::default().bg(BG_DARK).fg(TEXT)));
    frame.render_widget(widget, area);
}

fn draw_confirm_delete_feedback(frame: &mut Frame, app: &App, idx: usize) {
    let popup = centered_popup(frame.area(), 50, 6);
    frame.render_widget(Clear, popup);
    let preview = app.feedback_items.get(idx).map(|i| i.preview.as_str()).unwrap_or("?");
    let lines = vec![
        Line::styled("Delete this feedback?", Style::default().fg(WAITING_COLOR).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::styled(format!("\"{preview}\""), Style::default().fg(TEXT_DIM)),
        Line::styled("Y to confirm, any key to cancel", Style::default().fg(TEXT_MUTED)),
    ];
    frame.render_widget(Paragraph::new(lines).block(Block::default().title(" Delete ").borders(Borders::ALL)
        .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT))), popup);
}

// ─── Overlays ─────────────────────────────────────────────────────────

fn centered_popup(area: Rect, w: u16, h: u16) -> Rect {
    let width = w.min(area.width.saturating_sub(4));
    let height = h.min(area.height.saturating_sub(4));
    Rect::new((area.width - width) / 2, (area.height - height) / 2, width, height)
}

fn draw_spawn_goal(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 60, 16);
    frame.render_widget(Clear, popup);
    let proj_name = app.projects.get(app.spawn_project_idx).map(|p| p.name.as_str()).unwrap_or("?");
    let mut lines = vec![
        Line::from(vec![Span::raw("Project: "), Span::styled(proj_name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD))]),
        Line::raw(""),
        Line::styled("Goal (Enter=continue dev, Tab=roadmap):", Style::default().fg(TEXT_DIM)),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(ACCENT)),
            Span::styled(&app.spawn_goal_text, Style::default().fg(TEXT)),
            Span::styled(if app.spawn_goal_from_roadmap.is_none() { "█" } else { "" }, Style::default().fg(ACCENT)),
        ]),
    ];
    // Duplicate goal warning
    if let Some(proj) = app.projects.get(app.spawn_project_idx) {
        let check_goal = if app.spawn_goal_text.is_empty() { "continue development" } else { &app.spawn_goal_text };
        let dupes = app.duplicate_goal_count(&proj.path, check_goal);
        if dupes > 0 {
            lines.push(Line::styled(
                format!("  ⚠ {dupes} session(s) already working on this goal"),
                Style::default().fg(WAITING_COLOR),
            ));
        }

        let open = proj.open_roadmap_items();
        if !open.is_empty() {
            lines.push(Line::raw(""));
            for (i, item) in open.iter().enumerate() {
                let sel = app.spawn_goal_from_roadmap == Some(i);
                let marker = if sel { "■ " } else { "  " };
                // Show existing session count next to each roadmap item
                let existing = app.duplicate_goal_count(&proj.path, &item.title);
                let badge = if existing > 0 { format!(" ({existing}▶)") } else { String::new() };
                lines.push(Line::from(vec![
                    Span::styled(format!("{marker}{}", item.title),
                        if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) }),
                    Span::styled(badge, Style::default().fg(WAITING_COLOR)),
                ]));
            }
        }
    }
    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Goal (N=spawn all) ").borders(Borders::ALL).style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
        .wrap(Wrap { trim: false }), popup);
}

fn draw_spawn_workforce(frame: &mut Frame, app: &App) {
    let height = 7 + app.loaded_workforces.len() as u16;
    let popup = centered_popup(frame.area(), 60, height.min(16));
    frame.render_widget(Clear, popup);
    let goal_display = if app.spawn_goal_text.is_empty() { "continue development" } else { &app.spawn_goal_text };
    let mut lines = vec![
        Line::from(vec![Span::raw("Goal: "), Span::styled(goal_display, Style::default().fg(GREEN))]),
        Line::raw(""),
        Line::styled("Workforce (Tab/arrows to select, Enter to confirm):", Style::default().fg(TEXT_DIM)),
    ];

    // Option 0: no workforce (solo session)
    let no_wf_sel = app.spawn_workforce_idx == 0;
    lines.push(Line::styled(
        format!("{} (none) — solo session", if no_wf_sel { "▶" } else { " " }),
        if no_wf_sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) },
    ));

    // Workforce templates
    for (i, wf) in app.loaded_workforces.iter().enumerate() {
        let sel = app.spawn_workforce_idx == i + 1;
        let marker = if sel { "■ " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(format!("{}{}", marker, wf.name),
                if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) }),
            Span::styled(format!("  ({} agents)", wf.agents.len()), Style::default().fg(TEXT_MUTED)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Workforce ").borders(Borders::ALL).style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
        .wrap(Wrap { trim: false }), popup);
}

fn draw_workflow_picker(frame: &mut Frame, app: &App) {
    let height = 5 + app.workflow_choices.len() as u16;
    let popup = centered_popup(frame.area(), 50, height.min(14));
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::styled("Run Workflow (↑/↓ select, Enter to launch, Esc cancel)", Style::default().fg(TEXT_DIM)),
        Line::raw(""),
    ];

    for (i, (_script, display)) in app.workflow_choices.iter().enumerate() {
        let sel = i == app.workflow_picker_idx;
        let marker = if sel { "▶ " } else { "  " };
        lines.push(Line::styled(
            format!("{marker}{display}"),
            if sel {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            },
        ));
    }

    // Show the selected project name at the bottom
    if let Some(pidx) = app.selected_project_index() {
        if let Some(proj) = app.projects.get(pidx) {
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled("Project: ", Style::default().fg(TEXT_DIM)),
                Span::styled(&proj.name, Style::default().fg(GREEN)),
            ]));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default()
                .title(" Workflow ")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn draw_add_feature(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 55, 12);
    frame.render_widget(Clear, popup);

    let title_style = if app.add_feature_field == 0 {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(TEXT_DIM)
    };
    let desc_style = if app.add_feature_field == 1 {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(TEXT_DIM)
    };

    let cursor_title = if app.add_feature_field == 0 { "█" } else { "" };
    let cursor_desc = if app.add_feature_field == 1 { "█" } else { "" };

    let lines = vec![
        Line::styled("Add Feature (Tab=switch, Enter=add, Esc=cancel)", Style::default().fg(TEXT_DIM)),
        Line::raw(""),
        Line::styled("Title:", title_style.add_modifier(Modifier::BOLD)),
        Line::from(vec![
            Span::styled("> ", title_style),
            Span::styled(&app.add_feature_title, Style::default().fg(TEXT)),
            Span::styled(cursor_title, title_style),
        ]),
        Line::raw(""),
        Line::styled("Description:", desc_style.add_modifier(Modifier::BOLD)),
        Line::from(vec![
            Span::styled("> ", desc_style),
            Span::styled(&app.add_feature_desc, Style::default().fg(TEXT)),
            Span::styled(cursor_desc, desc_style),
        ]),
        Line::raw(""),
        Line::styled("Appends: N. [ ] **Title** — Description", Style::default().fg(TEXT_MUTED)),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default()
                .title(" Add Feature ")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn draw_add_mcp_server(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 60, 22);
    frame.render_widget(Clear, popup);

    let field = app.add_mcp_field;
    let cursor = |idx: usize| -> &'static str { if field == idx { "█" } else { "" } };
    let label_style = |idx: usize| -> Style {
        if field == idx {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_DIM)
        }
    };

    let transport_label = if app.add_mcp_transport == 0 { "stdio" } else { "sse" };
    let cmd_label = if app.add_mcp_transport == 0 { "Command:" } else { "URL:" };

    let mut lines = vec![
        Line::styled("Register MCP Server (Tab=next, Enter=save, Esc=cancel)", Style::default().fg(TEXT_DIM)),
        Line::raw(""),
        Line::styled("Name:", label_style(0)),
        Line::from(vec![
            Span::styled("> ", label_style(0)),
            Span::styled(&app.add_mcp_name, Style::default().fg(TEXT)),
            Span::styled(cursor(0), label_style(0)),
        ]),
        Line::raw(""),
        Line::styled("Description:", label_style(1)),
        Line::from(vec![
            Span::styled("> ", label_style(1)),
            Span::styled(&app.add_mcp_desc, Style::default().fg(TEXT)),
            Span::styled(cursor(1), label_style(1)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Transport: ", label_style(2)),
            Span::styled(
                format!("[{transport_label}]"),
                if field == 2 {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(GREEN)
                },
            ),
            Span::styled(if field == 2 { "  (Enter/s/e to toggle)" } else { "" }, Style::default().fg(TEXT_MUTED)),
        ]),
        Line::raw(""),
        Line::styled(cmd_label, label_style(3)),
        Line::from(vec![
            Span::styled("> ", label_style(3)),
            Span::styled(&app.add_mcp_command, Style::default().fg(TEXT)),
            Span::styled(cursor(3), label_style(3)),
        ]),
    ];

    if app.add_mcp_transport == 0 {
        lines.push(Line::raw(""));
        lines.push(Line::styled("Args (space-separated):", label_style(4)));
        lines.push(Line::from(vec![
            Span::styled("> ", label_style(4)),
            Span::styled(&app.add_mcp_args, Style::default().fg(TEXT)),
            Span::styled(cursor(4), label_style(4)),
        ]));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled("Roles (comma-separated):", label_style(5)));
    lines.push(Line::from(vec![
        Span::styled("> ", label_style(5)),
        Span::styled(&app.add_mcp_roles, Style::default().fg(TEXT)),
        Span::styled(cursor(5), label_style(5)),
    ]));

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default()
                .title(" Register MCP Server ")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn draw_spawn_agent(frame: &mut Frame, app: &App) {
    let height = 8 + app.agent_profiles.len() as u16;
    let popup = centered_popup(frame.area(), 55, height.min(18));
    frame.render_widget(Clear, popup);
    let goal_display = if app.spawn_goal_text.is_empty() { "continue development" } else { &app.spawn_goal_text };
    let mut lines = vec![
        Line::from(vec![Span::raw("Goal: "), Span::styled(goal_display, Style::default().fg(GREEN))]),
        Line::raw(""),
        Line::styled("Agent profile (Tab/arrows to select, Enter to confirm):", Style::default().fg(TEXT_DIM)),
    ];

    // Option 0: no agent (direct session)
    let no_agent_sel = app.spawn_agent_idx == 0;
    lines.push(Line::styled(
        format!("{} (none) — direct session", if no_agent_sel { "▶" } else { " " }),
        if no_agent_sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) },
    ));

    // Agent profiles
    for (i, profile) in app.agent_profiles.iter().enumerate() {
        let sel = app.spawn_agent_idx == i + 1;
        let marker = if sel { "■ " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(format!("{}{}", marker, profile.name),
                if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) }),
            Span::styled(format!("  {}", profile.role), Style::default().fg(TEXT_MUTED)),
        ]));
    }

    if app.agent_profiles.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled("  No agent profiles found in agents/", Style::default().fg(TEXT_MUTED)));
    }

    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Agent ").borders(Borders::ALL).style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
        .wrap(Wrap { trim: false }), popup);
}

fn draw_spawn_backend(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 50, 10);
    frame.render_widget(Clear, popup);
    let avail = app.pm.backends.available();
    let mut lines = vec![
        Line::from(vec![Span::raw("Goal: "), Span::styled(
            if app.spawn_goal_text.is_empty() { "continue development" } else { &app.spawn_goal_text },
            Style::default().fg(GREEN))]),
        Line::raw(""),
        Line::styled("Backend (Tab to toggle):", Style::default().fg(TEXT_DIM)),
    ];
    for &backend in BackendKind::cli_backends() {
        let selected = app.spawn_backend == backend;
        let found = avail.contains(&backend);
        let marker = if selected { "▶" } else { " " };
        let suffix = if found { "" } else { " (not found)" };
        let label = match backend {
            BackendKind::Claude => "Claude",
            BackendKind::Codex => "Codex",
            BackendKind::Gemini => "Gemini",
            BackendKind::Crush => "Crush",
            BackendKind::OpenCode => "OpenCode",
            _ => backend.label(),
        };
        lines.push(Line::styled(
            format!("{marker} {label}{suffix}"),
            if selected { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) },
        ));
    }
    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Backend ").borders(Borders::ALL).style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT))), popup);
}

fn draw_spawn_engine(frame: &mut Frame, app: &App) {
    // ENG-006: engine (LLM endpoint) picker. Index 0 = resolver default (today's
    // behavior); 1+ = a valve-passing engine from the library. Cloud engines are
    // always shown; local engines are never valve-gated.
    let engines = app.selectable_engines();
    let height = (7 + engines.len() as u16).min(18);
    let popup = centered_popup(frame.area(), 60, height);
    frame.render_widget(Clear, popup);

    let goal_display = if app.spawn_goal_text.is_empty() { "continue development" } else { &app.spawn_goal_text };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", app.spawn_backend.label()), Style::default().fg(CYAN)),
            Span::styled(goal_display, Style::default().fg(GREEN)),
        ]),
        Line::raw(""),
        Line::styled("Engine (Tab/arrows to select):", Style::default().fg(TEXT_DIM)),
    ];

    // Index 0 — resolver default. Show which engine the precedence layers would
    // pick (agent role / project / global default), or "harness default".
    let default_sel = app.spawn_engine_idx == 0;
    let resolver_hint = app
        .resolved_default_engine_label()
        .unwrap_or_else(|| "harness default endpoint".to_string());
    lines.push(Line::styled(
        format!("{} (default — resolver: {})", if default_sel { "▶" } else { " " }, resolver_hint),
        if default_sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) },
    ));

    if engines.is_empty() {
        lines.push(Line::styled(
            "  (no selectable engines — open provider valves in Library)",
            Style::default().fg(TEXT_MUTED),
        ));
    }

    for (i, eng) in engines.iter().enumerate() {
        let sel = app.spawn_engine_idx == i + 1;
        let loc = match eng.location {
            orrch_library::EngineLocation::Cloud => "cloud",
            orrch_library::EngineLocation::Gateway => "gateway",
            orrch_library::EngineLocation::Local => "local",
        };
        lines.push(Line::styled(
            format!("{} {} ({} · {})", if sel { "▶" } else { " " }, eng.name, eng.provider, loc),
            if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) },
        ));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled("Enter: continue · Esc: cancel", Style::default().fg(TEXT_MUTED)));

    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Engine ").borders(Borders::ALL).style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT))), popup);
}

fn draw_spawn_host(frame: &mut Frame, app: &App) {
    let remote_hosts: Vec<&orrch_core::remote::RemoteHost> = app.remote_hosts.iter()
        .filter(|h| !h.is_local)
        .collect();
    let height = 6 + remote_hosts.len() as u16;
    let popup = centered_popup(frame.area(), 50, height.min(16));
    frame.render_widget(Clear, popup);

    let goal_display = if app.spawn_goal_text.is_empty() { "continue development" } else { &app.spawn_goal_text };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", app.spawn_backend.label()), Style::default().fg(CYAN)),
            Span::styled(goal_display, Style::default().fg(GREEN)),
        ]),
        Line::raw(""),
        Line::styled("Host (Tab/arrows to select):", Style::default().fg(TEXT_DIM)),
    ];

    // Local option
    let local_sel = app.spawn_host_idx == 0;
    let local_hostname = app.remote_hosts.iter().find(|h| h.is_local).map(|h| h.name.as_str()).unwrap_or("local");
    lines.push(Line::styled(
        format!("{} {} (local)", if local_sel { "▶" } else { " " }, local_hostname),
        if local_sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) },
    ));

    // Remote options
    for (i, host) in remote_hosts.iter().enumerate() {
        let sel = app.spawn_host_idx == i + 1;
        let status = if host.reachable {
            if let Some(caps) = &host.capabilities {
                format!(" ({}/{})", caps.os, caps.mux)
            } else {
                " (ssh)".to_string()
            }
        } else {
            " (unreachable)".to_string()
        };
        lines.push(Line::styled(
            format!("{} {}{status}", if sel { "▶" } else { " " }, host.name),
            if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) }
            else if host.reachable { Style::default().fg(TEXT_DIM) }
            else { Style::default().fg(TEXT_MUTED) },
        ));
    }

    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Host ").borders(Borders::ALL).style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT))), popup);
}

fn draw_routing_summary(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 55, 12);
    frame.render_widget(Clear, popup);
    let mut lines = vec![Line::styled("Feedback processed!", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)), Line::raw("")];
    if app.routing_result.is_empty() {
        lines.push(Line::styled("No project matches — saved to workspace instructions_inbox.md", Style::default().fg(TEXT_DIM)));
    } else {
        lines.push(Line::styled(format!("Routed to {} project(s):", app.routing_result.len()), Style::default().fg(TEXT)));
        for (name, _) in &app.routing_result {
            lines.push(Line::from(vec![Span::raw("  • "), Span::styled(name, Style::default().fg(ACCENT))]));
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("Enter: spawn continue-dev sessions", Style::default().fg(TEXT)));
    lines.push(Line::styled("Esc: back", Style::default().fg(TEXT_DIM)));
    frame.render_widget(Paragraph::new(lines).block(Block::default().title(" Routed ").borders(Borders::ALL)
        .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT))), popup);
}

fn draw_confirm_complete(frame: &mut Frame, app: &App, proj_idx: usize) {
    let popup = centered_popup(frame.area(), 55, 8);
    frame.render_widget(Clear, popup);
    let name = app.projects.get(proj_idx).map(|p| p.name.as_str()).unwrap_or("?");
    let lines = vec![
        Line::styled(format!("Mark {name} as complete?"), Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::styled("This packages the project into a v1/ directory.", Style::default().fg(TEXT)),
        Line::styled("The project will appear in the Production panel.", Style::default().fg(TEXT_DIM)),
        Line::styled("Development can continue on the versioned source.", Style::default().fg(TEXT_DIM)),
        Line::styled("Y to confirm, any key to cancel", Style::default().fg(TEXT_MUTED)),
    ];
    frame.render_widget(Paragraph::new(lines).block(Block::default().title(" Complete → v1 ").borders(Borders::ALL)
        .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT))), popup);
}

fn draw_confirm_deprecate(frame: &mut Frame, app: &App, proj_idx: usize) {
    let popup = centered_popup(frame.area(), 50, 7);
    frame.render_widget(Clear, popup);
    let name = app.projects.get(proj_idx).map(|p| p.name.as_str()).unwrap_or("?");
    let lines = vec![
        Line::styled(format!("Deprecate {name}?"), Style::default().fg(WAITING_COLOR).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::styled(format!("Moves {name}/ → deprecated/{name}/"), Style::default().fg(TEXT)),
        Line::styled("Kept as reference, not deleted.", Style::default().fg(TEXT_DIM)),
        Line::styled("Y to confirm, any key to cancel", Style::default().fg(TEXT_DIM)),
    ];
    frame.render_widget(Paragraph::new(lines).block(Block::default().title(" Deprecate ").borders(Borders::ALL)
        .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT))), popup);
}

// ─── Rename popup (94a/94b) ───────────────────────────────────────────

fn draw_rename_popup(frame: &mut Frame, app: &App, title: &str) {
    let popup = centered_popup(frame.area(), 50, 5);
    frame.render_widget(Clear, popup);
    let content = format!(
        "New name: {}_\n\nEnter=save  Esc=cancel",
        app.rename_buffer
    );
    frame.render_widget(
        Paragraph::new(content)
            .style(Style::default().fg(TEXT))
            .block(
                Block::default()
                    .title(format!(" {title} "))
                    .borders(Borders::ALL)
                    .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(ACCENT)),
            ),
        popup,
    );
}

// ─── Confirm rollback popup (108) ────────────────────────────────────

fn draw_confirm_rollback(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 55, 7);
    frame.render_widget(Clear, popup);
    let tag = &app.rename_buffer;
    let lines = vec![
        Line::styled(
            format!("Delete release tag '{tag}'?"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::styled(
            "This removes the tag locally (does not push --delete).",
            Style::default().fg(TEXT),
        ),
        Line::styled(
            "To remove from remote: git push origin :refs/tags/<tag>",
            Style::default().fg(TEXT_DIM),
        ),
        Line::styled("Y to confirm, any key to cancel", Style::default().fg(TEXT_MUTED)),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Rollback Release ")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Rgb(40, 10, 10)).fg(TEXT)),
        ),
        popup,
    );
}

// ─── Confirm kill session popup (90d) ────────────────────────────────

fn draw_confirm_kill_session(frame: &mut Frame, name: &str) {
    let popup = centered_popup(frame.area(), 50, 5);
    frame.render_widget(Clear, popup);
    let lines = vec![
        Line::styled(
            format!("Kill session '{name}'?"),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::styled("Y to confirm, n/Esc to cancel", Style::default().fg(TEXT_MUTED)),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Kill Session ")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Rgb(40, 20, 10)).fg(TEXT)),
        ),
        popup,
    );
}

// ─── Steer session input popup ───────────────────────────────────────

fn draw_steer_session_input(frame: &mut Frame, app: &App, session_idx: usize) {
    let session_name = app.managed_sessions.get(session_idx)
        .map(|s| s.name.as_str())
        .unwrap_or("session");
    let popup = centered_popup(frame.area(), 70, 5);
    frame.render_widget(Clear, popup);
    let cursor_buf = format!("{}_", app.steer_buf);
    let lines = vec![
        Line::styled(
            format!("Send to '{session_name}'"),
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::styled(cursor_buf, Style::default().fg(TEXT)),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Send Input ")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Rgb(10, 20, 40)).fg(TEXT)),
        ),
        popup,
    );
}

// ─── OPT-005: Set Logo Path Popup ─────────────────────────────────────

fn draw_set_logo_path(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 70, 7);
    frame.render_widget(Clear, popup);
    let cursor_buf = format!("{}_", app.logo_path_input);
    let lines = vec![
        Line::styled("Enter file path for project logo:", Style::default().fg(TEXT_DIM)),
        Line::raw(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(ACCENT)),
            Span::styled(&cursor_buf, Style::default().fg(TEXT)),
        ]),
        Line::raw(""),
        Line::styled("Leave blank + Enter to clear.  Esc = cancel.", Style::default().fg(TEXT_MUTED)),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Set Logo Path (Enter=save  Esc=cancel) ")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Rgb(10, 20, 40)).fg(TEXT)),
        ),
        popup,
    );
}

// ─── Status Bar ───────────────────────────────────────────────────────

fn draw_status_bar(frame: &mut Frame, app: &mut App, area: Rect) {
    if let Some((ref msg, when)) = app.last_notification {
        if when.elapsed().as_secs() < 5 {
            frame.render_widget(
                Paragraph::new(format!(" {msg}"))
                    .style(Style::default().fg(GREEN).bg(BG_DARK)),
                area,
            );
            return;
        }
    }
    let line = build_hint_line(app);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(BG_DARK)),
        area,
    );

    // Tiny WebUI presence indicator on the right edge — clickable, no URL.
    // The URL itself lives in the Esc menu now. The indicator's only job is
    // to communicate "the WebUI is running" + provide a click target.
    if app.webui_port.is_some() {
        let glyph = " ⬡ ";
        let glyph_width = unicode_display_width(glyph) as u16;
        let badge_area = Rect {
            x: area.x + area.width.saturating_sub(glyph_width),
            y: area.y,
            width: glyph_width.min(area.width),
            height: 1,
        };
        app.webui_badge_area = Some(badge_area);
        let badge = Paragraph::new(Line::from(Span::styled(
            glyph,
            Style::default()
                .fg(Color::Rgb(0x1a, 0x1a, 0x2e))
                .bg(Color::Rgb(0x4a, 0xaa, 0x99))
                .add_modifier(Modifier::BOLD),
        )));
        frame.render_widget(badge, badge_area);
    } else {
        app.webui_badge_area = None;
    }
}

/// Compute the on-screen display width of a string. Counts each Unicode
/// scalar as 1 cell — adequate for the glyphs we use (BMP, no CJK).
fn unicode_display_width(s: &str) -> usize {
    s.chars().count()
}

/// Build a styled hint line with highlighted keys grouped by function.
fn build_hint_line(app: &App) -> Line<'static> {
    match (&app.panel, &app.sub) {
        (Panel::Oversee, SubView::List) if app.tree_browsing => hint_line(&[
            ("←", "back/collapse"), ("→", "expand"), ("Enter", "open"),
            ("|", ""),
            ("n", "spawn"), ("x", "kill"), ("a", "actions"),
        ]),
        (Panel::Oversee, SubView::List) => {
            // OPT-007: show completion-specific actions when selected project is done
            let selected_complete = app.selected_project_index()
                .and_then(|i| app.projects.get(i))
                .map_or(false, |p| p.roadmap_complete());
            if selected_complete {
                hint_line(&[
                    ("→/Enter", "detail view"), ("n", "spawn"),
                    ("|", ""),
                    ("a", "submit feedback | construct packages"),
                    ("|", ""),
                    ("↑↓", "select"), ("q", "quit"),
                ])
            } else {
                hint_line(&[
                    ("→/Enter", "detail view"), ("n", "spawn"), ("a", "actions"),
                    ("l", "lifecycle"), ("V", "visibility"),
                    ("|", ""),
                    ("↑↓", "select"), ("q", "quit"),
                ])
            }
        },
        (Panel::Design, SubView::List) => {
            match app.design_sub {
                crate::app::DesignSub::Intentions => hint_line(&[
                    ("Enter", "edit"), ("n", "new"), ("s", "submit"), ("r", "rename"), ("R", "review"), ("d", "delete"), ("X", "retract"),
                    ("|", ""),
                    ("↑↓", "select"), ("Tab", "sub-panel"),
                ]),
                crate::app::DesignSub::Workforce => hint_line(&[
                    ("Enter", "edit"), ("n", "new"), ("N", "AI-create"), ("r", "rename"), ("d", "del"), ("R", "refresh"),
                    ("|", ""),
                    ("←→", "tabs"), ("Home/End", "jump"),
                ]),
                crate::app::DesignSub::Library => hint_line(&[
                    ("v", "valve"), ("e", "toggle"), ("r", "refresh"),
                    ("|", ""),
                    ("←→", "tabs"), ("PgUp/Dn", "scroll"), ("Home/End", "jump"),
                ]),
                crate::app::DesignSub::Plans => hint_line(&[
                    ("Enter", "expand"), ("v", "verify"), ("s/S", "cycle status"), ("d", "deprecate"),
                    ("|", ""),
                    ("k/j", "move"), ("e", "edit"), ("r", "refresh"),
                ]),
            }
        },
        (Panel::Analyze, SubView::List) => hint_line(&[
            ("←→", "panels"), ("Esc", "menu"),
        ]),
        (Panel::Publish, SubView::List) => hint_line(&[
            ("←→", "tabs"), ("v", "preview"), ("b", "build"), ("D", "rollback tag"), ("r", "refresh"), ("Esc", "menu"),
        ]),
        (Panel::Hypervise, SubView::List) => {
            if app.session_log_view {
                hint_line(&[
                    ("↑↓", "select"), ("PgUp/PgDn", "scroll log"), ("Esc", "close logs"),
                ])
            } else {
                let has_sessions = !app.managed_sessions.is_empty();
                if has_sessions {
                    hint_line(&[
                        ("→", "expand"), ("Enter", "focus"), ("↑↓", "scroll preview"), ("PgUp/PgDn", "fast"), ("p", "prompt"), ("o", "external window"), ("x", "kill"), ("R", "refresh"),
                    ])
                } else {
                    hint_line(&[
                        ("R", "refresh"), ("L", "logs"), ("Esc", "menu"),
                    ])
                }
            }
        }
        (_, SubView::ExpandedSession(_)) => hint_line(&[
            ("← ←", "back to list"), ("Enter", "send prompt"), ("Esc", "clear"),
        ]),
        (_, SubView::SteerSession(_)) => hint_line(&[
            ("Enter", "send"), ("Esc", "cancel"),
        ]),
        // Feedback hints are now part of the Design panel
        (_, SubView::ProjectDetail(_)) => hint_line(&[
            ("Enter", "open"), ("n", "spawn"), ("a", "actions"),
            ("|", ""),
            ("Tab", "cycle focus"), ("Esc", "back"),
        ]),
        (_, SubView::ExternalSessionView(_)) => hint_line(&[
            ("r", "refresh"), ("Esc", "back"),
        ]),
        (_, SubView::DeprecatedBrowser) => hint_line(&[
            ("←→", "navigate"), ("Enter", "open"), ("d", "delete"), ("Esc", "back"),
        ]),
        (_, SubView::AppMenu) => hint_line(&[
            ("↑↓", "select"), ("Enter", "run"), ("Esc", "close"),
        ]),
        (_, SubView::CommitReview(_)) if app.commit_typing_correction => hint_line(&[
            ("Enter", "send correction"), ("Esc", "cancel"),
        ]),
        (_, SubView::CommitReview(_)) => hint_line(&[
            ("y", "approve"), ("n", "correct"), ("d", "deny"), ("↑↓", "scroll"), ("Esc", "cancel"),
        ]),
        (_, SubView::CommitCorrecting(_)) => hint_line(&[
            ("Esc", "cancel correction"),
        ]),
        (_, SubView::ActionMenu) => hint_line(&[
            ("↑↓", "select"), ("Enter", "run"), ("a-z", "shortcut"), ("Esc", "cancel"),
        ]),
        (_, SubView::SessionFocus(_)) => hint_line(&[
            ("Esc", "back to project"),
        ]),
        _ => Line::raw(""),
    }
}

/// Render a hint line from (key, action) pairs. "|" creates a dim separator.
fn hint_line(hints: &[(&str, &str)]) -> Line<'static> {
    let key_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let action_style = Style::default().fg(TEXT_MUTED);
    let sep_style = Style::default().fg(Color::Rgb(60, 60, 80));

    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (key, action) in hints {
        if *key == "|" {
            spans.push(Span::styled(" │ ", sep_style));
        } else if action.is_empty() {
            spans.push(Span::styled(key.to_string(), key_style));
        } else {
            spans.push(Span::styled(key.to_string(), key_style));
            spans.push(Span::styled(format!(" {action}  ", ), action_style));
        }
    }
    Line::from(spans)
}

fn draw_confirm_delete_deprecated(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 50, 7);
    frame.render_widget(Clear, popup);
    let name = app.dep_parent_entries.get(app.dep_parent_selected)
        .map(|e| e.name.as_str()).unwrap_or("?");
    let lines = vec![
        Line::styled(format!("Permanently delete deprecated/{name}?"), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::styled("This cannot be undone.", Style::default().fg(TEXT_DIM)),
        Line::raw(""),
        Line::styled("y: delete forever    n/Esc: cancel", Style::default().fg(TEXT_DIM)),
    ];
    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Delete ").borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(40, 15, 15)).fg(TEXT))), popup);
}

// ─── Commit Review Overlay ───────────────────────────────────────────

fn draw_commit_review(frame: &mut Frame, app: &App) {
    let pkg_count = app.commit_packages.len();
    let height = (pkg_count as u16 * 6 + 10).min(frame.area().height.saturating_sub(4));
    let width = frame.area().width.saturating_sub(6).min(80);
    let popup = centered_popup(frame.area(), width, height);
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::styled(
            format!("Review Commit — {} package(s)", pkg_count),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
    ];

    if pkg_count == 0 {
        lines.push(Line::styled("  No pending instruction packages found.", Style::default().fg(TEXT_MUTED)));
        lines.push(Line::styled("  Claude may still be processing, or all entries were already committed.", Style::default().fg(TEXT_MUTED)));
    } else {
        for pkg in &app.commit_packages {
            lines.push(Line::from(vec![
                Span::styled("  ▸ ", Style::default().fg(ACCENT)),
                Span::styled(&pkg.project_name, Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            ]));
            for preview_line in pkg.entry_preview.lines().take(4) {
                let truncated: String = preview_line.chars().take(70).collect();
                lines.push(Line::styled(
                    format!("    {truncated}"),
                    Style::default().fg(TEXT_DIM),
                ));
            }
            lines.push(Line::raw(""));
        }
    }

    // Correction text input (if active)
    if app.commit_typing_correction {
        lines.push(Line::styled("  What needs to be corrected?", Style::default().fg(WAITING_COLOR)));
        lines.push(Line::from(vec![
            Span::styled("  > ", Style::default().fg(ACCENT)),
            Span::styled(&app.commit_correction_text, Style::default().fg(TEXT)),
            Span::styled("█", Style::default().fg(ACCENT)),
        ]));
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "  Enter: send correction    Esc: cancel",
            Style::default().fg(TEXT_DIM),
        ));
    } else {
        lines.push(Line::styled(
            "  y: approve    n: correct    d: deny + return to draft    Esc: cancel",
            Style::default().fg(TEXT_DIM),
        ));
    }

    // Apply scroll
    let visible_lines: Vec<Line> = lines.into_iter().skip(app.commit_scroll).collect();

    frame.render_widget(Paragraph::new(visible_lines)
        .block(Block::default().title(" Commit Review ").borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
        .wrap(Wrap { trim: false }), popup);
}

fn draw_commit_correcting(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 55, 8);
    frame.render_widget(Clear, popup);

    let session = app.commit_correction_session.as_deref().unwrap_or("?");
    let lines = vec![
        Line::styled("Correcting...", Style::default().fg(WAITING_COLOR).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::styled(format!("  Claude is revising packages ({session})"), Style::default().fg(TEXT_DIM)),
        Line::styled("  This overlay will refresh when done.", Style::default().fg(TEXT_DIM)),
        Line::raw(""),
        Line::styled("  Esc: cancel correction", Style::default().fg(TEXT_MUTED)),
    ];

    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Correcting ").borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT))), popup);
}

// ─── App Menu (Esc) ──────────────────────────────────────────────────

/// OPT-018: shrink a URL to fit `max` columns by replacing a middle slice
/// with `…`. Keeps the scheme/host head and the path/port tail visible so
/// the user can still recognize what they're looking at.
///
/// - Returns the input unchanged when it already fits.
/// - When `max <= 1`, returns just `…`.
/// - Char-aware (no panics on multi-byte input).
fn truncate_url(s: &str, max: usize) -> String {
    let len = s.chars().count();
    if len <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    // Reserve 1 col for the ellipsis. Split the remaining budget
    // into a head (slightly larger) and tail.
    let budget = max - 1;
    let head_len = (budget + 1) / 2;
    let tail_len = budget - head_len;
    let chars: Vec<char> = s.chars().collect();
    let head: String = chars[..head_len].iter().collect();
    let tail: String = chars[len - tail_len..].iter().collect();
    format!("{head}…{tail}")
}

fn draw_app_menu(frame: &mut Frame, app: &App) {
    let items = &[
        ("q", "Quit orrchestrator"),
        ("r", "Reload all projects"),
        ("g", "Git commit all projects"),
        ("v", "Version info"),
    ];

    // WebUI URL block — shown when the server is running. Local always
    // appears; the public TLS URL appears when TLS is configured, and the
    // plaintext public HTTP URL appears when the secondary HTTP listener
    // is enabled (e.g. `0.0.0.0:80` for `orrchestrator.com`).
    let local_url = app.webui_port.map(|p| format!("http://localhost:{p}"));
    let public_url = app.webui_public_url.clone();
    let public_http_url = app.webui_public_http_url.clone();
    // orrch-relay URL — only when the relay subsystem is enabled.
    let relay_url = (std::env::var("ORRCH_RELAY_ENABLE").as_deref() == Ok("1")).then(|| {
        let bind = std::env::var("ORRCH_RELAY_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8585".to_string());
        format!("http://{bind}/v1")
    });
    let mut url_lines = 0u16;
    if local_url.is_some() {
        url_lines += 2; // "WebUI:" header + local line
        if public_url.is_some() {
            url_lines += 1;
        }
        if public_http_url.is_some() {
            url_lines += 1;
        }
        url_lines += 1; // spacer
    }
    if relay_url.is_some() {
        url_lines += 3; // "Relay" header + url line + spacer
    }

    let popup = centered_popup(frame.area(), 56, (items.len() as u16) + 5 + url_lines);
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::styled("orrchestrator", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Line::styled("v0.1.0", Style::default().fg(TEXT_MUTED)),
        Line::raw(""),
    ];

    if let Some(local) = &local_url {
        // OPT-018: app-menu popup is 56 cols wide; subtract 2 for border and
        // ~9 chars for the inline label prefix (`  public `) leaves ~45 cols
        // for the URL itself. Truncate longer URLs with a horizontal ellipsis
        // so they never wrap or overflow the popup edge.
        const URL_MAX: usize = 45;
        lines.push(Line::styled(
            "WebUI",
            Style::default().fg(TEXT_DIM).add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::from(vec![
            Span::styled("  local  ", Style::default().fg(TEXT_DIM)),
            Span::styled(truncate_url(local, URL_MAX), Style::default().fg(ACCENT)),
        ]));
        if let Some(public) = &public_url {
            lines.push(Line::from(vec![
                Span::styled("  public ", Style::default().fg(TEXT_DIM)),
                Span::styled(truncate_url(public, URL_MAX), Style::default().fg(ACCENT)),
            ]));
        }
        if let Some(http_public) = &public_http_url {
            lines.push(Line::from(vec![
                Span::styled("  http   ", Style::default().fg(TEXT_DIM)),
                Span::styled(truncate_url(http_public, URL_MAX), Style::default().fg(ACCENT)),
            ]));
        }
        lines.push(Line::raw(""));
    }

    if let Some(relay) = &relay_url {
        const URL_MAX: usize = 45;
        lines.push(Line::styled(
            "Relay (OpenAI API)",
            Style::default().fg(TEXT_DIM).add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::from(vec![
            Span::styled("  local  ", Style::default().fg(TEXT_DIM)),
            Span::styled(truncate_url(relay, URL_MAX), Style::default().fg(ACCENT)),
        ]));
        lines.push(Line::raw(""));
    }

    for (i, (key, label)) in items.iter().enumerate() {
        let sel = i == app.app_menu_selected;
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", if sel { "▶" } else { " " }),
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                key.to_string(),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {label}"),
                if sel { Style::default().fg(TEXT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) },
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Menu ").borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT))), popup);
}

// ─── Action Menu ─────────────────────────────────────────────────────

fn draw_action_menu(frame: &mut Frame, app: &App) {
    // Reserve room for header (1 line) + "for <project>" subtitle (1 line if any) + spacer + items + borders.
    let project_label = app
        .selected_project_index()
        .and_then(|idx| app.projects.get(idx))
        .map(|p| p.name.clone());
    let subtitle_lines: u16 = if project_label.is_some() { 1 } else { 0 };
    let height = (app.action_items.len() as u16 + 4 + subtitle_lines).min(22);
    let popup = centered_popup(frame.area(), 50, height);
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::styled("Actions", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
    ];
    if let Some(ref name) = project_label {
        lines.push(Line::from(vec![
            Span::styled("for ", Style::default().fg(TEXT_DIM)),
            Span::styled(name, Style::default().fg(ACCENT)),
        ]));
    }
    lines.push(Line::raw(""));

    for (i, item) in app.action_items.iter().enumerate() {
        let sel = i == app.action_selected;
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", if sel { "▶" } else { " " }),
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                format!("{}", item.key),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", item.label),
                if sel { Style::default().fg(TEXT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) },
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Actions ").borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT))), popup);
}

// ─── New Project Wizard Overlays ─────────────────────────────────────

fn draw_new_project_name(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 55, 10);
    frame.render_widget(Clear, popup);
    let mut lines = vec![
        Line::styled("New Project", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::styled("Name (lowercase, hyphens ok):", Style::default().fg(TEXT_DIM)),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(ACCENT)),
            Span::styled(&app.new_project_name, Style::default().fg(TEXT)),
            Span::styled("█", Style::default().fg(ACCENT)),
        ]),
    ];
    if let Some(ref err) = app.new_project_error {
        lines.push(Line::raw(""));
        lines.push(Line::styled(format!("  ✗ {err}"), Style::default().fg(Color::Red)));
    }
    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" New Project ").borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
        .wrap(Wrap { trim: false }), popup);
}

/// VIS-001: build the Oversee panel block title, appending a `(N hidden)`
/// badge when any scopes are filtered out.
fn projects_title(app: &App) -> String {
    let n = app.hidden_project_count();
    if n == 0 {
        " Projects ".to_string()
    } else {
        format!(" Projects  ({n} hidden, V to toggle) ")
    }
}

/// VIS-001: render the scope-visibility toggle popup. Mirrors
/// `draw_new_project_scope` styling.
fn draw_scope_visibility(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 60, 12);
    frame.render_widget(Clear, popup);

    let scopes = orrch_core::Scope::ALL;

    let mut lines = vec![
        Line::styled(
            "Toggle scopes hidden from Oversee:",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
    ];
    for (i, scope) in scopes.iter().enumerate() {
        let sel = i == app.scope_visibility_selected;
        let hidden = app.hidden_scopes.contains(scope);
        let checkbox = if hidden { "[ ] " } else { "[x] " };
        let cursor = if sel { "▶ " } else { "  " };
        let label_style = if sel {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else if hidden {
            Style::default().fg(TEXT_MUTED)
        } else {
            Style::default().fg(TEXT)
        };
        lines.push(Line::from(vec![
            Span::styled(cursor, Style::default().fg(ACCENT)),
            Span::styled(checkbox, Style::default().fg(if hidden { TEXT_MUTED } else { GREEN })),
            Span::styled(scope.label(), label_style),
        ]));
    }
    lines.push(Line::raw(""));
    let n = app.hidden_project_count();
    let summary = if n == 0 {
        "all projects visible".to_string()
    } else {
        format!("{n} project(s) hidden")
    };
    lines.push(Line::styled(format!("  {summary}"), Style::default().fg(TEXT_DIM)));
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "  ↑↓ select   Enter/Space toggle   Esc close",
        Style::default().fg(TEXT_DIM),
    ));

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Project Visibility ")
                    .borders(Borders::ALL)
                    .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)),
            )
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn draw_new_project_scope(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 55, 14);
    frame.render_widget(Clear, popup);

    let scopes = [
        (orrch_core::Scope::Personal, "personal", "Full-size project, user-only"),
        (orrch_core::Scope::Private, "private", "Ship fast, iterate — no public API"),
        (orrch_core::Scope::Public, "public", "Readable by others — docs, tests, license"),
        (orrch_core::Scope::Commercial, "commercial", "Production-grade — full CI/CD, compliance"),
    ];

    let mut lines = vec![
        Line::from(vec![
            Span::raw("Project: "),
            Span::styled(&app.new_project_name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        ]),
        Line::raw(""),
        Line::styled("Scope (Tab/arrows to select):", Style::default().fg(TEXT_DIM)),
    ];
    for (scope, label, desc) in &scopes {
        let sel = app.new_project_scope == *scope;
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} {label}", if sel { "▶" } else { " " }),
                if sel { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT) },
            ),
            Span::styled(format!("  {desc}"), Style::default().fg(TEXT_MUTED)),
        ]));
    }
    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Scope ").borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
        .wrap(Wrap { trim: false }), popup);
}

fn draw_new_project_confirm(frame: &mut Frame, app: &App) {
    let popup = centered_popup(frame.area(), 55, 12);
    frame.render_widget(Clear, popup);
    let lines = vec![
        Line::styled("Create Project?", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Name:  ", Style::default().fg(TEXT_DIM)),
            Span::styled(&app.new_project_name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Scope: ", Style::default().fg(TEXT_DIM)),
            Span::styled(app.new_project_scope.label(), Style::default().fg(CYAN)),
        ]),
        Line::from(vec![
            Span::styled("  Temp:  ", Style::default().fg(TEXT_DIM)),
            Span::styled("hot", Style::default().fg(Color::Rgb(255, 130, 80))),
            Span::styled(" (starts actively tracked)", Style::default().fg(TEXT_MUTED)),
        ]),
        Line::raw(""),
        Line::styled("Will create:", Style::default().fg(TEXT_DIM)),
        Line::styled(format!("  ~/projects/{}/", app.new_project_name), Style::default().fg(TEXT)),
        Line::styled("  + CLAUDE.md, .scope, .orrtemp", Style::default().fg(TEXT_MUTED)),
        Line::raw(""),
        Line::styled("  y/Enter: create + spawn plan session    n/Esc: back", Style::default().fg(TEXT_DIM)),
    ];
    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Confirm ").borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
        .wrap(Wrap { trim: false }), popup);
}

// ─── Feedback Confirmation Overlay ───────────────────────────────────

fn draw_feedback_confirm(frame: &mut Frame, app: &App) {
    let route_count = app.confirm_routes.len();
    let preview_lines = 4;
    let height = (8 + route_count as u16 + preview_lines).min(30);
    let popup = centered_popup(frame.area(), 65, height);
    frame.render_widget(Clear, popup);

    let enabled_count = app.confirm_routes.iter().filter(|(_, _, e)| *e).count();

    let is_plan = app.confirm_feedback_type == orrch_core::FeedbackType::Plan;
    let title_text = if is_plan { "Submit Planning Document" } else { "Submit Feedback" };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(title_text, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            if is_plan {
                Span::styled("  📋 PLAN", Style::default().fg(Color::Rgb(255, 200, 50)).add_modifier(Modifier::BOLD))
            } else {
                Span::raw("")
            },
        ]),
        Line::raw(""),
    ];

    // Preview (first few lines of feedback)
    let preview: Vec<&str> = app.confirm_feedback_text.lines()
        .filter(|l| !l.trim().is_empty())
        .take(3)
        .collect();
    for p in &preview {
        let truncated: String = p.chars().take(58).collect();
        lines.push(Line::styled(format!("  │ {truncated}"), Style::default().fg(TEXT_DIM)));
    }
    if app.confirm_feedback_text.lines().count() > 3 {
        lines.push(Line::styled("  │ ...", Style::default().fg(TEXT_MUTED)));
    }
    lines.push(Line::raw(""));

    // Route targets (suggestions, not rules)
    lines.push(Line::styled(
        format!("Suggest routing ({enabled_count} hinted — Claude decides final):"),
        Style::default().fg(TEXT_DIM),
    ));
    for (i, (name, _, enabled)) in app.confirm_routes.iter().enumerate() {
        let sel = i == app.confirm_route_selected;
        let check = if *enabled { "☑" } else { "☐" };
        let marker = if sel { "▶" } else { " " };
        lines.push(Line::styled(
            format!(" {marker} {check} {name}"),
            if sel {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else if *enabled {
                Style::default().fg(TEXT)
            } else {
                Style::default().fg(TEXT_MUTED)
            },
        ));
    }

    lines.push(Line::raw(""));
    if is_plan {
        lines.push(Line::styled(
            "  📋 PLAN MODE — can create projects + trigger versioning",
            Style::default().fg(Color::Rgb(255, 200, 50)),
        ));
    } else {
        lines.push(Line::styled(
            "  Claude analyzes → optimizes → routes to final destinations",
            Style::default().fg(CYAN),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "  Enter: submit    p: toggle plan mode    Esc: cancel",
        Style::default().fg(TEXT_DIM),
    ));

    frame.render_widget(Paragraph::new(lines)
        .block(Block::default().title(" Confirm Feedback ").borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(TEXT)))
        .wrap(Wrap { trim: false }), popup);
}

use orrch_core::BackendKind;

#[cfg(test)]
mod ui_tests {
    use super::truncate_url;

    #[test]
    fn truncate_url_passthrough_when_fits() {
        assert_eq!(truncate_url("http://localhost:8484", 45), "http://localhost:8484");
    }

    #[test]
    fn truncate_url_elides_middle() {
        let long = "http://very-long-orrchestrator-host.example.com:8484/login";
        let out = truncate_url(long, 30);
        assert!(out.chars().count() <= 30, "got {} ({} chars)", out, out.chars().count());
        assert!(out.starts_with("http://"));
        assert!(out.contains('…'));
        // Tail (path) should still be present.
        assert!(out.ends_with("/login") || out.ends_with("login"));
    }

    #[test]
    fn truncate_url_handles_tiny_max() {
        assert_eq!(truncate_url("anything", 1), "…");
        assert_eq!(truncate_url("anything", 0), "…");
    }

    #[test]
    fn truncate_url_unicode_safe() {
        let s = "http://é.example.com/foo";
        let out = truncate_url(s, 10);
        assert!(out.chars().count() <= 10);
    }
}
