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
//!   Built-in patterns exist for common tools (pytest, cargo test, npm test, etc.).
//!   Custom patterns can be loaded from project-local `<git-root>/.oo/patterns/` or
//!   user-global TOML files in the user patterns directory (project patterns take precedence).
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
//! use double_o::{classify, Classification, CommandOutput, Pattern};
//! use double_o::pattern::builtins;
//!
//! // Run a command
//! let args = vec!["echo".into(), "hello".into()];
//! let output = double_o::exec::run(&args).unwrap();
//!
//! // Classify the output
//! let command = "echo hello";
//! let patterns = builtins(); // or load_user_patterns(&path)
//! let result = classify(&output, command, &patterns);
//!
//! match result {
//!     Classification::Passthrough { output } => {
//!         println!("Output: {}", output);
//!     }
//!     Classification::Success { label, summary } => {
//!         println!("✓ {}: {}", label, summary);
//!     }
//!     Classification::Failure { label, output, .. } => {
//!         println!("✗ {}: {}", label, output);
//!     }
//!     Classification::Bounded { label, size, .. } => {
//!         println!("Bounded: {} ({} bytes)", label, size);
//!     }
//!     Classification::Large { label, size, .. } => {
//!         println!("Indexed: {} ({} bytes)", label, size);
//!     }
//! }
//! ```

/// API key presence check for `oo learn`.
pub mod api_key;
/// OS-string argv helpers: lossy display and UTF-8-or-reject conversion (issue #213).
#[doc(hidden)]
pub mod argv;
#[cfg(test)]
#[path = "argv_tests.rs"]
mod argv_tests;
/// Command output classification and intelligent truncation.
pub mod classify;
/// Byte-bounded head+tail truncation, re-exported from [`classify`].
mod classify_bounded;
#[doc(hidden)]
#[allow(missing_docs)]
pub mod commands;
/// Config and data directory resolution (`OO_CONFIG_DIR`, `OO_DATA_DIR`, or the platform dirs).
mod config;
/// Savings suffix for compressed indicator lines (shared by the render arms).
mod display;
/// Error types for oo operations.
pub mod error;
/// Command execution and output capture.
pub mod exec;
/// Failure-tier display and recall notice (used by `render_classification`).
mod failure_display;
#[doc(hidden)]
#[allow(missing_docs)]
pub mod help;
/// Indexing of raw command output for `oo recall` (shared by the render arms).
mod indexing;
#[doc(hidden)]
#[allow(missing_docs)]
pub mod init;
#[doc(hidden)]
#[allow(missing_docs)]
pub mod init_pi;
/// LLM-powered pattern learning.
pub mod learn;
mod learn_payload;
#[doc(hidden)]
pub mod learn_status;
/// Pattern matching and output compression.
pub mod pattern;
/// Canonical pattern-set assembly (project/user/builtins precedence) shared
/// by the dispatch layer and the rewrite/hook feature modules.
#[doc(hidden)]
pub mod pattern_load;
/// Page-based project drain and stale cleanup for backends without bulk
/// delete or pagination offset (Vipune adapter).
#[cfg(any(feature = "vipune-store", test))]
mod project_drain;
/// Retention of indexed output (`[store] retention` in config.toml).
mod retention;
/// Session tracking and management.
pub mod session;
/// Storage backends for indexed output.
pub mod store;

/// Claude Code PreToolUse hook processor + settings.json installer (issue #172).
#[doc(hidden)]
pub mod hook;
pub mod recall_display;
pub mod rewrite;

pub mod commands_patterns;
#[cfg(test)]
#[path = "init_args_tests.rs"]
mod init_args_tests;
pub mod init_cmd;
pub mod learn_prompt;
mod learn_url;
pub mod learn_utils;
/// Static usage text for reserved subcommands (`--help` and `oo help <sub>`).
#[doc(hidden)]
pub mod usage;
#[cfg(test)]
#[path = "usage_tests.rs"]
mod usage_tests;

// CLI internals - hidden from documentation but accessible to binary crate
#[doc(hidden)]
#[allow(missing_docs)]
pub mod util;

// Re-exports for library users
pub use classify::{Classification, classify};
pub use config::warn_ignored_overrides;
pub use error::Error;
pub use exec::CommandOutput;
pub use pattern::{Pattern, builtins, load_user_patterns};
pub use store::{SessionMeta, Store};

// CLI internals - re-exported for binary crate but hidden from documentation
#[doc(hidden)]
pub use commands::{
    Action, InitFormat, InitMode, cmd_forget, cmd_help, cmd_hook, cmd_init, cmd_learn,
    cmd_patterns, cmd_patterns_in, cmd_recall, cmd_rewrite, cmd_run, parse_action,
    render_classification, try_index,
};

// Internal type re-exported for learn module
#[doc(hidden)]
pub use pattern::FailureSection;
