use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use crossterm::event::{self, Event, KeyEventKind, MouseEventKind, EnableMouseCapture, DisableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use orrch_tui::App;
use orrch_tui::editor::{spawn_vim_window, PendingEditor};
use orrch_tui::ui;

/// Updates from the background remote discovery task.
enum RemoteUpdate {
    /// Host capability probes completed — update reachability + capabilities.
    Capabilities(Vec<orrch_core::remote::RemoteHost>),
    /// Periodic session discovery results.
    Sessions(Vec<orrch_core::ExternalSession>),
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(|| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/orrchestrator.log")
                .unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap())
        })
        .init();

    // --- CLI arg handling ---
    //
    // Minimal hand-rolled arg parser (no clap).
    //
    // Non-TUI entry points (PLAN items 37 / 39):
    //   --egui     — launch the native egui window scaffold (feature-gated)
    //   --webedit  — launch the local HTTP web node editor (PLAN item 37)
    //
    // Both modes are alternatives to the default TUI and deliberately avoid
    // the terminal-capability check below so terminal-averse users can run
    // orrchestrator as a windowed / browser-based app.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--egui") {
        return orrch_tui::launch_egui_window();
    }
    if args.iter().any(|a| a == "--webedit") {
        return run_webedit().await;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("orrchestrator — AI development pipeline hypervisor");
        println!();
        println!("USAGE:");
        println!("  orrchestrator            Launch the TUI (default)");
        println!("  orrchestrator --resume   Attach to a running orrchestrator tmux session");
        println!("  orrchestrator --web      Open the WebUI of the running instance in browser");
        println!("  orrchestrator --egui     Launch the native egui window (feature-gated)");
        println!("  orrchestrator --webedit  Launch the local HTTP web node editor");
        println!("  orrchestrator --help     Show this help");
        return Ok(());
    }

    // --resume: attach to the tmux session of a running orrchestrator instance.
    if args.iter().any(|a| a == "--resume") {
        return resume_session();
    }

    // --web: find a running instance's WebUI port and open it in the browser.
    if args.iter().any(|a| a == "--web") {
        return open_webui_in_browser();
    }

    if !io::stdout().is_terminal() {
        bail!(
            "orrchestrator requires a real terminal.\n\
             Run it directly in your terminal, not piped or inside another TUI.\n\
             Example: cargo run, or ./target/release/orrchestrator"
        );
    }

    // Restore terminal state on panic so the terminal isn't permanently locked
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        default_hook(info);
    }));

    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let _ = app.pm.discover_external().await;

    // Task 28: initial-clone flow for the library repo.
    // If a repo URL is configured and the library dir is missing/empty,
    // clone it now so the Library panel has content on first launch.
    app.library_clone_if_missing();

    // Advertise tmux session so `orrchestrator --resume` can attach to us.
    let session_file = std::path::PathBuf::from("/tmp/orrch-session");
    if let Ok(out) = std::process::Command::new("tmux")
        .args(["display-message", "-p", "#S"])
        .output()
    {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !name.is_empty() {
            let _ = std::fs::write(&session_file, &name);
        }
    }

    // Task 7: Start the WebUI companion server on an OS-assigned port.
    let pid = std::process::id();
    let port_file = std::path::PathBuf::from(format!("/tmp/orrch-webui-{pid}.port"));
    let webui = match orrch_webui::WebUiServer::start(0).await {
        Ok(srv) => {
            app.webui_port = Some(srv.port);
            tracing::info!("WebUI available at http://127.0.0.1:{}", srv.port);
            // Advertise port so `orrchestrator --web` can find this instance
            let _ = std::fs::write(&port_file, srv.port.to_string());
            Some(srv)
        }
        Err(e) => {
            tracing::warn!("WebUI failed to start: {e}");
            None
        }
    };

    let result = run_loop(&mut terminal, &mut app, webui).await;

    // Remove advertisement files
    let _ = std::fs::remove_file(&port_file);
    let _ = std::fs::remove_file(&session_file);

    // Restore terminal FIRST — before any cleanup that might hang
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture);
    let _ = terminal.show_cursor();

    // Clean up managed sessions (non-blocking)
    app.pm.cleanup();

    // Kill all managed tmux sessions and clear state records on clean exit
    orrch_core::windows::kill_all_managed_tmux_sessions();
    orrch_core::windows::clear_session_records();

    // Force exit — don't wait for background tokio tasks (remote discovery, timers)
    // They're all detached and will die with the process.
    if result.is_ok() {
        std::process::exit(0);
    }
    result
}

/// `--resume` entry point: attach to the tmux session of a running instance.
fn resume_session() -> Result<()> {
    let session_name = std::fs::read_to_string("/tmp/orrch-session")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    if session_name.is_empty() {
        eprintln!("No running orrchestrator session found.");
        eprintln!("Start orrchestrator first (it must be running inside tmux).");
        std::process::exit(1);
    }

    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new("tmux")
        .args(["attach-session", "-t", &session_name])
        .exec();
    bail!("tmux attach failed: {err}");
}

/// `--web` entry point: find a running instance's WebUI port and open it.
///
/// Reads /tmp/orrch-webui-*.port files written by running TUI instances.
/// If multiple files exist, picks the most recently modified one.
/// Prints the URL and opens it with xdg-open.
fn open_webui_in_browser() -> Result<()> {
    let mut candidates: Vec<(std::time::SystemTime, std::path::PathBuf, u16)> = std::fs::read_dir("/tmp")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if !name.starts_with("orrch-webui-") || !name.ends_with(".port") { return None; }
            let port_str = std::fs::read_to_string(&path).ok()?;
            let port: u16 = port_str.trim().parse().ok()?;
            let mtime = entry.metadata().ok()?.modified().ok()?;
            Some((mtime, path, port))
        })
        .collect();

    if candidates.is_empty() {
        eprintln!("No running orrchestrator instance found (no /tmp/orrch-webui-*.port files).");
        eprintln!("Start orrchestrator first, then run `orrchestrator --web`.");
        std::process::exit(1);
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let port = candidates[0].2;
    let url = format!("http://localhost:{port}");
    println!("Opening {url}");
    let _ = std::process::Command::new("xdg-open")
        .arg(&url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    Ok(())
}

/// Resolve the workforces directory for the web editor.
///
/// Mirrors the heuristic used by the egui window: prefer
/// `$ORRCH_WORKFORCES_DIR` if set, else `./workforces` relative to the
/// current working directory. The directory does not need to exist —
/// `orrch_webedit` will surface an empty list gracefully.
fn webedit_workforces_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("ORRCH_WORKFORCES_DIR") {
        return std::path::PathBuf::from(dir);
    }
    std::env::current_dir()
        .map(|p| p.join("workforces"))
        .unwrap_or_else(|_| std::path::PathBuf::from("workforces"))
}

/// `--webedit` entry point (PLAN item 37).
///
/// Launches the `orrch_webedit` HTTP server on an ephemeral port, prints
/// the URL to stdout, and blocks on Ctrl-C. Dropping the `ServerHandle`
/// at the end of the function signals the worker thread to stop and
/// joins it, giving a clean shutdown on the way out.
///
/// Unlike the TUI entry point, this function does NOT require a real
/// terminal — it is designed for headless / browser-only usage.
async fn run_webedit() -> Result<()> {
    let dir = webedit_workforces_dir();
    let handle = orrch_webedit::launch_webedit_server(dir.clone(), 0)
        .with_context(|| format!("launching webedit server on {}", dir.display()))?;

    println!("orrchestrator web editor");
    println!("  workforces dir: {}", dir.display());
    println!("  open {}", handle.url());
    println!("  press Ctrl-C to stop");

    // Park until Ctrl-C. `tokio::signal::ctrl_c` resolves once the signal
    // handler fires; the ServerHandle is dropped on return which stops the
    // worker thread.
    let _ = tokio::signal::ctrl_c().await;
    println!("shutting down web editor…");
    handle.shutdown();
    Ok(())
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    webui: Option<orrch_webui::WebUiServer>,
) -> Result<()> {
    let mut last_discovery = Instant::now();
    let mut last_feedback_reload = Instant::now();
    let mut last_retrospect = Instant::now();
    let mut last_workflow_poll = Instant::now();
    let mut last_intake_poll = Instant::now();
    let mut last_pipeline_sync = Instant::now();
    let mut last_webui_sync = Instant::now();

    // Background remote tasks — discovery + capability probes, never blocks render
    let (remote_tx, mut remote_rx) = mpsc::channel::<RemoteUpdate>(4);
    let hosts = app.remote_hosts.clone();
    tokio::spawn(async move {
        // Initial capability probe for all remote hosts
        let mut probed_hosts = hosts.clone();
        for host in probed_hosts.iter_mut() {
            if !host.is_local {
                orrch_core::remote::check_host_reachable(host).await;
            }
        }
        let _ = remote_tx.send(RemoteUpdate::Capabilities(probed_hosts.clone())).await;

        // Periodic discovery loop
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let mut all = Vec::new();
            for host in &probed_hosts {
                if !host.is_local {
                    let sessions = orrch_core::remote::discover_remote_sessions(host).await;
                    all.extend(sessions);
                }
            }
            if remote_tx.send(RemoteUpdate::Sessions(all)).await.is_err() {
                break;
            }
        }
    });

    loop {
        app.process_events();

        // Check for remote updates (non-blocking)
        while let Ok(update) = remote_rx.try_recv() {
            match update {
                RemoteUpdate::Capabilities(hosts) => {
                    // Merge reachability + capabilities into app's host list
                    for probed in &hosts {
                        if let Some(existing) = app.remote_hosts.iter_mut().find(|h| h.name == probed.name) {
                            existing.reachable = probed.reachable;
                            existing.capabilities = probed.capabilities.clone();
                        }
                    }
                }
                RemoteUpdate::Sessions(sessions) => {
                    app.remote_sessions = sessions;
                }
            }
        }

        // Local discovery every 5s — fast
        if last_discovery.elapsed() > Duration::from_secs(5) {
            let _ = app.pm.discover_external().await;
            app.categorize_projects();
            app.split_off_editors = orrch_core::windows::detect_split_off_editors("hub-edit");
            last_discovery = Instant::now();

            // Valve auto-reopen tick
            let reopened = app.valve_store.tick();
            for provider in &reopened {
                app.notify(format!("{} valve reopened", provider));
            }

            // IRM throttle check — auto-close valves for providers exceeding rate limits
            let throttled = app.usage_tracker.check_throttle();
            for (provider, reason, cooldown) in throttled {
                if !app.valve_store.is_blocked(&provider) {
                    app.valve_store.auto_close(&provider, &format!("IRM: {}", reason), cooldown);
                    app.notify(format!("{} auto-throttled: {}", provider, reason));
                }
            }
        }

        // Retrospect analysis every 10 minutes — generates troubleshooting protocols
        if last_retrospect.elapsed() > Duration::from_secs(600) {
            let projects_dir = app.projects_dir.clone();
            tokio::task::spawn_blocking(move || {
                let analysis = orrch_retrospect::analyze_ecosystem(&projects_dir);
                if analysis.total_errors_ecosystem > 0 {
                    orrch_retrospect::generate_protocols(&analysis, &projects_dir);
                }
            });
            last_retrospect = Instant::now();
        }

        // Intake review polling — every 3s, check for:
        //   1. pending review files in per-idea workspaces
        //   2. step-counter advances in workflow.json for in-progress intakes
        if last_intake_poll.elapsed() > Duration::from_secs(3) {
            let vault = orrch_core::vault::vault_dir(&app.projects_dir);

            // Sync intake-phase progress (1→49) for any submitted ideas
            // that haven't yet reached the user-confirmation gate.
            let mut any_changed = false;
            for idea in &app.ideas {
                if idea.pipeline.is_submitted() && idea.pipeline.progress < 50 {
                    if orrch_core::vault::sync_intake_progress(&vault, &idea.filename) {
                        any_changed = true;
                    }
                }
            }
            if any_changed {
                app.ideas = orrch_core::vault::load_ideas(&vault);
            }

            // Surface a pending review when one exists and none is loaded.
            if app.intake_review.is_none() {
                app.intake_review = orrch_core::load_intake_review(&vault, &app.projects);
            }

            last_intake_poll = Instant::now();
        }

        // Handle nvim request from app
        if let Some(req) = app.vim_request.take() {
            if let Some(child) = spawn_vim_window(&req.file, &req.title) {
                // New terminal window — TUI keeps running
                app.pending_editors.push(PendingEditor {
                    child,
                    file: req.file,
                    kind: req.kind,
                });
            } else {
                // Fallback: suspend TUI, run nvim in same terminal
                disable_raw_mode()?;
                execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                terminal.show_cursor()?;
                // Use the same orrchestrator-branded nvim args as the windowed path
                let vim_args = orrch_tui::editor::vim_title_args_pub(&req.title);
                let _ = std::process::Command::new("nvim")
                    .args(&vim_args)
                    .arg(&req.file)
                    .status();
                enable_raw_mode()?;
                execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
                terminal.clear()?;
                app.handle_vim_complete(&req.file, req.kind);
            }
        }

        // Check if any pending editors have finished
        app.check_pending_editors();

        // Re-read feedback files from disk every 2s while editors are open
        if !app.pending_editors.is_empty() && last_feedback_reload.elapsed() > Duration::from_secs(2) {
            app.reload_feedback();
            last_feedback_reload = Instant::now();
        }

        // Check for correction session completion (auto-refresh commit review)
        if let orrch_tui::SubView::CommitCorrecting(idx) = app.sub {
            if let Some(ref session) = app.commit_correction_session {
                let exists = std::process::Command::new("tmux")
                    .args(["has-session", "-t", session.as_str()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success());
                if !exists {
                    app.commit_correction_session = None;
                    app.open_commit_review(idx);
                    app.notify("Correction complete — review revised packages".into());
                }
            }
        }

        // Also auto-transition Processing → Processed periodically
        if last_feedback_reload.elapsed() > Duration::from_secs(5) {
            app.reload_feedback();
            last_feedback_reload = Instant::now();
        }

        // Sync pipeline progress every 10s — scans instruction inboxes for implemented markers
        if last_pipeline_sync.elapsed() > Duration::from_secs(10) {
            let vault = orrch_core::vault::vault_dir(&app.projects_dir);

            // Snapshot ideas that are mid-distribution (progress=50, targets empty)
            // so we can detect when distribution completes and kill the continuation session.
            let awaiting_distribution: Vec<String> = app.ideas.iter()
                .filter(|i| i.pipeline.progress == 50 && i.pipeline.targets.is_empty())
                .map(|i| i.filename.clone())
                .collect();

            let mut any_changed = false;
            for idea in &app.ideas {
                if idea.pipeline.is_submitted() && !idea.pipeline.is_complete() {
                    if orrch_core::vault::sync_pipeline_progress(&vault, &app.projects_dir, idea) {
                        any_changed = true;
                    }
                }
            }
            // Also sweep for intentions whose inboxes have fully cleared
            let swept = orrch_core::vault::refresh_implementation_from_inboxes(&app.projects_dir, &vault);
            if swept > 0 { any_changed = true; }
            if any_changed {
                app.ideas = orrch_core::vault::load_ideas(&vault);
            }

            // Kill intake continuation sessions whose distribution is now done
            // (targets populated since the snapshot above).
            for filename in &awaiting_distribution {
                let now_has_targets = app.ideas.iter()
                    .find(|i| &i.filename == filename)
                    .is_some_and(|i| !i.pipeline.targets.is_empty());
                if now_has_targets {
                    let stem = filename.trim_end_matches(".md");
                    let cont_name = format!("intake-cont-{stem}");
                    orrch_core::windows::kill_session(orrch_core::windows::SessionCategory::Dev, &cont_name);
                }
            }

            last_pipeline_sync = Instant::now();
        }

        // Task 27a: Periodic inbox maintenance — every 60s, run
        // `maintain_all_project_inboxes` on a blocking thread so it never
        // stalls the render loop. Failures are logged via tracing; successes
        // are silent. The max_bytes cap (64 KiB) matches the intake walker.
        if app.last_inbox_maintenance.elapsed() > Duration::from_secs(60) {
            let projects_dir = app.projects_dir.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) =
                    orrch_core::feedback::maintain_all_project_inboxes(&projects_dir, 65_536)
                {
                    tracing::warn!("maintain_all_project_inboxes failed: {}", e);
                }
            });
            app.last_inbox_maintenance = Instant::now();
        }

        // Poll workflow status for the selected session every 2s
        if last_workflow_poll.elapsed() > Duration::from_secs(2) {
            let cwd = app.managed_sessions
                .get(app.session_tab_selected)
                .map(|s| std::path::PathBuf::from(&s.cwd));
            app.workflow_status = cwd.and_then(|p| orrch_core::load_workflow_status(&p));
            last_workflow_poll = Instant::now();
        }

        // WebUI sync — push state and drain actions every ~1s
        if last_webui_sync.elapsed() > Duration::from_secs(1) {
            if let Some(ref srv) = webui {
                srv.update_state(app.web_snapshot());
                for action in srv.drain_actions() {
                    use orrch_webui::WebAction;
                    match action {
                        WebAction::Key { ref key } => {
                            use crossterm::event::KeyCode;
                            let code = match key.as_str() {
                                "n" => KeyCode::Char('n'),
                                "s" => KeyCode::Char('s'),
                                "r" => KeyCode::Char('r'),
                                "X" => KeyCode::Char('X'),
                                "Enter" | "\n" | "\r" => KeyCode::Enter,
                                "Escape" => KeyCode::Esc,
                                "Tab" => KeyCode::Tab,
                                "ArrowUp" => KeyCode::Up,
                                "ArrowDown" => KeyCode::Down,
                                "ArrowLeft" => KeyCode::Left,
                                "ArrowRight" => KeyCode::Right,
                                _ => continue,
                            };
                            use crossterm::event::KeyModifiers;
                            let _ = app.handle_key(code, KeyModifiers::empty());
                        }
                        WebAction::Retract { ref filename } => {
                            let vault = orrch_core::vault::vault_dir(&app.projects_dir);
                            let _ = orrch_core::vault::update_pipeline_progress(&vault, filename, 0);
                            app.ideas = orrch_core::vault::load_ideas(&vault);
                        }
                    }
                }
            }
            last_webui_sync = Instant::now();
        }

        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        app.handle_key(key.code, key.modifiers)?;
                    }
                }
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            app.handle_scroll(-3);
                        }
                        MouseEventKind::ScrollDown => {
                            app.handle_scroll(3);
                        }
                        MouseEventKind::Down(_) => {
                            // Click on the WebUI URL badge → open browser + copy URL
                            if let Some(badge) = app.webui_badge_area {
                                let (cx, cy) = (mouse.column, mouse.row);
                                if cy >= badge.y && cy < badge.y + badge.height
                                    && cx >= badge.x && cx < badge.x + badge.width
                                {
                                    app.open_webui();
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
