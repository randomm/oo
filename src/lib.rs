pub mod classify;
pub mod error;
pub mod exec;
pub mod help;
pub mod init;
pub mod learn;
pub mod pattern;
pub mod session;
pub mod store;
pub mod util;

use humansize::{BINARY, format_size};

use crate::classify::Classification;
use crate::store::SessionMeta;
use crate::util::{format_age, now_epoch};

pub enum Action {
    Run(Vec<String>),
    Recall(String),
    Forget,
    Learn(Vec<String>),
    Version,
    Help(Option<String>),
    Init,
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
        Some("init") => Action::Init,
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

pub fn cmd_init() -> i32 {
    match init::run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("oo: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn test_parse_action_no_args_is_help() {
        assert!(matches!(parse_action(&[]), Action::Help(None)));
    }

    #[test]
    fn test_parse_action_recall_single_word() {
        let args = vec![s("recall"), s("cargo")];
        assert!(matches!(parse_action(&args), Action::Recall(q) if q == "cargo"));
    }

    #[test]
    fn test_parse_action_recall_multi_word_joins() {
        let args = vec![s("recall"), s("hello"), s("world")];
        assert!(matches!(parse_action(&args), Action::Recall(q) if q == "hello world"));
    }

    #[test]
    fn test_parse_action_recall_empty_query() {
        // `oo recall` alone → empty query string
        let args = vec![s("recall")];
        assert!(matches!(parse_action(&args), Action::Recall(q) if q.is_empty()));
    }

    #[test]
    fn test_parse_action_forget() {
        assert!(matches!(parse_action(&[s("forget")]), Action::Forget));
    }

    #[test]
    fn test_parse_action_learn() {
        let args = vec![s("learn"), s("cargo"), s("test")];
        assert!(matches!(parse_action(&args), Action::Learn(a) if a == vec!["cargo", "test"]));
    }

    #[test]
    fn test_parse_action_learn_no_subargs() {
        let args = vec![s("learn")];
        assert!(matches!(parse_action(&args), Action::Learn(a) if a.is_empty()));
    }

    #[test]
    fn test_parse_action_version() {
        assert!(matches!(parse_action(&[s("version")]), Action::Version));
    }

    #[test]
    fn test_parse_action_help_no_cmd() {
        assert!(matches!(parse_action(&[s("help")]), Action::Help(None)));
    }

    #[test]
    fn test_parse_action_help_with_cmd() {
        let args = vec![s("help"), s("ls")];
        assert!(matches!(parse_action(&args), Action::Help(Some(c)) if c == "ls"));
    }

    #[test]
    fn test_parse_action_init() {
        assert!(matches!(parse_action(&[s("init")]), Action::Init));
    }

    #[test]
    fn test_parse_action_run_unknown() {
        let args = vec![s("echo"), s("hi")];
        assert!(matches!(parse_action(&args), Action::Run(a) if a[0] == "echo"));
    }

    #[test]
    fn test_parse_action_run_hyphen_arg() {
        // Ensure hyphen-prefixed args aren't swallowed
        let args = vec![s("ls"), s("-la")];
        assert!(matches!(parse_action(&args), Action::Run(a) if a == vec!["ls", "-la"]));
    }

    fn make_output(exit_code: i32, stdout: &str) -> exec::CommandOutput {
        exec::CommandOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            exit_code,
            duration: std::time::Duration::from_millis(1),
        }
    }

    #[test]
    fn test_classify_with_refs_passthrough_small() {
        let out = make_output(0, "hello\n");
        let result = classify_with_refs(&out, "echo hello", &[]);
        assert!(matches!(result, Classification::Passthrough { output } if output == "hello\n"));
    }

    #[test]
    fn test_classify_with_refs_failure_no_pattern() {
        let out = make_output(1, "something went wrong\n");
        let result = classify_with_refs(&out, "bad_cmd", &[]);
        assert!(matches!(result, Classification::Failure { label, .. } if label == "bad_cmd"));
    }

    #[test]
    fn test_classify_with_refs_large_no_pattern() {
        let big = "x\n".repeat(3000); // >4KB
        let out = make_output(0, &big);
        let result = classify_with_refs(&out, "some_tool", &[]);
        assert!(matches!(result, Classification::Large { label, .. } if label == "some_tool"));
    }

    #[test]
    fn test_classify_with_refs_success_with_pattern() {
        let patterns = pattern::builtins();
        let refs: Vec<&pattern::Pattern> = patterns.iter().collect();
        let big = format!("{}47 passed in 3.2s\n", ".\n".repeat(3000));
        let out = make_output(0, &big);
        let result = classify_with_refs(&out, "pytest tests/", &refs);
        assert!(
            matches!(result, Classification::Success { summary, .. } if summary.contains("47 passed"))
        );
    }

    #[test]
    fn test_classify_with_refs_failure_with_pattern() {
        let patterns = pattern::builtins();
        let refs: Vec<&pattern::Pattern> = patterns.iter().collect();
        let fail_output: String = (0..50).map(|i| format!("error line {i}\n")).collect();
        let out = make_output(1, &fail_output);
        // pytest has a tail-30 failure strategy
        let result = classify_with_refs(&out, "pytest -x", &refs);
        match result {
            Classification::Failure { label, output } => {
                assert_eq!(label, "pytest");
                assert!(output.contains("error line 49"));
            }
            _ => panic!("expected Failure"),
        }
    }

    #[test]
    fn test_cmd_recall_empty_query_returns_1() {
        assert_eq!(cmd_recall(""), 1);
    }

    #[test]
    fn test_cmd_learn_no_args_returns_1() {
        assert_eq!(cmd_learn(&[]), 1);
    }

    #[test]
    fn test_cmd_run_empty_args_returns_1() {
        assert_eq!(cmd_run(&[]), 1);
    }
}
