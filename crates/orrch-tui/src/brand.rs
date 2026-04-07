//! Orrchestrator brand — definitive logo and color palette.
//!
//! The authoritative source for these values lives at
//! `<workspace_root>/branding/BRAND.md`. Do not redefine these constants
//! in other modules; import from here.
//!
//! The palette is intended for truecolor terminals via `ratatui::style::Color::Rgb`.
//! 16-color ANSI fallbacks are deliberately not provided — the TUI depends on
//! truecolor for panel hierarchy, and a degraded palette would misrepresent the
//! brand. On a legacy terminal, the UI will render with dim approximations from
//! ratatui's default styling rather than muddled brand colors.

use ratatui::style::Color;

/// Raw PNG bytes of the authoritative orrchestrator logo.
///
/// Embedded at build time from `crates/orrch-tui/assets/logo.png`, which is
/// itself cropped from `branding/logo.png` at the workspace root.
///
/// Useful for an "About" dialog or a splash rendered via a terminal image
/// protocol (kitty/sixel/iTerm2) — see `markdown_image.rs` for existing
/// image-render plumbing.
pub const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");

/// Cortex Azure — brand primary. Active panel tabs, highlighted selection.
pub const PRIMARY: Color = Color::Rgb(0x08, 0x88, 0xA8);

/// Neural Sky — highlight. Focus ring, hover, active row.
pub const HIGHLIGHT: Color = Color::Rgb(0x08, 0x78, 0x98);

/// Signal Current — secondary buttons, badges.
pub const MID: Color = Color::Rgb(0x08, 0x68, 0x88);

/// Deep Current — borders, muted text on light.
pub const DEEP: Color = Color::Rgb(0x08, 0x58, 0x78);

/// Midnight Circuit — background, status bar, deep panels.
pub const SHADOW: Color = Color::Rgb(0x08, 0x38, 0x58);

/// Returns the full palette in brand-order (primary → shadow).
/// Useful for rendering a palette swatch in a diagnostic/about view.
pub const PALETTE: [Color; 5] = [PRIMARY, HIGHLIGHT, MID, DEEP, SHADOW];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_bytes_are_a_png() {
        // PNG magic number: 89 50 4E 47 0D 0A 1A 0A
        assert_eq!(
            &LOGO_PNG[..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
            "embedded logo is not a PNG — rerun the branding crop pipeline"
        );
    }

    #[test]
    fn palette_is_five_colors() {
        assert_eq!(PALETTE.len(), 5);
    }
}
