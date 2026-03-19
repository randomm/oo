// SPDX-License-Identifier: Apache-2.0
pub mod classify;
pub mod commands;
pub mod error;
pub mod exec;
pub mod init;
pub mod learn;
pub mod pattern;
pub mod session;
pub mod store;

// CLI internals - hidden from documentation but accessible to binary crate
#[doc(hidden)]
pub mod help;
#[doc(hidden)]
pub mod util;

// Re-exports for library users
pub use classify::{Classification, classify};
pub use error::Error;
pub use exec::CommandOutput;
pub use pattern::{Pattern, builtins, load_user_patterns};
pub use store::{SessionMeta, Store};

// CLI internals - re-exported for binary crate but hidden from documentation
#[doc(hidden)]
pub use commands::{
    Action, InitFormat, check_and_clear_learn_status, classify_with_refs, cmd_forget, cmd_help,
    cmd_init, cmd_learn, cmd_patterns, cmd_patterns_in, cmd_recall, cmd_run, parse_action,
    try_index, write_learn_status,
};

// Internal type re-exported for learn module
#[doc(hidden)]
pub use pattern::FailureSection;
