// Test-only helpers for isolating the store's data directory per test thread.
//
// `set_test_data_dir` points the store at a temporary directory for the
// duration of one test, without mutating the process-wide `OO_DATA_DIR`
// environment variable (which is shared across the parallel test threads of
// one test binary and would leak into other tests' store opens).
//
// A `thread_local` gives each test thread its own isolation without any shared
// lock — `db_path()` reads the thread-local on every call, so there is no
// re-entrant lock risk.

use std::path::PathBuf;

/// Guard that resets the thread-local test data dir on drop.
pub(crate) struct TestDataDirGuard;

impl Drop for TestDataDirGuard {
    fn drop(&mut self) {
        // Reset the thread-local so the isolation never leaks beyond this test.
        test_data_dir_with(|slot| *slot = None);
    }
}

fn test_data_dir_with<F: FnOnce(&mut Option<PathBuf>)>(f: F) {
    TEST_DATA_DIR.with(|cell| f(&mut cell.borrow_mut()))
}

thread_local! {
    static TEST_DATA_DIR: std::cell::RefCell<Option<PathBuf>> = std::cell::RefCell::new(None);
}

/// Point the store at `dir` (holding the guard) for the duration of the test.
/// Dropping the returned guard resets the thread-local to `None`.
pub(crate) fn set_test_data_dir(dir: &std::path::Path) -> TestDataDirGuard {
    test_data_dir_with(|slot| *slot = Some(dir.to_path_buf()));
    TestDataDirGuard
}

/// The per-thread override, if one is active.
pub(crate) fn test_data_dir() -> Option<PathBuf> {
    let mut result = None;
    test_data_dir_with(|slot| result = slot.clone());
    result
}
