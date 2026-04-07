# Orrchestrator — Brand

**Tagline:** AI Model Hypervisor

![Orrchestrator logo](logo.png)

The image at `branding/logo.png` is the **definitive logo** for orrchestrator.
It was cropped from the authoritative icon sheet at
`~/projects/personal_icon_pack.png`. Do not substitute, recolor, or redraw.

## Glyph

A central azure cog radiating into a ring of dendritic processes, each ending
in a small node — a hypervisor at the center, a workforce of agents at the
rim. The rays are irregular, not a starburst: every worker is doing something
slightly different. The inner "O" is the dispatcher; the outer nodes are the
sessions being managed.

## Color palette

| Role       | Hex        | Name            | Use                                     |
|------------|------------|-----------------|-----------------------------------------|
| Primary    | `#0888A8`  | Cortex Azure    | Brand blue, highlighted panel tab       |
| Highlight  | `#087898`  | Neural Sky      | Focus ring, selected row, active tab    |
| Mid        | `#086888`  | Signal Current  | Secondary buttons, badges               |
| Deep       | `#085878`  | Deep Current    | Borders, muted text on light            |
| Shadow     | `#083858`  | Midnight Circuit| TUI background, status bar, deep panels |

## Usage

- This is a terminal-first (ratatui) application. Map the palette to
  ratatui `Color::Rgb(r, g, b)` values — do **not** use the 16-color ANSI
  approximations, the palette depends on truecolor output.
- The current active panel tab should always render in **Cortex Azure**;
  inactive tabs in **Deep Current**. This is the only convention orrchestrator
  needs.
- Never pair with the concord greens in the same panel — blue-on-green is
  the color of success/disconnect noise in the Hypervise panel's status chips
  and must stay unambiguous.
- When rendering workforce graphs, edges should use **Neural Sky** at 40%
  alpha so the underlying crate structure reads cleanly.
