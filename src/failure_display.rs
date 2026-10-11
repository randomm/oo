//! Display of the Failure tier (exit ≠ 0) used by [`crate::commands::render_classification`].
//!
//! Lives outside `commands.rs` to keep that file under its 500-line cap.

use humansize::{BINARY, format_size};

use crate::commands::{savings_suffix, try_index};

/// Print a Failure classification: the `✗` indicator (with optional savings
/// suffix), a blank line, the displayed slice, and — only when the slice
/// does not match the full merged output — a recall notice after it.
pub fn render(label: &str, command: &str, original_size: usize, output: &str, full_output: &str) {
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
    // The slice above is lossy (pattern/smart truncation); index the
    // raw merged output so `oo recall` can reach what the display dropped.
    if !shows_all_lines(full_output, output) {
        let human_size = format_size(full_output.len(), BINARY);
        if try_index(command, full_output) {
            // SECURITY (residual risk, same as the Bounded arm): this line is
            // host-authored but printed after attacker-controlled `output`,
            // which can embed a byte-identical copy of it. Its text alone
            // cannot prove it came from oo. See docs/security-model.md.
            println!(
                "\u{25CF} {label} (output truncated: {human_size} total \u{2192} use `oo recall` to query)"
            );
        } else {
            eprintln!(
                "oo: warning: could not index output for recall — full output LOST (not recoverable); what follows is a truncated slice only"
            );
            println!(
                "\u{25CF} {label} (output truncated: {human_size} total — NOT indexed, recall unavailable)"
            );
        }
    }
}

/// Whether the displayed slice shows every line of the full output.
///
/// Compares lines rather than raw bytes: `extract_failure` joins its lines
/// without a final newline, and `lines()` ignores CRLF endings. Trailing empty
/// lines are dropped from both sides, because a slice that keeps or loses only
/// trailing blank lines (or the final newline) omits nothing a reader needs.
/// The 81–120-line `smart_truncate` join removes a line break, so it is
/// correctly "not shown".
fn shows_all_lines(full_output: &str, output: &str) -> bool {
    trimmed_lines(full_output).eq(trimmed_lines(output))
}

fn trimmed_lines(text: &str) -> impl Iterator<Item = &str> {
    let lines: Vec<&str> = text.lines().collect();
    let end = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map_or(0, |i| i + 1);
    lines.into_iter().take(end)
}

#[cfg(test)]
#[path = "failure_display_tests.rs"]
mod tests;
