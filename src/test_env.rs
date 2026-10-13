//! Process-global environment guard for lib tests.
//!
//! Lib tests share one process. Every test that mutates `HOME`, an `OO_*`
//! variable or the cwd must hold [`env_guard`] for the whole mutation; otherwise
//! two modules race (one removes `HOME` while another sets it). The guard
//! serializes mutators only. Readers are not covered, so a test that depends on
//! the exact value of one of these must hold the guard too.
//!
//! The guard snapshots the tracked variables and the cwd when acquired and
//! restores them on drop, so a panicking test cannot leak state to the next
//! holder. Restore runs before the lock is released, which makes recovery from a
//! poisoned lock safe.
//!
//! No unit test checks that the lock serializes: a single-threaded test passes
//! trivially. The stress loop at `RUST_TEST_THREADS=64` is the verification.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Variables that lib tests mutate; restored when an [`EnvGuard`] drops.
const TRACKED_VARS: &[&str] = &[
    "HOME",
    "OO_CONFIG_DIR",
    "OO_DATA_DIR",
    "OO_CLAUDE_DIR",
    "OO_PI_EXTENSIONS_DIR",
    "TMPDIR",
];

/// Holds the shared environment lock; the environment is restored when it drops.
pub struct EnvGuard {
    // Declared first, so it drops first: state is restored before the lock is released.
    _restore: Restore,
    _lock: MutexGuard<'static, ()>,
}

/// Acquires the shared environment guard and snapshots the tracked state.
pub fn env_guard() -> EnvGuard {
    let lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    EnvGuard {
        _restore: Restore::capture(),
        _lock: lock,
    }
}

/// Snapshot of the tracked environment and cwd, restored on drop. Holds no lock,
/// so the restore logic can be exercised on its own.
struct Restore {
    saved_vars: Vec<(&'static str, Option<OsString>)>,
    saved_cwd: Option<PathBuf>,
}

impl Restore {
    fn capture() -> Self {
        Self {
            saved_vars: TRACKED_VARS
                .iter()
                .map(|&key| (key, std::env::var_os(key)))
                .collect(),
            saved_cwd: std::env::current_dir().ok(),
        }
    }
}

impl Drop for Restore {
    fn drop(&mut self) {
        for (key, value) in &self.saved_vars {
            // SAFETY: the shared lock is held by the owning `EnvGuard`, so no other
            // lib test is mutating the process environment concurrently.
            unsafe {
                match value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }
        if let Some(cwd) = &self.saved_cwd {
            if let Err(e) = std::env::set_current_dir(cwd) {
                eprintln!("test_env: could not restore cwd {}: {e}", cwd.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Restore, env_guard};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn restore_reverts_tracked_var_when_unwinding() {
        // Holds the shared lock so no other test changes OO_CLAUDE_DIR meanwhile.
        let _env = env_guard();
        let before = std::env::var_os("OO_CLAUDE_DIR");
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _restore = Restore::capture();
            // SAFETY: this test holds the shared lock.
            unsafe { std::env::set_var("OO_CLAUDE_DIR", "/nonexistent-env-guard-probe") };
            panic!("simulated failure mid-mutation");
        }));
        assert!(result.is_err(), "the closure must unwind");
        assert_eq!(
            std::env::var_os("OO_CLAUDE_DIR"),
            before,
            "an unwinding test must not leak its environment mutation"
        );
    }
}
