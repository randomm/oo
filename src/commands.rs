use humansize::{BINARY, format_size};

use crate::classify::Classification;
pub use crate::init::InitFormat;
use crate::store::SessionMeta;
use crate::util::{format_age, now_epoch};
use crate::{classify, exec, help, init, learn, pattern, session, store};

pub enum Action {
    Run(Vec<String>),
    Recall(String),
    Forget,
    Learn(Vec<String>),
    Version,
    Help(Option<String>),
    Init(InitFormat),
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

pub fn parse_action(args: &[String]) -> Action {
    match args.first().map(|s| s.as_str()) {
        None => Action::Help(None),
        Some("recall") => Action::Recall(args[1..].join(" ")),
        Some("forget") => Action::Forget,
        Some("learn") => Action::Learn(args[1..].to_vec()),
        Some("version") => Action::Version,
        // `oo help <cmd>` — look up cheat sheet; `oo help` alone shows usage
        Some("help") => Action::Help(args.get(1).cloned()),
        Some("init") => Action::Init(parse_init_format(&args[1..])),
        _ => Action::Run(args.to_vec()),
    }
}

pub fn cmd_run(args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!("oo: no command specified");
        return 1;
    }

    // Load patterns: user patterns first (override), then builtins
    let user_patterns = pattern::load_user_patterns(&learn::patterns_dir());
    let builtin_patterns = pattern::builtins();
    let mut all_patterns: Vec<&pattern::Pattern> = Vec::new();
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

    if merged.len() <= 4096 {
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

    let size = merged.len();
    Classification::Large {
        label: lbl,
        output: merged,
        size,
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

pub fn cmd_learn(args: &[String]) -> i32 {
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

    // Spawn background learn process
    if let Err(e) = learn::spawn_background(&command, &merged, exit_code) {
        eprintln!("oo: learn failed: {e}");
    } else {
        eprintln!("  [learning pattern for \"{}\"]", classify::label(&command));
    }

    exit_code
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

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
