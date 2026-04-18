use axum::extract::ws::{Message, WebSocket};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

pub async fn handle_pty_ws(mut socket: WebSocket) {
    let pty_system = NativePtySystem::default();
    let pair = match pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("openpty: {e}");
            return;
        }
    };

    let mut cmd = CommandBuilder::new("tmux");
    // -A: attach if session exists, create it if not
    cmd.args(["new-session", "-A", "-s", "orrchestrator"]);
    let _child;
    _child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(_) => {
            let mut sh = CommandBuilder::new("bash");
            sh.env("TERM", "xterm-256color");
            match pair.slave.spawn_command(sh) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("spawn: {e}");
                    return;
                }
            }
        }
    };
    drop(pair.slave);

    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("clone_reader: {e}");
            return;
        }
    };
    let writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("take_writer: {e}");
            return;
        }
    };
    // master kept for resize; writer kept for stdin
    let master = Arc::new(Mutex::new(pair.master));
    let master2 = Arc::clone(&master);
    let writer = Arc::new(Mutex::new(writer));
    let writer2 = Arc::clone(&writer);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(32);

    tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    loop {
        tokio::select! {
            Some(bytes) = rx.recv() => {
                if socket.send(Message::Binary(bytes.into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        if let Ok(mut w) = writer2.lock() {
                            if w.write_all(&data).is_err() {
                                tracing::warn!("PTY stdin write failed; process likely dead");
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                            if v["type"] == "resize" {
                                let cols = v["cols"].as_u64().unwrap_or(80) as u16;
                                let rows = v["rows"].as_u64().unwrap_or(24) as u16;
                                if let Ok(m) = master2.lock() {
                                    let _ = m.resize(PtySize {
                                        rows,
                                        cols,
                                        pixel_width: 0,
                                        pixel_height: 0,
                                    });
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }
        }
    }
}
