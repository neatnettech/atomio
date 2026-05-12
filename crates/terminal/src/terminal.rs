//! PTY-backed terminal session.
//!
//! Spawns a child process behind a pseudo-terminal and feeds its
//! output through the ANSI [`crate::Parser`] into a [`crate::Grid`].
//!
//! Threading model:
//! - The UI thread holds a [`Terminal`] handle.
//! - One background thread reads PTY output, parses it into the
//!   shared grid (guarded by a `Mutex`), and pings a `dirty` channel.
//! - The UI thread sends keyboard input through `send_input`, which
//!   writes directly to the PTY master.
//!
//! The grid is intentionally cheap to snapshot under the lock; the UI
//! drops the lock immediately after cloning.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

use crate::ansi::Parser;
use crate::grid::{Grid, GridSnapshot};

/// One PTY session: child process + grid + read thread.
///
/// Dropping the value closes the PTY master and the child reads EOF.
/// The reader thread exits on its own once stdout closes; we don't
/// `join` it because the read may block indefinitely on slow shells.
pub struct Terminal {
    grid: Arc<Mutex<Grid>>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    dirty_rx: Receiver<()>,
    /// Held to keep the reader thread + reader handle alive for the
    /// lifetime of the Terminal. Not read directly.
    _reader: thread::JoinHandle<()>,
    /// Child process handle. Dropping it leaves the process attached
    /// to the PTY; killing it requires `child.kill()` explicitly.
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl Terminal {
    /// Spawn `argv[0]` with `argv[1..]` as args, cwd'd to `cwd`, in a
    /// freshly allocated PTY of `cols x rows`.
    ///
    /// Returns an error when the PTY allocation or the spawn itself
    /// fails; otherwise the reader thread is already running and the
    /// caller can start rendering / forwarding input.
    pub fn spawn(argv: &[&str], cwd: &Path, cols: u16, rows: u16) -> std::io::Result<Self> {
        if argv.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "argv must be non-empty",
            ));
        }
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                cols,
                rows,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(map_pty_err)?;

        let mut cmd = CommandBuilder::new(argv[0]);
        for arg in &argv[1..] {
            cmd.arg(arg);
        }
        cmd.cwd(cwd);
        // Pass through PATH + HOME so spawned shells find their tools.
        for (k, v) in std::env::vars() {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).map_err(map_pty_err)?;

        let mut reader = pair.master.try_clone_reader().map_err(map_pty_err)?;
        let writer = pair.master.take_writer().map_err(map_pty_err)?;

        let grid = Arc::new(Mutex::new(Grid::new(cols, rows)));
        let grid_for_reader = Arc::clone(&grid);
        let (dirty_tx, dirty_rx) = channel::<()>();
        let reader_handle = thread::Builder::new()
            .name("atomio-terminal-reader".into())
            .spawn(move || {
                let mut parser = Parser::new();
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Ok(mut g) = grid_for_reader.lock() {
                                parser.advance(&mut g, &buf[..n]);
                            }
                            let _ = dirty_tx.send(());
                        }
                        Err(e) => {
                            tracing::debug!(
                                target: "atomio::terminal",
                                error = %e,
                                "pty read errored, reader exiting"
                            );
                            break;
                        }
                    }
                }
            })
            .map_err(std::io::Error::other)?;

        Ok(Self {
            grid,
            writer,
            master: pair.master,
            dirty_rx,
            _reader: reader_handle,
            child,
        })
    }

    /// Write bytes to the PTY's stdin. Returns the number written.
    pub fn send_input(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.writer.write(bytes)
    }

    /// Resize the PTY + grid in lockstep so the child receives the
    /// matching `SIGWINCH`.
    pub fn resize(&mut self, cols: u16, rows: u16) -> std::io::Result<()> {
        self.master
            .resize(PtySize {
                cols,
                rows,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(map_pty_err)?;
        if let Ok(mut g) = self.grid.lock() {
            g.resize(cols, rows);
        }
        Ok(())
    }

    /// Cheap immutable snapshot for the render loop.
    pub fn snapshot(&self) -> GridSnapshot {
        self.grid
            .lock()
            .map(|g| g.snapshot())
            .unwrap_or_else(|_| Grid::new(1, 1).snapshot())
    }

    /// Drain pending dirty pings without blocking. Returns `true` when
    /// at least one read happened since the last drain.
    pub fn drain_dirty(&self) -> bool {
        let mut any = false;
        loop {
            match self.dirty_rx.try_recv() {
                Ok(_) => any = true,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        any
    }

    /// Try to reap the child. `Some(status)` when the process exited;
    /// `None` while still running. Never blocks.
    pub fn poll_exit(&mut self) -> Option<portable_pty::ExitStatus> {
        self.child.try_wait().ok().flatten()
    }
}

fn map_pty_err(e: anyhow::Error) -> std::io::Error {
    std::io::Error::other(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn drain_until<F: Fn(&GridSnapshot) -> bool>(
        term: &Terminal,
        timeout: Duration,
        predicate: F,
    ) -> Option<GridSnapshot> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            let snap = term.snapshot();
            if predicate(&snap) {
                return Some(snap);
            }
            thread::sleep(Duration::from_millis(20));
        }
        None
    }

    fn render_first_row(snap: &GridSnapshot) -> String {
        let cols = snap.cols as usize;
        snap.cells[..cols].iter().map(|c| c.ch).collect()
    }

    #[test]
    fn spawn_errors_on_empty_argv() {
        let res = Terminal::spawn(&[], Path::new("."), 80, 24);
        assert!(res.is_err());
    }

    #[test]
    fn echo_command_lands_in_grid() {
        // Use `printf` so we don't depend on echo's behaviour around
        // trailing newlines across shells.
        let mut t = Terminal::spawn(
            &["/bin/sh", "-c", "printf hello && sleep 0.1"],
            Path::new("."),
            16,
            4,
        )
        .expect("spawn ok");
        let snap = drain_until(&t, Duration::from_secs(3), |s| {
            render_first_row(s).starts_with("hello")
        })
        .expect("expected 'hello' on first row within 3s");
        assert!(render_first_row(&snap).starts_with("hello"));
        // Reap so the test process doesn't leak the child.
        let _ = t.poll_exit();
    }

    #[test]
    fn resize_updates_grid_dimensions() {
        let mut t = Terminal::spawn(&["/bin/sh", "-c", "sleep 0.5"], Path::new("."), 8, 4)
            .expect("spawn ok");
        t.resize(20, 10).expect("resize ok");
        let snap = t.snapshot();
        assert_eq!(snap.cols, 20);
        assert_eq!(snap.rows, 10);
    }
}
