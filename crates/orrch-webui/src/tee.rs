//! TeeWriter: writes bytes to a local sink (stdout) AND broadcasts them
//! to a tokio broadcast channel for consumption by WebUI terminal clients.
//!
//! Failed broadcasts (no receivers, backpressure) are silently dropped.
//! The local sink is the source of truth; if writing to it fails, the
//! error is propagated — that's what the ratatui backend needs to stay
//! in sync.

use std::io::{self, Write};

use tokio::sync::broadcast;

pub struct TeeWriter<W: Write> {
    local: W,
    tx: broadcast::Sender<Vec<u8>>,
    /// Max size of a single broadcast chunk. Large chunks are split.
    chunk_size: usize,
}

impl<W: Write> TeeWriter<W> {
    pub fn new(local: W, tx: broadcast::Sender<Vec<u8>>, chunk_size: usize) -> Self {
        Self { local, tx, chunk_size }
    }

    fn broadcast_bytes(&self, buf: &[u8]) {
        for chunk in buf.chunks(self.chunk_size) {
            let receivers = self.tx.receiver_count();
            let result = self.tx.send(chunk.to_vec());
            if receivers > 0 {
                tracing::info!(
                    "TEE: {} bytes -> {} receivers, result={:?}",
                    chunk.len(),
                    receivers,
                    result.as_ref().map(|n| *n).map_err(|_| "no receivers")
                );
            }
        }
    }
}

impl<W: Write> Write for TeeWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.local.write(buf)?;
        // Broadcast only what was actually written to local.
        if n > 0 {
            self.broadcast_bytes(&buf[..n]);
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.local.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.local.write_all(buf)?;
        self.broadcast_bytes(buf);
        Ok(())
    }
}
