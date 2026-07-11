//! Debounced file watching for `ara serve` live reload.
//!
//! Watches the ARA directory recursively; a debounced change wakes the async
//! receiver, which reparses and swaps the cache. `--poll` selects the polling
//! backend for network mounts / cross-boundary bind mounts where inotify-style
//! events are unreliable.

use std::path::Path;
use std::time::Duration;

use notify_debouncer_full::notify::{self, PollWatcher, RecursiveMode};
use notify_debouncer_full::{
    DebounceEventResult, RecommendedCache, new_debouncer, new_debouncer_opt,
};
use tokio::sync::mpsc::UnboundedSender;

/// Debounce window: collapses editor save-bursts into a single reparse.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// Start watching `dir`; every debounced batch sends `()` on `tx`.
///
/// The debouncer owns a background thread and must outlive the server, so it is
/// intentionally leaked (`Box::leak`): the process owns it for its whole
/// lifetime and the two watcher backends have distinct types that cannot share
/// one binding. Returns an error only if the initial watch registration fails.
pub fn spawn(dir: &Path, poll: bool, tx: UnboundedSender<()>) -> notify::Result<()> {
    let handler = move |res: DebounceEventResult| {
        if res.is_ok() {
            // Ignore send errors: a closed receiver means the server is shutting
            // down, in which case there is nothing left to notify.
            let _ = tx.send(());
        }
    };

    if poll {
        let config = notify::Config::default().with_poll_interval(DEBOUNCE);
        let mut debouncer = new_debouncer_opt::<_, PollWatcher, RecommendedCache>(
            DEBOUNCE,
            None,
            handler,
            RecommendedCache::new(),
            config,
        )?;
        debouncer.watch(dir, RecursiveMode::Recursive)?;
        Box::leak(Box::new(debouncer));
    } else {
        let mut debouncer = new_debouncer(DEBOUNCE, None, handler)?;
        debouncer.watch(dir, RecursiveMode::Recursive)?;
        Box::leak(Box::new(debouncer));
    }

    Ok(())
}
