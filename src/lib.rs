// SPDX-License-Identifier: Apache-2.0
pub mod classify;
pub mod commands;
pub mod error;
pub mod exec;
pub mod help;
pub mod init;
pub mod learn;
pub mod pattern;
pub mod session;
pub mod store;
pub mod util;

pub use commands::{
    Action, InitFormat, check_and_clear_learn_status, classify_with_refs, cmd_forget, cmd_help,
    cmd_init, cmd_learn, cmd_patterns, cmd_patterns_in, cmd_recall, cmd_run, parse_action,
    try_index, write_learn_status,
};
