use std::path::Path;

use humansize::{BINARY, format_size};
use std::io::Write;

use crate::classify::Classification;
pub use crate::init::InitFormat;
use crate::store::SessionMeta;
use crate::util::{format_age, now_epoch};
use crate::{classify, commands_patterns, exec, help, init, learn, pattern, session, store};

pub enum Action {
    Run(Vec<String>),
    Recall(String),
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
        Some("recall") => Action::Recall(args[1..].join(" ")),
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

pub fn cmd_run(args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!("oo: no command specified");
        return 1;
    }

    // Load patterns: project-local first, then user config, then builtins.
    // First match wins, so project patterns override user patterns override builtins.
    let project_patterns = load_project_patterns();
    let user_patterns = pattern::load_user_patterns(&learn::patterns_dir());
    let builtin_patterns = pattern::builtins();
    let mut all_patterns: Vec<&pattern::Pattern> = Vec::new();
    for p in &project_patterns {
        all_patterns.push(p);
    }
    for p in &user_patterns {
        all_patterns.push(p);
    }
    for p in builtin_patterns {
        all_patterns.push(p);
    }

    // Run command
    let output = match exec::run(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("oo: {e}");
            return 1;
        }
    };

    let exit_code = output.exit_code;
    let command = args.join(" ");

    let combined: Vec<&pattern::Pattern> = all_patterns;
    let classification = classify_with_refs(&output, &command, &combined);

    // Print result
    match &classification {
        Classification::Failure { label, output } => {
            println!("\u{2717} {label}\n");
            println!("{output}");
        }
        Classification::Passthrough { output } => {
            print!("{output}");
        }
        Classification::Success { label, summary } => {
            if summary.is_empty() {
                println!("\u{2713} {label}");
            } else {
                println!("\u{2713} {label} ({summary})");
            }
        }
        Classification::Large {
            label,
            output,
            size,
            ..
        } => {
            // Index into store
            let indexed = try_index(&command, output);
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

    exit_code
}

/// Classify using a slice of pattern references.
pub fn classify_with_refs(
    output: &exec::CommandOutput,
    command: &str,
    patterns: &[&pattern::Pattern],
) -> Classification {
    let merged = output.merged_lossy();
    let lbl = classify::label(command);

    if output.exit_code != 0 {
        let filtered = match pattern::find_matching_ref(command, patterns) {
            Some(pat) => {
                if let Some(failure) = &pat.failure {
                    pattern::extract_failure(failure, &merged)
                } else {
                    classify::smart_truncate(&merged)
                }
            }
            _ => classify::smart_truncate(&merged),
        };
        return Classification::Failure {
            label: lbl,
            output: filtered,
        };
    }

    if merged.len() <= classify::SMALL_THRESHOLD {
        return Classification::Passthrough { output: merged };
    }

    if let Some(pat) = pattern::find_matching_ref(command, patterns) {
        if let Some(sp) = &pat.success {
            if let Some(summary) = pattern::extract_summary(sp, &merged) {
                return Classification::Success {
                    label: lbl,
                    summary,
                };
            }
        }
    }

    // Large, no pattern match — use category to determine behavior
    let category = classify::detect_category(command);
    match category {
        classify::CommandCategory::Status => {
            // Status commands: quiet success (empty summary)
            Classification::Success {
                label: lbl,
                summary: String::new(),
            }
        }
        classify::CommandCategory::Content | classify::CommandCategory::Unknown => {
            // Content and Unknown: always passthrough (never index)
            Classification::Passthrough { output: merged }
        }
        classify::CommandCategory::Data => {
            // Data: index for recall
            let size = merged.len();
            Classification::Large {
                label: lbl,
                output: merged,
                size,
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

pub fn cmd_recall(query: &str) -> i32 {
    if query.is_empty() {
        eprintln!("oo: recall requires a query");
        return 1;
    }

    let mut store = match store::open() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("oo: {e}");
            return 1;
        }
    };

    let project_id = session::project_id();

    match store.search(&project_id, query, 5) {
        Ok(results) if results.is_empty() => {
            println!("No results found.");
            0
        }
        Ok(results) => {
            for r in &results {
                if let Some(meta) = &r.meta {
                    let age = format_age(meta.timestamp);
                    println!("[session] {} ({age}):", meta.command);
                } else {
                    println!("[memory] project memory:");
                }
                // Indent content
                for line in r.content.lines() {
                    println!("  {line}");
                }
                println!();
            }
            0
        }
        Err(e) => {
            eprintln!("oo: {e}");
            1
        }
    }
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

    // Show normal oo output first
    let patterns = pattern::builtins();
    let classification = classify::classify(&output, &command, patterns);
    match &classification {
        Classification::Failure { label, output } => {
            println!("\u{2717} {label}\n");
            println!("{output}");
        }
        Classification::Passthrough { output } => {
            print!("{output}");
        }
        Classification::Success { label, summary } => {
            if summary.is_empty() {
                println!("\u{2713} {label}");
            } else {
                println!("\u{2713} {label} ({summary})");
            }
        }
        Classification::Large { label, size, .. } => {
            let human_size = format_size(*size, BINARY);
            println!("\u{25CF} {label} (indexed {human_size} \u{2192} use `oo recall` to query)");
        }
    }

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
