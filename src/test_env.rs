//! Process-global environment guard for tests.
//!
//! Mutators of `HOME`, `OO_*` variables or the cwd must hold [`env_guard`] for the
//! whole mutation: tests share one process, so unguarded writes race across modules.
//! The guard serializes mutators only; readers are not covered.

use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Acquires the shared environment guard. A poisoned lock (a prior test panicked
/// mid-mutation) is recovered so one failure does not fail the whole suite.
pub fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
