//! Embedded static assets for the web node editor.
//!
//! The real HTML/JS/CSS lives in `assets/*` under the crate root and is
//! pulled in via `include_str!` so the resulting binary is self-contained —
//! no filesystem access required at runtime. This keeps the TUI's "open web
//! editor" UX a single click even on machines that don't have the source
//! tree laid out next to the binary.

/// Embedded index page for the node editor.
pub const INDEX_HTML: &str = include_str!("../assets/index.html");

/// Embedded client-side JavaScript for the node editor canvas.
pub const APP_JS: &str = include_str!("../assets/app.js");

/// Embedded stylesheet for the node editor.
pub const STYLE_CSS: &str = include_str!("../assets/style.css");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_contains_canvas_element() {
        assert!(
            INDEX_HTML.contains("<canvas"),
            "embedded index.html must contain a <canvas> element for the node editor"
        );
    }

    #[test]
    fn assets_are_non_empty() {
        assert!(!INDEX_HTML.is_empty());
        assert!(!APP_JS.is_empty());
        assert!(!STYLE_CSS.is_empty());
    }
}
