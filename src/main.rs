mod classify;
mod error;
mod exec;
mod learn;
mod pattern;
mod session;
mod store;

use clap::Parser;
use humansize::{BINARY, format_size};

use crate::classify::Classification;
use crate::store::SessionMeta;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "oo",
    version,
    about = "Context-efficient command runner for AI coding agents",
    long_about = "o̵̥̟͓̿͛̚õ̵̙͈̝̚\n\nContext-efficient command runner for AI coding agents."
)]
struct Cli {
    /// Arguments: a subcommand (recall/forget/learn/version) or a command to run
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

enum Action {
    Run(Vec<String>),
    Recall(String),
    Forget,
    Learn(Vec<String>),
    Version,
    Help,
}

fn parse_action(args: &[String]) -> Action {
    match args.first().map(|s| s.as_str()) {
        None => Action::Help,
        Some("recall") => Action::Recall(args[1..].join(" ")),
        Some("forget") => Action::Forget,
        Some("learn") => Action::Learn(args[1..].to_vec()),
        Some("version") => Action::Version,
        Some("_learn_bg") => {
            // Hidden internal subcommand for background learning
            if let Some(path) = args.get(1) {
                let _ = learn::run_background(path);
            }
            std::process::exit(0);
        }
        _ => Action::Run(args.to_vec()),
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // Intercept _learn_bg before clap parsing (it's a hidden internal command)
    let raw_args: Vec<String> = std::env::args().collect();
    if raw_args.get(1).is_some_and(|a| a == "_learn_bg") {
        if let Some(path) = raw_args.get(2) {
            let _ = learn::run_background(path);
        }
        std::process::exit(0);
    }

    let cli = Cli::parse();

    let exit_code = match parse_action(&cli.args) {
        Action::Help => {
            println!("o̵̥̟͓̿͛̚õ̵̙͈̝̚");
            println!();
            println!("Usage: oo <command> [args...]");
            println!("       oo recall <query>");
            println!("       oo forget");
            println!("       oo learn <command> [args...]");
            println!("       oo version");
            0
        }
        Action::Version => {
            println!("o̵̥̟͓̿͛̚õ̵̙͈̝̚ {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Action::Run(args) => cmd_run(&args),
        Action::Recall(query) => cmd_recall(&query),
        Action::Forget => cmd_forget(),
        Action::Learn(args) => cmd_learn(&args),
    };

    std::process::exit(exit_code);
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_run(args: &[String]) -> i32 {
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

    // Build a temporary slice of Pattern refs for classify
    // We need to convert &[&Pattern] to work with classify which takes &[Pattern]
    // Instead, build a combined vec just for classification
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
fn classify_with_refs(
    output: &exec::CommandOutput,
    command: &str,
    patterns: &[&pattern::Pattern],
) -> Classification {
    let merged = output.merged_lossy();
    let lbl = classify::label(command);

    if output.exit_code != 0 {
        let filtered = match pattern::find_matching_ref(command, patterns) {
            Some(pat) if pat.failure.is_some() => {
                pattern::extract_failure(pat.failure.as_ref().unwrap(), &merged)
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

fn try_index(command: &str, content: &str) -> bool {
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

fn open_store() -> Result<Box<dyn store::Store>, error::Error> {
    store::open()
}

fn cmd_recall(query: &str) -> i32 {
    if query.is_empty() {
        eprintln!("oo: recall requires a query");
        return 1;
    }

    let mut store = match open_store() {
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

fn cmd_forget() -> i32 {
    let mut store = match open_store() {
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

fn cmd_learn(args: &[String]) -> i32 {
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn format_age(timestamp: i64) -> String {
    let age = now_epoch() - timestamp;
    if age < 60 {
        format!("{age}s ago")
    } else if age < 3600 {
        format!("{}min ago", age / 60)
    } else if age < 86400 {
        format!("{}h ago", age / 3600)
    } else {
        format!("{}d ago", age / 86400)
    }
}
