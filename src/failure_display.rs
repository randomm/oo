//! Display of the Failure tier (exit ≠ 0) used by [`crate::commands::render_classification`].
//!
//! Lives outside `commands.rs` to keep that file under its 500-line cap.

use humansize::{BINARY, format_size};

use crate::display::savings_suffix;
use crate::indexing::index_or_warn;

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
        if index_or_warn(command, full_output) {
            // SECURITY (residual risk, same as the Bounded arm): this line is
            // host-authored but printed after attacker-controlled `output`,
            // which can embed a byte-identical copy of it. Its text alone
            // cannot prove it came from oo. See docs/security-model.md.
            println!(
                "\u{25CF} {label} (output truncated: {human_size} total \u{2192} use `oo recall` to query)"
            );
        } else {
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
/// trailing empty lines (or the final newline) omits nothing a reader needs.
/// The 81–120-line `smart_truncate` join removes a line break, so it is
/// correctly "not shown".
fn shows_all_lines(full_output: &str, output: &str) -> bool {
    trim_trailing_empty_lines(full_output)
        .lines()
        .eq(trim_trailing_empty_lines(output).lines())
}

/// Drop the trailing run of empty lines without allocating.
///
/// Strips only whole line terminators (`\n` or `\r\n`), so a whitespace-only
/// line stays put. Afterwards the text ends in a non-terminator character, so
/// `lines()` yields no trailing empty line — the same result as collecting the
/// lines and dropping empties from the end.
fn trim_trailing_empty_lines(text: &str) -> &str {
    let mut end = text.len();
    loop {
        let head = &text[..end];
        if let Some(rest) = head
            .strip_suffix("\r\n")
            .or_else(|| head.strip_suffix('\n'))
        {
            end = rest.len();
        } else {
            return head;
        }
    }
}

#[cfg(test)]
#[path = "failure_display_tests.rs"]
mod tests;
