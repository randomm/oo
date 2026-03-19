// SPDX-License-Identifier: Apache-2.0
#![warn(missing_docs)]

//! `oo` (double-o) — a context-efficient command runner for AI coding agents.
//!
//! This library helps AI agents run shell commands efficiently by classifying output
//! and reducing context usage. Commands are executed, their output is analyzed, and
//! results are compressed using pattern matching and intelligent categorization.
//!
//! # Core Concepts
//!
//! - **Classification**: Commands are categorized into four tiers based on success/failure
//!   and output size. Small successful outputs pass through verbatim, while large outputs
//!   are pattern-matched to extract terse summaries or indexed for later recall.
//! - **Patterns**: Regular expressions define how to extract summaries from command output.
//!   Built-in patterns exist for common tools (pytest, cargo test, npm test, etc.), and
//!   user-defined patterns can be loaded from TOML files in `~/.config/oo/patterns/`.
//! - **Storage**: Large unpatterned outputs are stored in a searchable database (SQLite by
//!   default, with optional Vipune semantic search). Stored outputs can be recalled with
//!   full-text search.
//! - **Categories**: Commands are auto-detected as Status (tests, builds, linters),
//!   Content (git show, diff, cat), Data (git log, ls, gh), or Unknown. This determines
//!   default behavior when no pattern matches.
//!
//! # Example
//!
//! ```
//! use oo::{classify, CommandOutput, Pattern};
//! use oo::pattern::builtins;
//!
//! // Run a command
//! let args = vec!["echo".into(), "hello".into()];
//! let output = oo::exec::run(&args).unwrap();
//!
//! // Classify the output
//! let command = "echo hello";
//! let patterns = builtins(); // or load_user_patterns(&path)
//! let result = classify(&output, command, &patterns);
//!
//! match result {
//!     oo::Classification::Passthrough { output } => {
//!         println!("Output: {}", output);
//!     }
//!     oo::Classification::Success { label, summary } => {
//!         println!("✓ {}: {}", label, summary);
//!     }
//!     oo::Classification::Failure { label, output } => {
//!         println!("✗ {}: {}", label, output);
//!     }
//!     oo::Classification::Large { label, size, .. } => {
//!         println!("Indexed: {} ({} bytes)", label, size);
//!     }
//! }
//! ```

pub mod classify;
#[doc(hidden)]
#[allow(missing_docs)]
pub mod commands;
pub mod error;
pub mod exec;
#[doc(hidden)]
#[allow(missing_docs)]
pub mod init;
pub mod learn;
pub mod pattern;
pub mod session;
pub mod store;

// CLI internals - hidden from documentation but accessible to binary crate
#[doc(hidden)]
#[allow(missing_docs)]
pub mod help;
#[doc(hidden)]
#[allow(missing_docs)]
pub mod util;

// Re-exports for library users
pub use classify::{Classification, classify};
pub use error::Error;
pub use exec::CommandOutput;
pub use pattern::{Pattern, builtins, load_user_patterns};
pub use store::{SessionMeta, Store};

// CLI internals - re-exported for binary crate but hidden from documentation
#[doc(hidden)]
#[allow(missing_docs)]
pub use commands::{
    Action, InitFormat, check_and_clear_learn_status, classify_with_refs, cmd_forget, cmd_help,
    cmd_init, cmd_learn, cmd_patterns, cmd_patterns_in, cmd_recall, cmd_run, parse_action,
    try_index, write_learn_status,
};

// Internal type re-exported for learn module
#[doc(hidden)]
pub use pattern::FailureSection;
