use std::path::Path;

use humansize::{BINARY, format_size};
use std::io::Write;

use crate::classify::Classification;
pub use crate::init::InitFormat;
use crate::store::SessionMeta;
use crate::util::now_epoch;
use crate::{classify, commands_patterns, exec, help, init, learn, pattern, session, store};

pub enum Action {
    Run(Vec<String>),
    Recall { query: String, full: bool },
    Forget,
    Learn(Vec<String>, Option<String>),
    Version,
    Help(Option<String>),
    Init(InitFormat),
    Patterns,
}

/// Parse `--format <value>` from the remaining init args.
///
/// Recognised values: `claude` (default), `generic`.
/// Unknown values emit a warning to stderr and fall back to Claude.
fn parse_init_format(args: &[String]) -> InitFormat {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--format" {
            return match iter.next().map(|s| s.as_str()) {
                Some("generic") => InitFormat::Generic,
                Some("claude") | None => InitFormat::Claude,
                Some(other) => {
                    eprintln!(
                        "oo: unknown --format value '{}', defaulting to claude",
                        other
                    );
                    InitFormat::Claude
                }
            };
        }
    }
    InitFormat::Claude
}

/// Parse `oo recall` arguments, extracting optional `--full` flag.
///
/// `--full` is stripped from ANY position in the trailing args (consistent
/// with `parse_learn_action`'s `--hint` handling). Searching for the literal
/// string `--full` in a query is unsupported; document that in help.
fn parse_recall_action(args: &[String]) -> Action {
    let query: String = args
        .iter()
        .filter(|a| a.as_str() != "--full")
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let full = args.iter().any(|a| a.as_str() == "--full");
    Action::Recall { query, full }
}

/// Parse `oo learn` arguments, extracting optional `--hint <text>` flag.
///
/// Returns (args_without_hint, hint_text). The hint text is removed from the
/// command args so it doesn't interfere with the actual command being run.
fn parse_learn_action(args: &[String]) -> Action {
    let mut result: Vec<String> = Vec::new();
    let mut hint: Option<String> = None;
    let mut iter = args.iter().peekable();

    while let Some(arg) = iter.next() {
        if arg == "--hint" {
            // Take the next argument as the hint text, but only if it's not a flag
            // If the next arg starts with '-', treat it as a command argument, not hint text
            if let Some(hint_text) = iter.next() {
                if !hint_text.starts_with('-') {
                    hint = Some(hint_text.clone());
                } else {
                    // The next arg is a flag, treat it as part of the command
                    result.push(hint_text.clone());
                }
            }
            // If no hint text after --hint, emit a warning and treat as no hint
        } else {
            result.push(arg.clone());
        }
    }

    Action::Learn(result, hint)
}

pub fn parse_action(args: &[String]) -> Action {
    match args.first().map(|s| s.as_str()) {
        None => Action::Help(None),
        Some("recall") => parse_recall_action(&args[1..]),
        Some("forget") => Action::Forget,
        Some("learn") => parse_learn_action(&args[1..]),
        Some("version") => Action::Version,
        // `oo help <cmd>` — look up cheat sheet; `oo help` alone shows usage
        Some("help") => Action::Help(args.get(1).cloned()),
        Some("init") => Action::Init(parse_init_format(&args[1..])),
        Some("patterns") => Action::Patterns,
        _ => Action::Run(args.to_vec()),
    }
}

/// Run a command via [`exec::run`], load the pattern set used by `cmd_run`, classify the output,
/// and render it. Returns the exit code and the classification so tests can inspect the
/// classification directly (rendering happens here so the normal `cmd_run` path is exercised).
///
/// The classification is `None` on the two pre-classification error paths
/// (no command given, or exec failure) — there is no output to classify, so
/// no `Classification` value is fabricated.
pub fn run_command_args(args: &[String]) -> (i32, Option<Classification>) {
    if args.is_empty() {
        eprintln!("oo: no command specified");
        return (1, None);
    }

    // Load patterns: project-local first, then user config, then builtins.
    // First match wins, so project patterns override user patterns override builtins.
    let mut all_patterns = load_project_patterns();
    all_patterns.extend(pattern::load_user_patterns(&learn::patterns_dir()));
    all_patterns.extend_from_slice(pattern::builtins());

    // Run command
    let output = match exec::run(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("oo: {e}");
            return (1, None);
        }
    };

    let exit_code = output.exit_code;
    let command = args.join(" ");

    // Classify
    let merged = output.merged_lossy();
    let classification = classify::classify(&output, &command, &all_patterns);

    // Print result
    render_classification(&classification, &command, merged.len());

    (exit_code, Some(classification))
}

/// Run a command via [`exec::run`], classify the output, and render it. Returns the exit code.
pub fn cmd_run(args: &[String]) -> i32 {
    run_command_args(args).0
}

/// Build the `[saved {humansize}]` suffix for a compressed indicator line.
///
/// `original_bytes` is the merged output size in bytes (`merged_lossy().len()`);
/// `indicator_bytes` is the byte length of the indicator line EXCLUDING the
/// savings suffix (call sites pass `line.len()` before the suffix is
/// appended), so the reported figure overstates displayed bytes by the
/// suffix's own length (~15 B — immaterial at the sizes where a suffix can
/// appear). Returns `None` when the saving is ≤ [`classify::MIN_SAVINGS`]
/// (i.e. the suffix appears only when `saved > MIN_SAVINGS`; saturating
/// arithmetic makes underflow impossible), so the caller prints the line
/// unchanged. Only the indicator line counts as displayed — the filtered
/// output lines printed after it (Failure arm) are not included.
pub fn savings_suffix(original_bytes: usize, indicator_bytes: usize) -> Option<String> {
    let saved = original_bytes.saturating_sub(indicator_bytes);
    (saved > classify::MIN_SAVINGS).then(|| format!(" [saved {}]", format_size(saved, BINARY)))
}

/// Print a [`Classification`] to stdout using the shared display path.
///
/// `original_size` is the merged output size in bytes, used to append the
/// savings suffix to the Success and Failure indicator lines via
/// [`savings_suffix`]. The Large tier indexes the output via [`try_index`]
/// and falls back to [`classify::smart_truncate`] on indexing failure.
/// Both `cmd_run` and `cmd_learn` use this so the arms cannot drift apart.
pub fn render_classification(classification: &Classification, command: &str, original_size: usize) {
    match classification {
        Classification::Failure { label, output } => {
            let line = format!("\u{2717} {label}");
            // The `\n` in the payload plus `println!`'s own newline is a
            // deliberate BLANK LINE between the indicator and the error
            // body — the suffix (if any) lands on the indicator line only,
            // and the blank line survives with or without it.
            println!(
                "{line}{}\n",
                savings_suffix(original_size, line.len()).unwrap_or_default()
            );
            println!("{output}");
        }
        Classification::Passthrough { output } => {
            print!("{output}");
        }
        Classification::Success { label, summary } => {
            let line = if summary.is_empty() {
                format!("\u{2713} {label}")
            } else {
                format!("\u{2713} {label} ({summary})")
            };
            println!(
                "{line}{}",
                savings_suffix(original_size, line.len()).unwrap_or_default()
            );
        }
        Classification::Bounded {
            label,
            output,
            display,
            size,
            ..
        } => {
            // SECURITY (residual risk, pre-existing): the framing line printed
            // below (`● {label} (output truncated: ...)`) is byte-predictable
            // and is followed by attacker-controlled `display`, which could
            // embed a byte-identical copy of it — so "the framing line comes
            // first" is a convention, not an enforceable guarantee, and an
            // agent that greps for the framing pattern can't distinguish the
            // host-authored line from a forged one. The Large arm's
            // `● ... (indexed ...)` line has the same property. NOT fixed
            // here (would require changing the output format); document the
            // residual risk at the point of use instead.
            //
            // Best-effort index the full output for recall; the display is
            // always printed — the byte-bounded head+tail slice IS the point
            // of this arm (issue #148). The truncation marker in `display`
            // makes it detectable as bounded output, but it lives inside
            // attacker-controlled output, so the host also prints its own
            // framing line carrying the authoritative size (mirrors the Large
            // arm) — an agent must never be told to recall data that was not
            // indexed.
            let indexed = try_index(command, output);
            let human_size = format_size(*size, BINARY);
            if indexed {
                println!(
                    "\u{25CF} {label} (output truncated: {human_size} total \u{2192} use `oo recall` to query)"
                );
            } else {
                // Indexing failed: do NOT advertise `oo recall` — the display
                // is all the agent gets, and the stderr note keeps the
                // failure visible instead of swallowed.
                eprintln!(
                    "oo: warning: could not index output for recall — full output LOST (not recoverable); what follows is a truncated slice only"
                );
                println!(
                    "\u{25CF} {label} (output truncated: {human_size} total — NOT indexed, recall unavailable)"
                );
            }
            print!("{display}");
        }
        Classification::Large {
            label,
            output,
            size,
            ..
        } => {
            // Index into store
            let indexed = try_index(command, output);
            let human_size = format_size(*size, BINARY);
            if indexed {
                println!(
                    "\u{25CF} {label} (indexed {human_size} \u{2192} use `oo recall` to query)"
                );
            } else {
                // Couldn't index, show truncated output instead
                let truncated = classify::smart_truncate(output);
                print!("{truncated}");
            }
        }
    }
}

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

    // Lazy TTL cleanup (best-effort)
    let _ = store.cleanup_stale(&project_id, 86400);

    store.index(&project_id, content, &meta).is_ok()
}

pub fn cmd_recall(query: &str, full: bool) -> i32 {
    crate::recall_display::cmd_recall(query, full)
}

pub fn cmd_forget() -> i32 {
    let mut store = match store::open() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("oo: {e}");
            return 1;
        }
    };

    let project_id = session::project_id();
    let sid = session::session_id();

    match store.delete_by_session(&project_id, &sid) {
        Ok(count) => {
            println!("Cleared session data ({count} entries)");
            0
        }
        Err(e) => {
            eprintln!("oo: {e}");
            1
        }
    }
}

pub fn cmd_learn(args: &[String], hint: Option<&str>) -> i32 {
    if args.is_empty() {
        eprintln!("oo: learn requires a command");
        return 1;
    }

    // Run the command normally first
    let output = match exec::run(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("oo: {e}");
            return 1;
        }
    };

    let exit_code = output.exit_code;
    let command = args.join(" ");
    let merged = output.merged_lossy();
    let original_size = merged.len();

    // Show normal oo output first (builtins only — project/user patterns are
    // intentionally not consulted here; see issue #151)
    let classification = classify::classify(&output, &command, pattern::builtins());
    render_classification(&classification, &command, original_size);

    // Print provider before spawning so the user sees it in the foreground
    let config = learn::load_learn_config().unwrap_or_else(|e| {
        eprintln!("oo: config error: {e}");
        learn::LearnConfig::default()
    });
    eprintln!(
        "  [learning pattern for \"{}\" ({})]",
        classify::label(&command),
        config.provider
    );

    // Spawn background learn process
    if let Err(e) = learn::spawn_background(&command, &merged, exit_code, hint) {
        eprintln!("oo: learn failed: {e}");
    }

    exit_code
}

/// Write a one-line status entry to the learn status file.
///
/// Called by the background process after successfully saving a pattern so
/// the NEXT foreground invocation can display the result.
pub fn write_learn_status(
    status_path: &Path,
    cmd_name: &str,
    pattern_path: &Path,
) -> Result<(), std::io::Error> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(status_path)?;
    writeln!(
        file,
        "learned pattern for {} → {}",
        cmd_name,
        pattern_path.display()
    )
}

/// Write a one-line failure entry to the learn status file.
///
/// Called by the background process when `run_learn` returns `Err`, so the
/// NEXT foreground invocation can display the error.
pub fn write_learn_status_failure(
    status_path: &Path,
    cmd_name: &str,
    error_msg: &str,
) -> Result<(), std::io::Error> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(status_path)?;
    let first_line = error_msg.lines().next().unwrap_or(error_msg);
    writeln!(file, "FAILED {cmd_name}: {first_line}")
}

/// Check for a pending learn-status file, print its contents to stderr, then
/// delete the file.  Called early in each foreground invocation.
pub fn check_and_clear_learn_status(status_path: &Path) {
    if let Ok(content) = std::fs::read_to_string(status_path) {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("FAILED ") {
                // Format: "FAILED cmd-name: error message"
                if let Some((cmd, msg)) = rest.split_once(": ") {
                    eprintln!("oo: learn failed for {cmd} — {msg}");
                } else {
                    eprintln!("oo: learn failed — {rest}");
                }
            } else {
                eprintln!("oo: {line}");
            }
        }
        let _ = std::fs::remove_file(status_path);
    }
}

/// List patterns from both project-local and user config directories.
pub fn cmd_patterns() -> i32 {
    self::commands_patterns::cmd_patterns()
}

/// List learned pattern files from a single directory (legacy test helper).
pub fn cmd_patterns_in(dir: &Path) -> i32 {
    self::commands_patterns::cmd_patterns_in(dir)
}

/// Print pattern entries from a single directory, returning true if any were found.
///
/// Each line is printed with a two-space indent so callers can add section headers.
pub fn list_patterns_in(dir: &Path) -> bool {
    self::commands_patterns::list_patterns_in(dir)
}

pub fn cmd_help(cmd: &str) -> i32 {
    match help::lookup(cmd) {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(e) => {
            eprintln!("oo: {e}");
            1
        }
    }
}

pub fn cmd_init(format: InitFormat) -> i32 {
    match init::run(format) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("oo: {e}");
            1
        }
    }
}

/// Load project-local patterns from `<git-root>/.oo/patterns/`.
///
/// Returns an empty vec when cwd cannot be determined or the directory
/// does not exist (gracefully handled by `load_user_patterns`).
pub fn load_project_patterns() -> Vec<pattern::Pattern> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    pattern::load_user_patterns(&init::project_patterns_dir(&cwd))
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
