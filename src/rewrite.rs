//! `oo rewrite` — print an `oo`-prefixed version of a command for agent hooks.
//!
//! Shared rewriter that hook preexec handlers (pi, Claude Code) call:
//! exit 0 with the rewritten command on stdout when at least one shell
//! segment matches an oo pattern, exit 1 with no output otherwise. It never
//! executes the command and never touches the store.

use crate::pattern::{self, Pattern};

/// Maximum command length (bytes) accepted by [`rewrite`]. Realistic shell
/// commands stay well below this; the bound caps the worst-case regex work
/// done over attacker-influenced input.
const MAX_REWRITE_INPUT_BYTES: usize = 16 * 1024;

/// Rewrite `command` if at least one of its shell segments has an oo pattern.
///
/// Returns `None` (→ exit 1, no output) when the command is empty/blank,
/// longer than [`MAX_REWRITE_INPUT_BYTES`], contains an unterminated quote or
/// a trailing lone backslash (neither can be re-parsed identically), or
/// contains a construct we refuse to reason about: pipes, redirections,
/// heredocs, command substitution, or background `&`.
///
/// The input is a **raw shell string**, and the printed output is meant to be
/// shell-executed by the hook. The scanner is quote- and backslash-aware:
/// quote content is opaque to the refused-construct scan (a quoted
/// `|`/`&&`/`$()` does not trigger the refusal), and outside single quotes a
/// backslash escapes the next character (an escaped quote character does not
/// toggle quoting, an escaped `&` is not a background operator, and two
/// backslash-escaped `&` characters are not a separator). Inside single
/// quotes a backslash is literal. The exact original text — backslashes
/// included — is preserved byte-for-byte in the rewritten output, so the
/// output is guaranteed to re-parse identically under POSIX shell quoting
/// rules only.
///
/// Matching is **unanchored and per segment**: a segment is rewritten when
/// *any* pattern's `command_match` regex matches *anywhere* in the segment's
/// text — the same patterns and the same unanchored matching as
/// `oo <command>` (see [`rewrite_segment`]). Quoted text is not excluded
/// from matching.
pub fn rewrite(command: &str, patterns: &[Pattern]) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_REWRITE_INPUT_BYTES {
        return None;
    }

    // One quote-aware pass over the command: it both rejects refused
    // constructs and splits on `&&`/`||`/`;` separators (outside quotes), so
    // the input is scanned exactly once per call.
    let split = split_segments(trimmed)?;
    let mut out = String::with_capacity(trimmed.len());
    let mut changed = false;

    for (seg_text, sep_after) in split.iter() {
        let rewritten = rewrite_segment(seg_text, patterns);
        match &rewritten {
            Some(r) => {
                changed = true;
                out.push_str(r);
                out.push_str(sep_after);
            }
            None => {
                out.push_str(seg_text);
                out.push_str(sep_after);
            }
        }
    }

    changed.then_some(out)
}

/// Rewrite a single (trimmed) shell segment, preserving a leading `VAR=value`
/// environment prefix, or `None` when the segment needs no rewriting.
///
/// Matching is unanchored: the segment is rewritten when any pattern's
/// `command_match` regex matches anywhere in the segment text (this is the
/// same matching semantics as `oo <command>`, so a segment like
/// `nope pytest -q` *is* rewritten to `oo nope pytest -q`). Quoted text is
/// not excluded from matching.
fn rewrite_segment(segment: &str, patterns: &[Pattern]) -> Option<String> {
    let words: Vec<&str> = segment.split_whitespace().collect();
    let first = words.first().copied().unwrap_or("");
    if first == "oo" || first.is_empty() {
        return None;
    }
    // Count the leading `NAME=value` tokens; the env prefix is those words
    // re-joined with single spaces.
    let mut env_count = 0;
    for word in words.iter() {
        let is_env = word
            .split_once('=')
            .is_some_and(|(name, _)| is_valid_var_name(name));
        if !is_env {
            break;
        }
        env_count += 1;
    }
    if env_count == words.len() {
        return None;
    }
    let rest = words[env_count..].join(" ");
    // The `regex` crate guarantees linear-time matching, so unanchored
    // pattern matching over user-supplied input is not a ReDoS vector.
    pattern::find_matching(&rest, patterns)?;
    let env_prefix = words[..env_count].join(" ");
    if env_prefix.is_empty() {
        Some(format!("oo {rest}"))
    } else {
        Some(format!("{env_prefix} oo {rest}"))
    }
}

/// True when `name` is a valid shell variable name: non-empty and made up
/// solely of ASCII alphanumerics and underscores.
fn is_valid_var_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Split `command` into (trimmed_segment, separator_after) pairs on `&&`,
/// `||`, and `;` outside single or double quotes, refusing (returning `None`)
/// when the command contains any construct [`rewrite`] won't reason about:
/// pipes, redirections, heredocs, command substitution, or background `&`.
///
/// The scanner is **backslash-aware**, mirroring POSIX shell quoting:
/// outside single quotes, a backslash escapes the next character — the
/// escaped character is skipped for quote toggling, separator detection, and
/// the refused-construct scan (so an escaped quote character in
/// `pytest <backslash><double-quote>x && cargo build` does not open a quote
/// and the `&&` is a real separator, while `echo <backslash>&<backslash>&
/// pytest` is not split). Inside single quotes a backslash is literal (the
/// quote itself ends the quoted region). A trailing lone backslash (its
/// escape has no target) and an unterminated quote both yield `None` —
/// conservative refusal, since neither re-parses identically.
///
/// The separator is the canonical form with a single leading space:
/// `" && "`, `" || "`, or `" ; "`. The last segment has an empty separator.
///
/// The scan works over `char_indices` with a two-element lookahead (no
/// intermediate `Vec`), so every slice of `command` lands on a valid UTF-8
/// boundary regardless of multi-byte characters in the input — the function
/// cannot panic for any UTF-8 string.
fn split_segments(command: &str) -> Option<Vec<(String, &'static str)>> {
    let mut iter = command.char_indices().peekable();
    let mut result: Vec<(String, &'static str)> = Vec::new();
    let mut start = 0usize;
    let mut quote: Option<char> = None;

    while let Some((byte_idx, c)) = iter.next() {
        match quote {
            // Inside double quotes, a backslash escapes the next character
            // (an escaped double-quote does not close the quote).
            Some('"') if c == '\\' => {
                iter.peek()?;
                iter.next();
            }
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c == '\\' => {
                // Outside single quotes a backslash escapes the next
                // character, whatever it is. A trailing lone backslash has
                // no target character, so the input is refused.
                iter.peek()?;
                iter.next();
            }
            None => {
                let next = iter.peek().map(|(_, c)| *c);
                // Separator checks first: `&&`/`||` are split points, not
                // refused constructs (a lone `|` or `&` is refused below).
                if c == '&' && next == Some('&') {
                    result.push((command[start..byte_idx].trim().to_string(), " && "));
                    // The next segment starts after both separator bytes.
                    start = byte_idx + 2;
                    iter.next();
                    continue;
                }
                if c == '|' && next == Some('|') {
                    result.push((command[start..byte_idx].trim().to_string(), " || "));
                    start = byte_idx + 2;
                    iter.next();
                    continue;
                }
                if c == ';' {
                    result.push((command[start..byte_idx].trim().to_string(), " ; "));
                    start = byte_idx + 1;
                    continue;
                }
                // Anything else is a refused construct: the command cannot be
                // rewritten safely.
                if c == '|' || c == '<' || c == '>' || c == '&' {
                    return None;
                }
                if (c == '$' && next == Some('(')) || c == '`' {
                    return None;
                }
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    result.push((command[start..].trim().to_string(), ""));
    Some(result)
}

/// Tests live in `rewrite_tests.rs` (sibling module, see `#[path]` below) —
/// this file holds only production code to stay under the 500-line cap.
#[cfg(test)]
#[path = "rewrite_tests.rs"]
mod tests;
