//! Unit tests for the reserved-subcommand usage table and its dispatch.

use crate::commands::{Action, parse_action};
use crate::usage::{for_subcommand, reserved_names};

fn args(t: &[&str]) -> Vec<String> {
    t.iter().map(|s| s.to_string()).collect()
}

#[test]
fn every_reserved_subcommand_has_usage() {
    for sub in reserved_names() {
        let text = for_subcommand(sub).unwrap_or_else(|| panic!("no usage for {sub}"));
        assert!(
            text.contains(&format!("oo {sub}")),
            "usage for {sub}: {text:?}"
        );
    }
}

#[test]
fn non_reserved_name_has_no_usage() {
    assert!(for_subcommand("cargo").is_none());
    assert!(for_subcommand("--help").is_none());
}

/// Every name in the usage table is dispatched by `parse_action` to its own
/// action, never to the `Run` fall-through (which would spawn it as a shell
/// command). Keeps the dispatch match in step with the table.
#[test]
fn every_reserved_name_is_dispatched_not_run() {
    for sub in reserved_names() {
        assert!(
            !matches!(parse_action(&args(&[sub])), Action::Run(_)),
            "`oo {sub}` must be a reserved dispatch, not a shell command"
        );
    }
}

#[test]
fn help_flag_after_reserved_subcommand_yields_usage() {
    for sub in reserved_names() {
        for flag in ["--help", "-h"] {
            match parse_action(&args(&[sub, flag])) {
                Action::Usage(text) => assert_eq!(Some(text), for_subcommand(sub)),
                _ => panic!("`oo {sub} {flag}` must yield Action::Usage"),
            }
        }
    }
}

#[test]
fn help_flag_not_first_argument_is_not_intercepted() {
    // `oo recall help` is a word search, and `oo recall x --help` is a query
    // whose --help is not the first argument.
    assert!(matches!(
        parse_action(&args(&["recall", "help"])),
        Action::Recall { .. }
    ));
    assert!(matches!(
        parse_action(&args(&["recall", "x", "--help"])),
        Action::Recall { .. }
    ));
}

#[test]
fn non_reserved_command_with_help_flag_still_runs() {
    match parse_action(&args(&["cargo", "--help"])) {
        Action::Run(a) => assert_eq!(a, args(&["cargo", "--help"])),
        _ => panic!("`oo cargo --help` must still execute"),
    }
}
