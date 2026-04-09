//! Local HTTP server serving a drag-and-drop workforce node editor.
//!
//! Implements PLAN item 37: a web-based node editor that reads and writes
//! structured markdown workforce files via `orrch-workforce`. The server is
//! intentionally tiny — it uses `tiny_http` (a synchronous, zero-dependency
//! HTTP/1.1 implementation) so we do not drag a full async stack into the TUI
//! process just for a development tool.
//!
//! ## Three ways to launch
//!
//! 1. **Standalone CLI**: `orrchestrator --webedit` runs the server in
//!    headless mode (no terminal required), prints the URL to stdout, and
//!    blocks on Ctrl-C. Useful for terminal-averse users and remote dev.
//!
//! 2. **From the TUI**: press `Ctrl+w` while in the Design > Workforce panel.
//!    The TUI starts the server in a background thread, opens the URL in the
//!    system default browser via `xdg-open`, and continues running. The
//!    server stops automatically when the TUI exits.
//!
//! 3. **Programmatic**: call [`launch_webedit_server`] directly. The
//!    returned [`ServerHandle`] stops the worker thread on `Drop`.
//!
//! ## Programmatic example
//!
//! ```no_run
//! use std::path::PathBuf;
//! use orrch_webedit::launch_webedit_server;
//!
//! let handle = launch_webedit_server(PathBuf::from("/tmp/workforces"), 0)
//!     .expect("server starts");
//! println!("open http://{}", handle.addr());
//! // handle is dropped → server thread is asked to stop
//! ```

pub mod api;
pub mod assets;
pub mod server;

pub use server::{launch_webedit_server, ServerHandle};
