//! Scaffolding for a native egui window (PLAN items 38 / 39).
//!
//! This is a first-slice scaffold only. The real node editor / non-TUI mode
//! lives behind follow-up work; right now the goal is:
//!
//! 1. Prove the feature flag wires correctly (no new deps by default, optional
//!    `eframe` + `egui` when `--features egui-window` is passed).
//! 2. Give `main.rs` something to call when the user passes `--egui`.
//! 3. Present a consistent `launch_egui_window()` API regardless of whether
//!    the feature is enabled — the disabled variant returns a clear error
//!    telling the user to rebuild with the feature.

/// Launch the native egui window. Blocks the current thread until the window
/// is closed.
///
/// When the `egui-window` feature is disabled (the default), this returns an
/// error instructing the user how to enable the feature. When enabled, it
/// spawns a minimal `eframe` app that renders a centered stub label.
#[cfg(feature = "egui-window")]
pub fn launch_egui_window() -> anyhow::Result<()> {
    use eframe::egui;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("orrchestrator"),
        ..Default::default()
    };

    eframe::run_native(
        "orrchestrator",
        native_options,
        Box::new(|_cc| Ok(Box::new(StubApp::default()))),
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

#[cfg(feature = "egui-window")]
#[derive(Default)]
struct StubApp;

#[cfg(feature = "egui-window")]
impl eframe::App for StubApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);
                ui.heading("orrchestrator");
                ui.add_space(16.0);
                ui.label("egui window — stub");
                ui.add_space(8.0);
                ui.label("Scaffold for PLAN items 38 / 39 (node editor / non-TUI mode).");
                ui.add_space(32.0);
                ui.label("Close the window to return to the shell.");
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
}
