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

/// Agents supported by `oo init --agent`. Keep in sync with the match in
/// [`parse_init_mode`]; `init_args_tests` checks every entry parses.
pub const SUPPORTED_AGENTS: &[&str] = &["pi", "claude-code"];

/// A `--format` occurrence: given bare (no value follows), or with a value.
#[derive(Debug, PartialEq, Eq)]
enum FormatArg {
    Bare,
    Value(String),
}

/// Parse the trailing args of `oo init` into an [`InitMode`].
///
/// Single pass over the args. Every flag may appear at most once: a repeated
/// `--agent`, `--format` or `--global` is a parse-time error, so no earlier
/// value is silently overridden. `--agent` and `--format` are mutually
/// exclusive. `--global` requires `--agent`. `--agent` and `--format` need a
/// value, and a following dash-token is never consumed as that value. Values
/// are validated before anything is written; an unknown option or stray
/// positional is also an error. Plain `oo init` (no flags) selects the Claude
/// format.
pub(crate) fn parse_init_mode(args: &[String]) -> Result<InitMode, String> {
    let supported = SUPPORTED_AGENTS.join(", ");
    let mut agent: Option<String> = None;
    let mut global = false;
    let mut format: Option<FormatArg> = None;
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--agent" => {
                if agent.is_some() {
                    return Err("--agent given more than once".to_string());
                }
                let Some(value) = iter.next().filter(|v| !v.starts_with('-')) else {
                    // No value, or the next token is itself a flag (e.g.
                    // `--agent --global`) — report it as missing.
                    return Err(format!(
                        "--agent requires a value; supported agents: {supported}"
                    ));
                };
                agent = Some(value.clone());
            }
            "--global" => {
                if global {
                    return Err("--global given more than once".to_string());
                }
                global = true;
            }
            "--format" => {
                if format.is_some() {
                    return Err("--format given more than once".to_string());
                }
                format = Some(match iter.peek() {
                    Some(v) if !v.starts_with('-') => {
                        let value = (*v).clone();
                        iter.next();
                        FormatArg::Value(value)
                    }
                    _ => FormatArg::Bare,
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
        // `--global` only makes sense with `--agent`: there is no legacy
        // global install.
        if global {
            return Err("--global requires --agent".to_string());
        }
        return match format {
            None => Ok(InitMode::Format(InitFormat::Claude)),
            Some(FormatArg::Bare) => {
                Err("--format requires a value; supported formats: claude, generic".to_string())
            }
            Some(FormatArg::Value(value)) => parse_init_format(&value).map(InitMode::Format),
        };
    };
    if format.is_some() {
        // Mutual exclusion regardless of flag order or value.
        return Err("--agent and --format cannot be used together".to_string());
    }
    match agent.as_str() {
        "pi" => Ok(InitMode::Pi { global }),
        "claude-code" => Ok(InitMode::ClaudeCode { global }),
        _ => Err(format!(
            "unknown --agent value '{agent}'; supported agents: {supported}"
        )),
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
            "oo init: unsupported format '{other}' (supported: claude, generic)"
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
