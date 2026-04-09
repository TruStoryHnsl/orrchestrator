//! Native egui window for the node editor / non-TUI mode (PLAN items 38 / 39).
//!
//! This is the "terminal-averse" entry point: it launches a desktop window,
//! loads workforces from disk via `orrch_workforce::load_workforces`, and
//! presents a minimal sidebar-plus-detail view. The user can select a
//! workforce from the left and see its agents and connections on the right.
//!
//! The implementation is feature-gated behind `egui-window` so the default
//! build has zero new dependencies. When the feature is disabled,
//! [`launch_egui_window`] returns a clear error telling the user how to
//! rebuild.
//!
//! Scope (per PLAN item 39): this is a *viewer*. Editing is handled by the
//! web editor (PLAN item 37). The egui window exists so that users without
//! terminal comfort can still explore the workforce library.

/// Launch the native egui window. Blocks the current thread until the
/// window is closed.
///
/// `workforces_dir` — directory to load workforce markdown files from.
/// Passing `None` uses the current working directory's `workforces/` folder.
#[cfg(feature = "egui-window")]
pub fn launch_egui_window() -> anyhow::Result<()> {
    use eframe::egui;

    let workforces_dir = default_workforces_dir();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_title("orrchestrator — workforce viewer"),
        ..Default::default()
    };

    eframe::run_native(
        "orrchestrator",
        native_options,
        Box::new(move |_cc| Ok(Box::new(WorkforceViewerApp::new(workforces_dir)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe failed: {e}"))
}

/// Feature-disabled fallback. Returns a clear error so the user knows what to do.
#[cfg(not(feature = "egui-window"))]
pub fn launch_egui_window() -> anyhow::Result<()> {
    anyhow::bail!(
        "egui window support is not compiled into this build.\n\
         Rebuild with:  cargo build --release --features orrch-tui/egui-window\n\
         (see PLAN.md items 38/39 — native egui window scaffold)"
    )
}

/// Best-effort default for the workforces directory.
///
/// Prefers `$ORRCH_WORKFORCES_DIR` if set, otherwise `./workforces`. If
/// neither exists the app still launches with an empty list and surfaces
/// the fact in the sidebar.
#[allow(dead_code)]
fn default_workforces_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("ORRCH_WORKFORCES_DIR") {
        return std::path::PathBuf::from(dir);
    }
    std::env::current_dir()
        .map(|p| p.join("workforces"))
        .unwrap_or_else(|_| std::path::PathBuf::from("workforces"))
}

/// Viewer app state. Public (crate-private in practice) for the sake of
/// unit tests that exercise the loader without opening a window.
#[allow(dead_code)]
pub(crate) struct WorkforceViewerApp {
    workforces: Vec<orrch_workforce::Workforce>,
    selected: Option<usize>,
    load_error: Option<String>,
    workforces_dir: std::path::PathBuf,
}

#[allow(dead_code)]
impl WorkforceViewerApp {
    /// Construct a new viewer by loading workforces from `dir`.
    ///
    /// If the directory doesn't exist the app still constructs cleanly —
    /// `workforces` will be empty and `load_error` will hold a friendly
    /// message for the UI to surface.
    pub(crate) fn new(dir: std::path::PathBuf) -> Self {
        let (workforces, load_error) = if dir.exists() {
            (orrch_workforce::load_workforces(&dir), None)
        } else {
            (
                Vec::new(),
                Some(format!("workforces directory not found: {}", dir.display())),
            )
        };
        let selected = if workforces.is_empty() { None } else { Some(0) };
        Self {
            workforces,
            selected,
            load_error,
            workforces_dir: dir,
        }
    }

    /// Number of workforces currently loaded. Used by tests.
    pub(crate) fn workforce_count(&self) -> usize {
        self.workforces.len()
    }
}

#[cfg(feature = "egui-window")]
impl eframe::App for WorkforceViewerApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        use eframe::egui;

        egui::SidePanel::left("workforces")
            .resizable(true)
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.heading("Workforces");
                ui.separator();

                if let Some(err) = &self.load_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, err);
                    ui.separator();
                }

                if self.workforces.is_empty() {
                    ui.label("(no workforces loaded)");
                    ui.add_space(4.0);
                    ui.label(format!("dir: {}", self.workforces_dir.display()));
                    return;
                }

                for (idx, wf) in self.workforces.iter().enumerate() {
                    let is_selected = self.selected == Some(idx);
                    if ui.selectable_label(is_selected, &wf.name).clicked() {
                        self.selected = Some(idx);
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(idx) = self.selected else {
                ui.vertical_centered(|ui| {
                    ui.add_space(120.0);
                    ui.heading("orrchestrator");
                    ui.label("Select a workforce from the sidebar.");
                });
                return;
            };
            let Some(wf) = self.workforces.get(idx) else {
                return;
            };

            ui.heading(&wf.name);
            ui.label(&wf.description);
            ui.separator();

            ui.label(format!("Operations: {}", wf.operations.join(", ")));
            ui.add_space(8.0);

            ui.collapsing(format!("Agents ({})", wf.agents.len()), |ui| {
                egui::Grid::new("agents-grid")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("ID");
                        ui.strong("Profile");
                        ui.strong("User-Facing");
                        ui.strong("Nested");
                        ui.end_row();
                        for a in &wf.agents {
                            ui.label(&a.id);
                            ui.label(&a.agent_profile);
                            ui.label(if a.user_facing { "yes" } else { "no" });
                            ui.label(a.nested_workforce.as_deref().unwrap_or("-"));
                            ui.end_row();
                        }
                    });
            });

            ui.collapsing(format!("Connections ({})", wf.connections.len()), |ui| {
                egui::Grid::new("conns-grid")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("From");
                        ui.strong("To");
                        ui.strong("Data");
                        ui.end_row();
                        for c in &wf.connections {
                            ui.label(&c.from);
                            ui.label(&c.to);
                            ui.label(format!("{:?}", c.data_type));
                            ui.end_row();
                        }
                    });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(feature = "egui-window"))]
    fn launch_errors_without_feature() {
        let err = launch_egui_window().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("egui-window"),
            "error should mention the feature flag, got: {msg}"
        );
    }

    /// Compile-time available regardless of feature flag: loader must
    /// handle a missing directory cleanly.
    #[test]
    fn new_with_missing_dir_returns_empty_with_error() {
        let dir = std::path::PathBuf::from("/nonexistent/orrch/workforces");
        let app = WorkforceViewerApp::new(dir);
        assert_eq!(app.workforce_count(), 0);
        assert!(app.selected.is_none());
        assert!(app.load_error.is_some());
    }

    #[test]
    fn new_with_fixture_dir_loads_workforces() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("one.md");
        std::fs::write(
            &path,
            "---\nname: Fixture\ndescription: test\noperations:\n  - X\n---\n\n## Agents\n\n| ID | Agent Profile | User-Facing |\n|----|---------------|-------------|\n| pm | Project Manager | yes |\n",
        )
        .unwrap();

        let app = WorkforceViewerApp::new(tmp.path().to_path_buf());
        assert_eq!(app.workforce_count(), 1);
        assert_eq!(app.selected, Some(0));
        assert!(app.load_error.is_none());
    }
}
