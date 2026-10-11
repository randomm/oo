//! Indexing of raw command output for `oo recall`, shared by the render arms.
//!
//! Lives below `commands.rs` and `failure_display.rs` so neither needs to
//! import the other (`failure_display` must not depend on `commands`).

use crate::session;
use crate::store::{self, SessionMeta};
use crate::util::now_epoch;

/// Best-effort index `content` for `oo recall`. Returns `false` when the
/// store cannot be opened or the write fails; callers decide how to surface it.
pub fn try_index(command: &str, content: &str) -> bool {
    let mut store = match store::open() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let project_id = session::project_id();
    let meta = SessionMeta {
        source: "oo".into(),
        session: session::session_id(),
        command: command.into(),
        timestamp: now_epoch(),
    };

    crate::retention::purge_stale(store.as_mut(), &project_id);

    store.index(&project_id, content, &meta).is_ok()
}

/// [`try_index`], printing the shared "full output LOST" warning to stderr on
/// failure. Returns whether the content was indexed; the caller prints the
/// arm-specific stdout notice.
pub fn index_or_warn(command: &str, content: &str) -> bool {
    let indexed = try_index(command, content);
    if !indexed {
        eprintln!(
            "oo: warning: could not index output for recall — full output LOST (not recoverable); what follows is a truncated slice only"
        );
    }
    indexed
}
