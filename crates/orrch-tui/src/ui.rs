use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Row,
    Table, TableState, Wrap,
};
use ratatui::Frame;

use orrch_core::{Project, SessionState, FeedbackStatus};
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

    // Layout: panel tabs (1 line) + content + status bar (1 line)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(3), Constraint::Length(1)])
        .split(area);

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
    }

    draw_status_bar(frame, app, layout[2]);
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
    match app.panel {
        Panel::Design => draw_design(frame, app, area),
        Panel::Oversee => draw_projects(frame, app, area),
        Panel::Hypervise => draw_sessions_tab(frame, app, area),
        Panel::Analyze => draw_analyze(frame, app, area),
        Panel::Publish => draw_publish(frame, app, area),
    }
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

fn draw_placeholder(frame: &mut Frame, area: Rect, title: &str, message: &str) {
    let msg = Paragraph::new(message)
        .style(Style::default().fg(TEXT_DIM))
        .block(Block::default().title(format!(" {} ", title)).borders(Borders::ALL));
    frame.render_widget(msg, area);
}

fn draw_analyze(frame: &mut Frame, app: &App, area: Rect) {
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
        PublishTab::Compliance => draw_compliance_tab(frame, app, chunks[1]),
        PublishTab::Marketing => draw_marketing_tab(frame, app, chunks[1]),
        PublishTab::History => draw_history_tab(frame, app, chunks[1]),
    }
}

fn draw_packaging_tab(frame: &mut Frame, app: &App, area: Rect) {
    // Split horizontally: left=release notes, right=checklist+build targets
    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    // ── Release Notes (left) ────────────────────────────────────────────
    let notes_text = app.release_notes_preview.as_deref().unwrap_or(
        "Release notes not yet generated.\nNavigate to this tab to load.\n\n[v] preview next version changelog  [b] build artifacts",
    );
    let notes = Paragraph::new(notes_text)
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: false })
        .block(Block::default()
            .title(" Release Notes  [v]=preview version  [b]=build ")
            .borders(Borders::ALL)
            .style(Style::default().fg(TEXT_MUTED)));
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
            " {} ({}) — n=new N=AI Enter=edit d=del x=export i=import r=refresh{}",
            app.workforce_tab.label(), items_data.len(), scroll_hint,
        )
    } else {
        format!(
            " {} ({}) — n=new N=AI Enter=edit d=del r=refresh{}",
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
                LibrarySub::Agents => app.agent_profiles.len(),
                LibrarySub::Models => app.library_models.len(),
                LibrarySub::Harnesses => app.library_harnesses.len(),
                LibrarySub::McpServers => app.library_mcp_servers.len(),
                LibrarySub::Skills => app.library_skills.len(),
                LibrarySub::Tools => app.library_tools.len(),
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

    match app.library_sub {
        LibrarySub::Agents => draw_library_agents(frame, app, chunks[0], chunks[1]),
        LibrarySub::Models => draw_library_models(frame, app, chunks[0], chunks[1]),
        LibrarySub::Harnesses => draw_library_harnesses(frame, app, chunks[0], chunks[1]),
        LibrarySub::McpServers => draw_library_mcp(frame, app, chunks[0], chunks[1]),
        LibrarySub::Skills => draw_library_generic(frame, app, &app.library_skills, "Skills", chunks[0], chunks[1]),
        LibrarySub::Tools => draw_library_generic(frame, app, &app.library_tools, "Tools", chunks[0], chunks[1]),
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

fn draw_library_generic(frame: &mut Frame, app: &App, items_data: &[(String, std::path::PathBuf)], label: &str, list_area: Rect, preview_area: Rect) {
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
    let title = format!(" {} ({}) r=refresh{} ", label, items_data.len(), scroll_hint);
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

    let title = format!(" Intentions ({}) — n=new s=submit Enter=edit ", app.ideas.len());
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
        let done = proj.done_count();
        let total = proj.roadmap.len();
        // OPT-006: show "no plan" indicator for projects without PLAN.md
        let goals_str = if total > 0 {
            format!(" {done}/{total}")
        } else if !proj.has_plan {
            " [no plan]".to_string()
        } else {
            String::new()
        };
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
        let queued_str = if proj.queued_prompts > 0 { format!(" Q:{}", proj.queued_prompts) } else { String::new() };

        let mut lines = vec![Line::from(vec![
            Span::styled(proj.color_tag.icon(), Style::default().fg(tag_color)),
            Span::styled(format!(" {}", proj.name), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" [{}]", proj.scope.badge()), Style::default().fg(CYAN)),
            Span::styled(goals_str, Style::default().fg(
                if done == total && total > 0 { GREEN }
                else if !proj.has_plan && total == 0 { TEXT_MUTED }
                else { TEXT_DIM }
            )),
            Span::styled(sess_str, Style::default().fg(if waiting > 0 { WAITING_COLOR } else { GREEN })),
            Span::styled(queued_str, Style::default().fg(WAITING_COLOR)),
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
            let pipelines = app.pipelines_for_project(&proj.path);
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
            .block(Block::default().title(" Projects ").borders(Borders::ALL).style(Style::default().fg(ACCENT)))
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
            .block(Block::default().title(" Projects ").borders(Borders::ALL).style(Style::default().fg(TEXT_DIM)))
            .highlight_style(Style::default().bg(BG_HIGHLIGHT))
            .highlight_symbol("▶ ")
            .highlight_spacing(HighlightSpacing::Always);
        let mut state = ListState::default().with_selected(Some(app.project_selected));
        frame.render_stateful_widget(list, area, &mut state);
    }
}

// ─── Production Panel ─────────────────────────────────────────────────

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
    use crate::app::DetailFocus;
    let Some(proj) = app.projects.get(proj_idx) else { return; };
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

    // Header
    let header = Paragraph::new(Line::from(vec![
        Span::styled(&proj.name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  [{}] {}/{} goals", proj.scope.badge(), proj.done_count(), proj.roadmap.len()), Style::default().fg(TEXT_DIM)),
    ])).style(Style::default().bg(BG_DARK));
    frame.render_widget(header, layout[0]);

    // Roadmap — color-coded by feature status, scrollable via PgUp/PgDn
    let in_roadmap = app.detail_focus == crate::app::DetailFocus::Roadmap;
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
    } else {
        Style::default().fg(TEXT_DIM)
    };
    let scroll_hint = if scroll_offset > 0 { format!(" Roadmap ↑{scroll_offset} ") } else { " Roadmap ".to_string() };
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

    let sess_border = if in_sessions { Style::default().fg(ACCENT) } else { Style::default().fg(TEXT_MUTED) };
    if session_rows.is_empty() {
        let msg = Paragraph::new("  No sessions. Press 'n' to spawn.")
            .style(Style::default().fg(TEXT_MUTED))
            .block(Block::default().title(" Sessions (Enter:open  x:kill  n:spawn) ").borders(Borders::ALL).style(sess_border));
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
        .block(Block::default().title(" Sessions (Enter:open  x:kill) ").borders(Borders::ALL).style(sess_border))
        .row_highlight_style(Style::default().bg(BG_HIGHLIGHT))
        .highlight_symbol("▶ ");
        let mut state = TableState::default().with_selected(if in_sessions { Some(app.session_selected) } else { None });
        frame.render_stateful_widget(table, layout[2], &mut state);
    }

    // File browser — single tree column + preview pane
    let browser_border = if in_browser { Style::default().fg(ACCENT) } else { Style::default().fg(TEXT_MUTED) };
    let unfocused_border = Style::default().fg(TEXT_MUTED);

    // Two-column split: tree (35%) | preview (65%)
    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(layout[browser_slot]);

    // Build tree items: current directory entries, with child entries expanded inline
    let mut tree_items: Vec<ListItem> = Vec::new();
    let mut tree_index: usize = 0; // tracks which item is the "real" selected one
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

fn draw_dev_map(frame: &mut Frame, app: &mut App, area: Rect, proj_idx: usize, focused: bool) {
    use orrch_core::FeatureStatus;

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

fn draw_sessions_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    use orrch_core::windows::{SessionCategory, SessionStatus};

    // Refresh sessions on each render (fast — just reads tmux state)
    app.managed_sessions = orrch_core::windows::list_all_sessions();

    // Poll workflow status from active sessions' working directories
    app.workflow_status = app.managed_sessions.iter()
        .filter(|s| matches!(s.status, SessionStatus::Working | SessionStatus::WaitingForInput))
        .find_map(|s| orrch_core::load_workflow_status(std::path::Path::new(&s.cwd)));

    let mut lines: Vec<Line> = Vec::new();
    let mut flat_idx: usize = 0;

    for cat in SessionCategory::all() {
        let cat_sessions: Vec<&orrch_core::windows::ManagedSession> = app.managed_sessions.iter()
            .filter(|s| s.category == *cat)
            .collect();

        let cat_label = cat.label();
        let count = cat_sessions.len();
        lines.push(Line::styled(
            format!("── {} ({}) ─────────────────────────", cat_label, count),
            Style::default().fg(match cat {
                SessionCategory::Dev => ACCENT,
                SessionCategory::Edit => CYAN,
                SessionCategory::Proc => WAITING_COLOR,
            }).add_modifier(Modifier::BOLD),
        ));

        if cat_sessions.is_empty() {
            lines.push(Line::styled("    (none)", Style::default().fg(TEXT_MUTED)));
        }

        for s in &cat_sessions {
            let selected = flat_idx == app.session_tab_selected;
            let marker = if selected { " ▶ " } else { "   " };

            let status_color = match s.status {
                SessionStatus::Working => GREEN,
                SessionStatus::Idle => TEXT_MUTED,
                SessionStatus::WaitingForInput => WAITING_COLOR,
                SessionStatus::Dead => Color::Red,
            };

            let style = if selected {
                Style::default().fg(TEXT).bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };

            // Session row: marker icon name [status]
            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(ACCENT)),
                Span::styled(format!("{} ", s.status.icon()), Style::default().fg(status_color)),
                Span::styled(&s.name, style),
                Span::styled(format!("  [{}]", s.status.label()), Style::default().fg(status_color)),
                {
                    use orrch_core::session::device_class;
                    let dc = device_class(None);
                    Span::styled(
                        format!(" {}", dc.badge()),
                        Style::default().fg(match dc {
                            orrch_core::session::DeviceClass::Primary => CYAN,
                            orrch_core::session::DeviceClass::Compatibility => TEXT_MUTED,
                        }),
                    )
                },
            ]));

            // Show cwd (truncated)
            if !s.cwd.is_empty() {
                let cwd_display: String = s.cwd.chars().rev().take(60).collect::<String>()
                    .chars().rev().collect();
                let prefix = if s.cwd.len() > 60 { "…" } else { "" };
                lines.push(Line::styled(
                    format!("      {prefix}{cwd_display}"),
                    Style::default().fg(Color::Rgb(80, 80, 120)),
                ));
            }

            // Show up to 2 recent output lines underneath
            for output_line in s.last_output.lines().take(2) {
                if !output_line.trim().is_empty() {
                    lines.push(Line::styled(
                        format!("      {}", output_line),
                        Style::default().fg(TEXT_MUTED),
                    ));
                }
            }

            flat_idx += 1;
        }

        lines.push(Line::raw(""));
    }

    // Clamp selection
    let total = app.managed_sessions.len();
    if total > 0 && app.session_tab_selected >= total {
        app.session_tab_selected = total - 1;
    }

    // Decide whether to split for workflow tree
    let show_workflow = app.workflow_status.as_ref().is_some_and(|ws| {
        matches!(ws.status.as_str(), "running" | "paused" | "failed" | "complete")
    });

    let (session_area, workflow_area) = if show_workflow {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let items: Vec<ListItem> = lines.into_iter().map(|l| ListItem::new(l)).collect();
    let list = List::new(items)
        .scroll_padding(SCROLL_PAD)
        .block(Block::default().title(" Sessions ").borders(Borders::ALL).style(Style::default().fg(TEXT_DIM)));
    frame.render_widget(list, session_area);

    // Render workflow agent tree when a workflow is running
    if let (Some(wf_area), Some(ws)) = (workflow_area, &app.workflow_status) {
        let mut tree_lines: Vec<Line> = Vec::new();
        let agent_count = ws.agents.len();
        for (i, agent) in ws.agents.iter().enumerate() {
            let is_last = i == agent_count - 1;
            let connector = if is_last { "  └─ " } else { "  ├─ " };
            let status_color = match agent.status.as_str() {
                "complete" => GREEN,
                "running" => GREEN,
                "waiting" => WAITING_COLOR,
                "failed" => ACCENT,
                _ => TEXT_MUTED,
            };
            tree_lines.push(Line::from(vec![
                Span::styled(connector, Style::default().fg(TEXT_DIM)),
                Span::styled(&agent.role, Style::default().fg(TEXT)),
                Span::styled(format!("    [{}]", agent.status), Style::default().fg(status_color)),
            ]));
        }

        let title = format!(
            " Workflow: {} \u{2014} Step {}/{} ",
            ws.workflow, ws.step, ws.total_steps
        );
        let tree_block = Paragraph::new(tree_lines)
            .block(
                Block::default()
                    .title(Span::styled(&title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
                    .borders(Borders::ALL)
                    .style(Style::default().fg(TEXT_DIM)),
            );
        frame.render_widget(tree_block, wf_area);
    }
}

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

// ─── Status Bar ───────────────────────────────────────────────────────

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
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
                    ("|", ""),
                    ("↑↓", "select"), ("q", "quit"),
                ])
            }
        },
        (Panel::Design, SubView::List) => {
            match app.design_sub {
                crate::app::DesignSub::Intentions => hint_line(&[
                    ("Enter", "edit"), ("n", "new"), ("s", "submit"), ("d", "delete"),
                    ("|", ""),
                    ("↑↓", "select"), ("Tab", "sub-panel"),
                ]),
                crate::app::DesignSub::Workforce => hint_line(&[
                    ("Enter", "edit"), ("n", "new"), ("N", "AI-create"), ("d", "del"), ("r", "refresh"),
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
            ("←→", "tabs"), ("Esc", "menu"),
        ]),
        (Panel::Hypervise, SubView::List) => {
            let has_sessions = !app.managed_sessions.is_empty();
            if has_sessions {
                hint_line(&[
                    ("Enter", "focus"), ("m", "minimize"), ("x", "kill"), ("R", "refresh"),
                ])
            } else {
                hint_line(&[
                    ("R", "refresh"), ("Esc", "menu"),
                ])
            }
        }
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

fn draw_app_menu(frame: &mut Frame, app: &App) {
    let items = &[
        ("q", "Quit orrchestrator"),
        ("r", "Reload all projects"),
        ("g", "Git commit all projects"),
        ("v", "Version info"),
    ];

    let popup = centered_popup(frame.area(), 40, (items.len() as u16) + 5);
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::styled("orrchestrator", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Line::styled("v0.1.0", Style::default().fg(TEXT_MUTED)),
        Line::raw(""),
    ];

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
    let height = (app.action_items.len() as u16 + 4).min(20);
    let popup = centered_popup(frame.area(), 45, height);
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::styled("Actions", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Line::raw(""),
    ];

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
