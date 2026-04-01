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

    let result = run_loop(&mut terminal, &mut app).await;

    // Restore terminal FIRST — before any cleanup that might hang
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture);
    let _ = terminal.show_cursor();

    // Clean up managed sessions (non-blocking)
    app.pm.cleanup();

    // Force exit — don't wait for background tokio tasks (remote discovery, timers)
    // They're all detached and will die with the process.
    if result.is_ok() {
        std::process::exit(0);
    }
    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let mut last_discovery = Instant::now();
    let mut last_feedback_reload = Instant::now();
    let mut last_retrospect = Instant::now();

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
            last_discovery = Instant::now();
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

        // Handle vim request from app
        if let Some(req) = app.vim_request.take() {
            if let Some(child) = spawn_vim_window(&req.file, &req.title) {
                // New terminal window — TUI keeps running
                app.pending_editors.push(PendingEditor {
                    child,
                    file: req.file,
                    kind: req.kind,
                });
            } else {
                // Fallback: suspend TUI, run vim in same terminal
                disable_raw_mode()?;
                execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                terminal.show_cursor()?;
                // Use the same orrchestrator-branded vim args as the windowed path
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
