use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Row,
    Table, TableState, Tabs, Wrap,
};
use ratatui::Frame;

use orrch_core::{Project, SessionState, FeedbackStatus};
use crate::app::{App, Panel, SubView};

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
    }

    draw_status_bar(frame, app, layout[2]);
}

fn draw_panel_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.tab_focused;
    let titles: Vec<Line> = Panel::ALL.iter().map(|p| {
        let style = if *p == app.panel {
            if focused {
                // Tab bar is focused — bright accent with underline
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            }
        } else {
            Style::default().fg(TEXT_MUTED)
        };
        Line::styled(p.label(), style)
    }).collect();

    let bg = if focused { Color::Rgb(30, 30, 55) } else { BG_DARK };
    let tabs = Tabs::new(titles)
        .select(app.panel.index())
        .divider("│")
        .style(Style::default().bg(bg).fg(TEXT_MUTED))
        .highlight_style(if focused {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        });
    frame.render_widget(tabs, area);
}

fn draw_panel_content(frame: &mut Frame, app: &mut App, area: Rect) {
    match app.panel {
        Panel::Ideas => draw_ideas(frame, app, area),
        Panel::Projects => draw_projects(frame, app, area),
        Panel::Sessions => draw_sessions_tab(frame, app, area),
        Panel::Feedback => draw_feedback_tab(frame, app, area),
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

// ─── Ideas Panel ──────────────────────────────────────────────────────

fn draw_ideas(frame: &mut Frame, app: &App, area: Rect) {
    if app.ideas.is_empty() {
        let msg = Paragraph::new("No ideas yet. Press 'n' to create one.")
            .style(Style::default().fg(TEXT_DIM))
            .block(Block::default().title(" Ideas ").borders(Borders::ALL));
        frame.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = app.ideas.iter().map(|idea| {
        ListItem::new(vec![
            Line::styled(&idea.title, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Line::styled(format!("  {}", idea.preview), Style::default().fg(TEXT_DIM)),
        ])
    }).collect();

    let list = List::new(items)
        .scroll_padding(SCROLL_PAD)
        .block(Block::default().title(" Ideas ").borders(Borders::ALL))
        .highlight_style(Style::default().bg(BG_HIGHLIGHT))
        .highlight_symbol("▶ ");
    let mut state = ListState::default().with_selected(Some(app.idea_selected));
    frame.render_stateful_widget(list, area, &mut state);
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
        let goals_str = if total > 0 { format!(" {done}/{total}") } else { String::new() };
        let pipeline_count = app.pipelines_for_project(&proj.path).len();
        let sess_str = if session_count > 0 {
            if pipeline_count > 1 {
                // Show pipeline count for parallel work
                if waiting > 0 { format!(" {pipeline_count}⊞⚠") } else { format!(" {pipeline_count}⊞") }
            } else {
                if waiting > 0 { format!(" {session_count}⚠") } else { format!(" {session_count}▶") }
            }
        } else { String::new() };
        let queued_str = if proj.queued_prompts > 0 { format!(" Q:{}", proj.queued_prompts) } else { String::new() };

        let mut lines = vec![Line::from(vec![
            Span::styled(proj.color_tag.icon(), Style::default().fg(tag_color)),
            Span::styled(format!(" {}", proj.name), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" [{}]", proj.scope.badge()), Style::default().fg(CYAN)),
            Span::styled(goals_str, Style::default().fg(if done == total && total > 0 { GREEN } else { TEXT_DIM })),
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

    let roadmap_height = proj.roadmap.len().min(8) as u16 + 3;
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),          // header
            Constraint::Length(roadmap_height), // roadmap
            Constraint::Length(8),          // sessions (compact)
            Constraint::Min(5),            // file browser
        ])
        .split(area);

    // Header
    let header = Paragraph::new(Line::from(vec![
        Span::styled(&proj.name, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  [{}] {}/{} goals", proj.scope.badge(), proj.done_count(), proj.roadmap.len()), Style::default().fg(TEXT_DIM)),
    ])).style(Style::default().bg(BG_DARK));
    frame.render_widget(header, layout[0]);

    // Roadmap
    let roadmap_items: Vec<ListItem> = proj.roadmap.iter().map(|item| {
        let style = if item.done { Style::default().fg(TEXT_MUTED) } else { Style::default().fg(TEXT) };
        ListItem::new(format!("{} {}", item.status_icon(), item.title)).style(style)
    }).collect();
    let roadmap = List::new(roadmap_items)
        .scroll_padding(SCROLL_PAD)
        .block(Block::default().title(" Roadmap ").borders(Borders::ALL).style(Style::default().fg(TEXT_DIM)));
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
        .split(layout[3]);

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

            // Session row: marker icon name [status] cwd
            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(ACCENT)),
                Span::styled(format!("{} ", s.status.icon()), Style::default().fg(status_color)),
                Span::styled(&s.name, style),
                Span::styled(format!("  [{}]", s.status.label()), Style::default().fg(status_color)),
            ]));

            // Show last output line underneath
            if !s.last_output.is_empty() {
                lines.push(Line::styled(
                    format!("      {}", s.last_output),
                    Style::default().fg(TEXT_MUTED),
                ));
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

    let items: Vec<ListItem> = lines.into_iter().map(|l| ListItem::new(l)).collect();
    let list = List::new(items)
        .scroll_padding(SCROLL_PAD)
        .block(Block::default().title(" Sessions ").borders(Borders::ALL).style(Style::default().fg(TEXT_DIM)));
    frame.render_widget(list, area);
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
                let marker = if sel { "▶ " } else { "  " };
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
        let marker = if sel { "▶ " } else { "  " };
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
    let lines = vec![
        Line::from(vec![Span::raw("Goal: "), Span::styled(
            if app.spawn_goal_text.is_empty() { "continue development" } else { &app.spawn_goal_text },
            Style::default().fg(GREEN))]),
        Line::raw(""),
        Line::styled("Backend (Tab to toggle):", Style::default().fg(TEXT_DIM)),
        Line::styled(format!("{} Claude{}", if app.spawn_backend == BackendKind::Claude { "▶" } else { " " },
            if avail.contains(&BackendKind::Claude) { "" } else { " (not found)" }),
            if app.spawn_backend == BackendKind::Claude { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) }),
        Line::styled(format!("{} Gemini{}", if app.spawn_backend == BackendKind::Gemini { "▶" } else { " " },
            if avail.contains(&BackendKind::Gemini) { "" } else { " (not found)" }),
            if app.spawn_backend == BackendKind::Gemini { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(TEXT_DIM) }),
    ];
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
        lines.push(Line::styled("No project matches — saved to workspace fb2p.md", Style::default().fg(TEXT_DIM)));
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
        (Panel::Projects, SubView::List) if app.tree_browsing => hint_line(&[
            ("←", "back/collapse"), ("→", "expand"), ("Enter", "open"),
            ("|", ""),
            ("n", "spawn"), ("x", "kill"), ("a", "actions"),
        ]),
        (Panel::Projects, SubView::List) => hint_line(&[
            ("→", "enter project"), ("Enter", "detail view"), ("n", "spawn"), ("a", "actions"),
            ("|", ""),
            ("↑↓", "select"), ("q", "quit"),
        ]),
        (Panel::Ideas, SubView::List) => hint_line(&[
            ("Enter", "open"), ("n", "new"), ("d", "delete"),
            ("|", ""),
            ("←→", "panels"), ("Esc", "menu"),
        ]),
        (Panel::Sessions, SubView::List) => {
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
        (Panel::Feedback, SubView::List) => {
            // Dynamic hints based on selected item's status
            let selected_status = app.feedback_items.get(app.feedback_selected).map(|i| i.status);
            match selected_status {
                Some(FeedbackStatus::Draft) => hint_line(&[
                    ("s", "submit"), ("r", "resume"), ("p", "plan"), ("d", "delete"), ("f", "new"),
                ]),
                Some(FeedbackStatus::Processing) => hint_line(&[
                    ("u", "cancel+draft"),
                    ("|", ""),
                    ("f", "new"),
                ]),
                Some(FeedbackStatus::Processed) => hint_line(&[
                    ("c", "review+commit"), ("u", "recall to draft"),
                    ("|", ""),
                    ("f", "new"),
                ]),
                Some(FeedbackStatus::Routed) => hint_line(&[
                    ("u", "recall to draft"),
                    ("|", ""),
                    ("f", "new"),
                ]),
                None => hint_line(&[
                    ("f", "new feedback"),
                ]),
            }
        }
        (_, SubView::ProjectDetail(_)) => hint_line(&[
            ("Enter", "open"), ("n", "spawn"), ("a", "actions"),
            ("|", ""),
            ("Tab", "sess↔files"), ("Esc", "back"),
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
