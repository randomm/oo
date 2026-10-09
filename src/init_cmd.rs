//! `oo init` argument parsing and dispatch.
//!
//! Split out of `commands.rs` to keep that module under the 500-line cap.

use crate::error::Error;
use crate::init::InitFormat;
use crate::{init, init_pi};

/// Resolved mode for `oo init`: the legacy `--format` value or the new
/// `--agent` target (issue #171).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitMode {
    /// `--format claude`/`generic` (or plain `oo init`) — behaviour unchanged.
    Format(InitFormat),
    /// `--agent pi [--global]` — install the pi extension.
    Pi {
        /// Install user-wide instead of per project.
        global: bool,
    },
    /// `--agent claude-code [--global]` — install the Claude Code hook (issue #172).
    ClaudeCode {
        /// Install user-wide instead of per project.
        global: bool,
    },
}

/// Agents supported by `oo init --agent`.
pub const SUPPORTED_AGENTS: &[&str] = &["pi", "claude-code"];

/// Parse the trailing args of `oo init` into an [`InitMode`].
///
/// Single pass over the args; each recognised flag is handled inline and no
/// value is silently discarded. `--agent` and `--format` are mutually
/// exclusive (issue #171, operator decision 1): passing both is a
/// parse-time error before anything is written. `--agent pi` installs the pi
/// extension, with `--global` writing to the user-level extensions directory
/// (`--global` without `--agent` is an error — there is no legacy global
/// install). `--agent`'s value is validated against [`SUPPORTED_AGENTS`]:
/// `pi` works, `claude-code` is supported by name but not yet implemented
/// (a sibling ticket ships it), and anything else errors. On the pure
/// `--format` path (no `--agent` at all), the original args are handed to
/// [`parse_init_format`], so `oo init` and `oo init --format claude|generic`
/// behave as before. An unknown format value, an unknown option, or a stray
/// positional is an error — nothing is written.
pub(crate) fn parse_init_mode(args: &[String]) -> Result<InitMode, String> {
    let supported = SUPPORTED_AGENTS.join(", ");
    let mut agent: Option<String> = None;
    let mut global = false;
    // Outer Option: `--format` was given. Inner: its value (None when missing).
    let mut format: Option<Option<String>> = None;
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--agent" => {
                let Some(value) = iter.next().filter(|v| !v.starts_with("--")) else {
                    // No value, or the next token is itself a flag (e.g.
                    // `--agent --global`) — report it as missing.
                    return Err(format!(
                        "--agent requires a value; supported agents: {supported}"
                    ));
                };
                agent = Some(value.clone());
            }
            "--global" => global = true,
            // A value is required; a following flag-like token is not consumed.
            // A repeated `--format` is rejected: last-wins would silently drop
            // an earlier value (possibly invalid) without validating it.
            "--format" => {
                if format.is_some() {
                    return Err("--format given more than once".to_string());
                }
                format = Some(match iter.peek() {
                    Some(v) if !v.starts_with('-') => {
                        let value = (*v).clone();
                        iter.next();
                        Some(value)
                    }
                    _ => None,
                });
            }
            other if other.starts_with('-') => {
                return Err(format!(
                    "init: unknown option '{other}' (try: oo init --help)"
                ));
            }
            other => {
                return Err(format!(
                    "init: unexpected argument '{other}' (try: oo init --help)"
                ));
            }
        }
    }
    let Some(agent) = agent else {
        // `--global` without `--agent` is always an error, regardless of
        // whether `--format` is also present — there is no legacy global
        // install, so `--global` only makes sense with `--agent`.
        if global {
            return Err("--global requires --agent".to_string());
        }
        return match format {
            None => Ok(InitMode::Format(InitFormat::Claude)),
            Some(None) => {
                Err("--format requires a value; supported formats: claude, generic".to_string())
            }
            Some(Some(value)) => parse_init_format(&value).map(InitMode::Format),
        };
    };
    if format.is_some() {
        // Mutual exclusion regardless of flag order or value.
        return Err("--agent and --format cannot be used together".to_string());
    }
    if !SUPPORTED_AGENTS.contains(&agent.as_str()) {
        return Err(format!(
            "unknown --agent value '{agent}'; supported agents: {supported}"
        ));
    }
    match agent.as_str() {
        "pi" => Ok(InitMode::Pi { global }),
        "claude-code" => Ok(InitMode::ClaudeCode { global }),
        _ => unreachable!("agent validated against SUPPORTED_AGENTS above"),
    }
}

/// Map a `--format <value>` to an [`InitFormat`].
///
/// Recognised values: `claude`, `generic`. Anything else is an error, so a
/// typo is rejected instead of silently writing `.claude/hooks.json`.
fn parse_init_format(value: &str) -> Result<InitFormat, String> {
    match value {
        "generic" => Ok(InitFormat::Generic),
        "claude" => Ok(InitFormat::Claude),
        other => Err(format!(
            "init: unknown --format value '{other}' (supported: claude, generic)"
        )),
    }
}

/// Run the install selected by `mode`; returns the process exit code.
pub fn cmd_init(mode: InitMode) -> i32 {
    match &mode {
        InitMode::Format(format) => match init::run(*format) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("oo: {e}");
                1
            }
        },
        InitMode::Pi { global } => {
            let Ok(cwd) = std::env::current_dir() else {
                eprintln!("oo: cannot determine working directory");
                return 1;
            };
            match init_pi::run(&cwd, *global) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("oo: {e}");
                    1
                }
            }
        }
        InitMode::ClaudeCode { global } => match std::env::current_dir().map_err(Error::from) {
            Err(e) => {
                eprintln!("oo: {e}");
                1
            }
            Ok(cwd) => match crate::hook::claude_settings_path(&cwd, *global) {
                Err(e) => {
                    eprintln!("oo: {e}");
                    1
                }
                Ok(path) => match crate::hook::merge_oo_hook(&path) {
                    Ok(_) => {
                        println!(
                            "Installed `oo hook claude` PreToolUse hook in {}",
                            path.display()
                        );
                        println!("Uninstall: remove the `oo hook claude` entry from that file.");
                        0
                    }
                    Err(e) => {
                        eprintln!("oo: {e}");
                        1
                    }
                },
            },
        },
    }
}
