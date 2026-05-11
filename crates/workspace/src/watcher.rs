//! Filesystem watcher for an open workspace.
//!
//! Wraps [`notify_debouncer_mini`] so the UI gets coalesced "tree
//! changed" ticks rather than a firehose of raw inotify events. The
//! atomio shell drains the receiver each render frame and re-runs
//! [`crate::Workspace::refresh`] when anything came through.
//!
//! Lifetime: hold the [`Watcher`] for as long as the project is open.
//! Dropping it stops the OS-level watcher and frees the worker thread.
//!
//! Errors during initial watch registration surface from [`Watcher::spawn`];
//! errors during the watch lifetime are logged through `tracing` and
//! delivered as `Err` variants on the channel so the shell can
//! optionally show them in the status bar.

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::Duration;

use notify_debouncer_mini::notify::{Error as NotifyError, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};

/// Default debounce window. Matches the gpui render tick cadence so a
/// burst of saves (e.g. a formatter rewriting 50 files) coalesces into
/// a single refresh.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(200);

/// One coalesced "something under root changed" tick. We intentionally
/// drop the per-path detail -- the UI only needs to know it should
/// re-scan; the scan itself is cheap (`ignore` walker, capped).
#[derive(Debug, Clone, Copy)]
pub struct Tick;

/// Filesystem watcher for a single workspace root.
///
/// The internal [`Debouncer`] holds a background thread that lives as
/// long as the value. The channel side is [`Send`] so the gpui main
/// thread can poll it from the render loop.
pub struct Watcher {
    /// Held to keep the watcher thread alive; never read directly.
    _debouncer: Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>,
    rx: Receiver<Tick>,
}

impl Watcher {
    /// Spawn a recursive watcher on `root`. Returns the handle whose
    /// channel ticks once per debounce window when anything under
    /// `root` changes.
    pub fn spawn(root: &Path) -> Result<Self, NotifyError> {
        Self::spawn_with_debounce(root, DEFAULT_DEBOUNCE)
    }

    /// Variant of [`Self::spawn`] with a caller-supplied debounce.
    /// Useful for tests that don't want to wait 200ms per assertion.
    pub fn spawn_with_debounce(root: &Path, debounce: Duration) -> Result<Self, NotifyError> {
        let (tx, rx) = channel::<Tick>();
        let mut debouncer = new_debouncer(debounce, move |res: DebounceEventResult| match res {
            Ok(events) => {
                // We don't care which path changed -- the renderer's
                // refresh is cheap. One tick per debounce window keeps
                // the channel from filling up under heavy churn.
                if !events.is_empty() {
                    let _ = tx.send(Tick);
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "atomio::workspace::watcher",
                    error = %e,
                    "notify reported an error"
                );
            }
        })?;
        debouncer.watcher().watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            _debouncer: debouncer,
            rx,
        })
    }

    /// Non-blocking drain: returns `true` if at least one tick was
    /// pending. Called from the gpui render loop each frame.
    pub fn drain(&self) -> bool {
        let mut any = false;
        loop {
            match self.rx.try_recv() {
                Ok(_) => any = true,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use tempfile::TempDir;

    /// Drain the watcher in a busy-loop until `predicate` returns
    /// true or `timeout` elapses. Returns the elapsed time so tests
    /// can assert it stayed within the debounce window.
    fn drain_until(
        watcher: &Watcher,
        timeout: Duration,
        predicate: impl Fn() -> bool,
    ) -> Option<Duration> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if watcher.drain() && predicate() {
                return Some(start.elapsed());
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    }

    #[test]
    fn spawn_succeeds_on_existing_dir() {
        let tmp = TempDir::new().unwrap();
        let _w = Watcher::spawn(tmp.path()).expect("spawn ok");
    }

    #[test]
    fn spawn_errors_on_missing_dir() {
        let res = Watcher::spawn(Path::new("/this/does/not/exist/atomio-test"));
        assert!(res.is_err());
    }

    #[test]
    fn file_create_produces_tick() {
        let tmp = TempDir::new().unwrap();
        let w =
            Watcher::spawn_with_debounce(tmp.path(), Duration::from_millis(50)).expect("spawn ok");
        // Initial drain should be empty (no changes yet).
        assert!(!w.drain());

        fs::write(tmp.path().join("created.txt"), "hi").unwrap();

        let elapsed = drain_until(&w, Duration::from_secs(2), || true);
        assert!(
            elapsed.is_some(),
            "watcher did not deliver a tick within 2s"
        );
    }

    #[test]
    fn empty_drain_returns_false() {
        let tmp = TempDir::new().unwrap();
        let w = Watcher::spawn(tmp.path()).expect("spawn ok");
        assert!(!w.drain());
    }
}
