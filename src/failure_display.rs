//! Display of the Failure tier (exit ≠ 0) used by [`crate::commands::render_classification`].
//!
//! Lives outside `commands.rs` to keep that file under its 500-line cap.

use humansize::{BINARY, format_size};

use crate::commands::{savings_suffix, try_index};

/// Print a Failure classification: the `✗` indicator (with optional savings
/// suffix), a blank line, the displayed slice, and — only when the slice
/// differs from the full merged output — a recall notice after it.
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
    // raw merged bytes so `oo recall` can reach what the display dropped.
    // Equal bytes mean nothing was dropped, so nothing is indexed.
    if full_output != output {
        let human_size = format_size(full_output.len(), BINARY);
        if try_index(command, full_output) {
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
