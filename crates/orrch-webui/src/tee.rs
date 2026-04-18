//! TeeWriter: writes bytes to a local sink (stdout) AND broadcasts them
//! to a tokio broadcast channel for WebUI terminal clients.
//!
//! Critically, broadcasts are deferred until `flush()` is called. This
//! matters because ratatui emits ANSI sequences byte-by-byte via
//! `queue!`, and many small WebSocket frames cause xterm.js's parser
//! to choke on split escape sequences. By coalescing into one big
//! broadcast per frame flush, we guarantee each broadcast is a coherent
//! chunk of output.

use std::io::{self, Write};

use tokio::sync::broadcast;

pub struct TeeWriter<W: Write> {
    local: W,
    tx: broadcast::Sender<Vec<u8>>,
    /// Accumulator: bytes written since the last flush. Sent as a single
    /// broadcast message on flush().
    pending: Vec<u8>,
    /// Soft cap for pending buffer — if we accumulate this many bytes
    /// without a flush, broadcast anyway so we don't grow unbounded.
    max_pending: usize,
}

impl<W: Write> TeeWriter<W> {
    pub fn new(local: W, tx: broadcast::Sender<Vec<u8>>, max_pending: usize) -> Self {
        Self { local, tx, pending: Vec::with_capacity(4096), max_pending }
    }

    fn broadcast_pending(&mut self) {
        if self.pending.is_empty() { return; }
        let receivers = self.tx.receiver_count();
        let len = self.pending.len();
        let payload = std::mem::replace(&mut self.pending, Vec::with_capacity(4096));
        let result = self.tx.send(payload);
        if receivers > 0 {
            tracing::info!(
                "TEE flush: {} bytes -> {} receivers, result={:?}",
                len,
                receivers,
                result.as_ref().map(|n| *n).map_err(|_| "no receivers")
            );
        }
    }
}

impl<W: Write> Write for TeeWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.local.write(buf)?;
        if n > 0 {
            self.pending.extend_from_slice(&buf[..n]);
            if self.pending.len() >= self.max_pending {
                self.broadcast_pending();
            }
        }
        Ok(n)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.local.write_all(buf)?;
        self.pending.extend_from_slice(buf);
        if self.pending.len() >= self.max_pending {
            self.broadcast_pending();
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.local.flush()?;
        self.broadcast_pending();
        Ok(())
    }
}
