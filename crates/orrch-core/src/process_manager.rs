use std::collections::HashMap;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;

use nix::sys::signal::{self, Signal};
use nix::sys::wait::WaitPidFlag;
use nix::unistd::Pid;
use tokio::sync::mpsc;
use tracing::error;

use crate::backend::{BackendKind, BackendsConfig};
use crate::session::{ExternalSession, Session, SessionState};

/// Events emitted by the process manager.
#[derive(Debug)]
pub enum SessionEvent {
    Output { sid: String, data: Vec<u8> },
    Died { sid: String },
}

/// Manages Claude Code sessions in PTYs.
pub struct ProcessManager {
    sessions: HashMap<String, Session>,
    external: Vec<ExternalSession>,
    next_id: u32,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
    pub backends: BackendsConfig,
}

impl ProcessManager {
    pub fn new(event_tx: mpsc::UnboundedSender<SessionEvent>) -> Self {
        Self {
            sessions: HashMap::new(),
            external: Vec::new(),
            next_id: 1,
            event_tx,
            backends: BackendsConfig::load(),
        }
    }

    pub fn sessions(&self) -> Vec<&Session> {
        self.sessions.values().collect()
    }

    pub fn external_sessions(&self) -> &[ExternalSession] {
        &self.external
    }

    pub fn get_session(&self, sid: &str) -> Option<&Session> {
        self.sessions.get(sid)
    }

    pub fn get_session_mut(&mut self, sid: &str) -> Option<&mut Session> {
        self.sessions.get_mut(sid)
    }

    /// Spawn a new Claude Code session in a PTY.
    pub fn spawn(
        &mut self,
        project_dir: &Path,
        backend: BackendKind,
        prompt: Option<&str>,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<String> {
        let sid = format!("s{}", self.next_id);
        self.next_id += 1;

        let mut cmd_args = self
            .backends
            .get_command(backend)
            .ok_or_else(|| anyhow::anyhow!("{} backend not available", backend.label()))?;
        if let Some(p) = prompt {
            // Positional argument, not -p (which is print/non-interactive mode)
            cmd_args.push(p.into());
        }

        // Open PTY pair via libc
        let mut master_fd: RawFd = -1;
        let mut slave_fd: RawFd = -1;
        let rc = unsafe { libc::openpty(&mut master_fd, &mut slave_fd, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
        if rc != 0 {
            anyhow::bail!("openpty failed");
        }

        let winsize = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe { libc::ioctl(slave_fd, libc::TIOCSWINSZ, &winsize) };

        let project_dir_owned = project_dir.to_path_buf();

        let pid = unsafe { libc::fork() };
        if pid < 0 {
            anyhow::bail!("fork failed");
        }

        if pid == 0 {
            // Child — become the Claude Code process
            unsafe {
                libc::close(master_fd);
                libc::setsid();
                libc::ioctl(slave_fd, libc::TIOCSCTTY, 0);
                libc::dup2(slave_fd, 0);
                libc::dup2(slave_fd, 1);
                libc::dup2(slave_fd, 2);
                if slave_fd > 2 {
                    libc::close(slave_fd);
                }
                let dir_cstr = std::ffi::CString::new(
                    project_dir_owned.to_string_lossy().as_bytes(),
                ).unwrap();
                libc::chdir(dir_cstr.as_ptr());

                let c_args: Vec<std::ffi::CString> = cmd_args
                    .iter()
                    .map(|a| std::ffi::CString::new(a.as_str()).unwrap())
                    .collect();
                let c_ptrs: Vec<*const libc::c_char> = c_args
                    .iter()
                    .map(|a| a.as_ptr())
                    .chain(std::iter::once(std::ptr::null()))
                    .collect();
                libc::execvp(c_ptrs[0], c_ptrs.as_ptr());
                libc::_exit(1);
            }
        }

        // Parent
        unsafe { libc::close(slave_fd) };

        // Set non-blocking on master
        unsafe {
            let flags = libc::fcntl(master_fd, libc::F_GETFL);
            libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }

        let nix_pid = Pid::from_raw(pid);
        let goal = prompt.map(|s| s.to_string());
        let session = Session::new(sid.clone(), project_dir.to_path_buf(), nix_pid, master_fd, backend, goal);
        self.sessions.insert(sid.clone(), session);

        // Spawn async reader task
        let tx = self.event_tx.clone();
        let sid_clone = sid.clone();
        tokio::spawn(async move {
            read_pty_loop(master_fd, sid_clone, tx).await;
        });

        Ok(sid)
    }

    /// Send input bytes to a session's PTY.
    pub fn write_to_session(&self, sid: &str, data: &[u8]) -> anyhow::Result<()> {
        if let Some(session) = self.sessions.get(sid) {
            if session.state != SessionState::Dead {
                unsafe { libc::write(session.master_fd, data.as_ptr() as *const _, data.len()) };
            }
        }
        Ok(())
    }

    /// Resize a session's PTY.
    pub fn resize_session(&self, sid: &str, rows: u16, cols: u16) {
        if let Some(session) = self.sessions.get(sid) {
            if session.state != SessionState::Dead {
                let winsize = libc::winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                unsafe { libc::ioctl(session.master_fd, libc::TIOCSWINSZ, &winsize) };
            }
        }
    }

    /// Kill a managed session.
    pub fn kill_session(&mut self, sid: &str) {
        if let Some(session) = self.sessions.get_mut(sid) {
            if let Ok(pgid) = nix::unistd::getpgid(Some(session.pid)) {
                let _ = signal::killpg(pgid, Signal::SIGTERM);
            }
            unsafe { libc::close(session.master_fd) };
            session.state = SessionState::Dead;
        }
    }

    /// Remove all dead sessions.
    pub fn remove_dead(&mut self) -> Vec<String> {
        let dead: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.state == SessionState::Dead)
            .map(|(sid, _)| sid.clone())
            .collect();
        for sid in &dead {
            if let Some(session) = self.sessions.remove(sid) {
                unsafe { libc::close(session.master_fd) };
            }
        }
        dead
    }

    /// Non-blocking waitpid to detect dead children.
    pub fn reap_children(&mut self) {
        for session in self.sessions.values_mut() {
            if session.state == SessionState::Dead {
                continue;
            }
            match nix::sys::wait::waitpid(session.pid, Some(WaitPidFlag::WNOHANG)) {
                Ok(nix::sys::wait::WaitStatus::Exited(..))
                | Ok(nix::sys::wait::WaitStatus::Signaled(..)) => {
                    session.state = SessionState::Dead;
                }
                _ => {}
            }
        }
    }

    /// Discover external Claude Code processes.
    /// Filters out: managed PIDs, shell wrappers, non-claude binaries.
    pub async fn discover_external(&mut self) -> anyhow::Result<()> {
        let output = tokio::process::Command::new("pgrep")
            .args(["-af", "claude"])
            .output()
            .await?;

        let managed_pids: std::collections::HashSet<i32> = self
            .sessions
            .values()
            .map(|s| s.pid.as_raw())
            .collect();

        let own_pid = std::process::id();

        let mut external = Vec::new();
        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            let mut parts = line.splitn(2, ' ');
            let pid: u32 = match parts.next().and_then(|p| p.parse().ok()) {
                Some(p) => p,
                None => continue,
            };
            let cmdline = parts.next().unwrap_or("").to_string();

            // Skip ourselves
            if pid == own_pid { continue; }
            // Skip managed sessions
            if managed_pids.contains(&(pid as i32)) { continue; }
            // Must be an actual claude binary, not a shell wrapper or pgrep itself
            if !cmdline.contains("/claude") && !cmdline.starts_with("claude") { continue; }
            // Skip shell wrappers (zsh -c, bash -c, etc.)
            if cmdline.contains("zsh -c") || cmdline.contains("bash -c") || cmdline.contains("sh -c") { continue; }
            // Skip pgrep itself
            if cmdline.contains("pgrep") { continue; }

            let cwd = std::fs::read_link(format!("/proc/{pid}/cwd"))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let session_name = crate::session::read_session_name(pid);
            external.push(ExternalSession { pid, project_dir: cwd, cmdline, session_name, host: String::new() });
        }

        self.external = external;
        Ok(())
    }

    /// Kill all sessions.
    pub fn cleanup(&mut self) {
        let sids: Vec<String> = self.sessions.keys().cloned().collect();
        for sid in sids {
            self.kill_session(&sid);
        }
        self.sessions.clear();
    }
}

/// Async PTY reader using tokio's AsyncFd.
async fn read_pty_loop(fd: RawFd, sid: String, tx: mpsc::UnboundedSender<SessionEvent>) {
    // Wrap in OwnedFd for AsyncFd (we won't actually close it — ProcessManager owns the fd)
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let async_fd = match tokio::io::unix::AsyncFd::new(owned) {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to create AsyncFd for {sid}: {e}");
            let _ = tx.send(SessionEvent::Died { sid });
            return;
        }
    };

    let mut buf = [0u8; 4096];
    loop {
        let mut ready = match async_fd.readable().await {
            Ok(r) => r,
            Err(_) => break,
        };

        match ready.try_io(|inner| {
            let n = unsafe {
                libc::read(inner.as_raw_fd(), buf.as_mut_ptr() as *mut _, buf.len())
            };
            if n > 0 {
                Ok(n as usize)
            } else if n == 0 {
                Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "EOF"))
            } else {
                Err(std::io::Error::last_os_error())
            }
        }) {
            Ok(Ok(n)) => {
                let _ = tx.send(SessionEvent::Output {
                    sid: sid.clone(),
                    data: buf[..n].to_vec(),
                });
            }
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Ok(Err(_)) | Err(_) => break,
        }
    }

    let _ = tx.send(SessionEvent::Died { sid });
    // Leak the OwnedFd so we don't double-close — ProcessManager owns it
    std::mem::forget(async_fd);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_labels() {
        assert_eq!(SessionState::Working.label(), "working");
        assert_eq!(SessionState::Dead.icon(), "💀");
    }

    #[test]
    fn test_external_session_display() {
        let ext = ExternalSession {
            pid: 12345,
            project_dir: "/home/corr/projects/orrapus".into(),
            cmdline: "claude --dangerously-skip-permissions".into(),
            session_name: String::new(),
            host: String::new(),
        };
        assert_eq!(ext.display_name(), "orrapus");
        assert!(!ext.is_remote());

        let named = ExternalSession {
            pid: 999,
            project_dir: "/home/corr/projects/concord".into(),
            cmdline: "claude".into(),
            session_name: "concord - main".into(),
            host: String::new(),
        };
        assert_eq!(named.display_name(), "concord - main");

        let remote = ExternalSession {
            pid: 46206,
            project_dir: "/Users/coltonorr/projects/concord".into(),
            cmdline: "claude --dangerously-skip-permissions".into(),
            session_name: String::new(),
            host: "orrpheus".into(),
        };
        assert_eq!(remote.host_badge(), "@orrpheus");
        assert!(remote.is_remote());
    }
}
