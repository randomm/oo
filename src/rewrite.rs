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
/// The segment text is preserved byte-for-byte: the only modification is the
/// inserted `oo ` prefix, spliced at the end of the last leading `VAR=value`
/// token (or byte 0 when there is no env prefix) — so internal whitespace,
/// quotes, newlines, and tabs are all kept exactly as written.
///
/// Matching is unanchored: the segment is rewritten when any pattern's
/// `command_match` regex matches anywhere in the env-stripped rest (this is
/// the same matching semantics as `oo <command>`, so a segment like
/// `nope pytest -q` *is* rewritten). Quoted text is not excluded from
/// matching. Whitespace-normalizing the env-stripped rest is acceptable for
/// matching purposes only — the output is never rebuilt from it.
///
/// Tokens are located with a quote- and backslash-aware scan of the original
/// segment (see [`scan_tokens`]): a leading `VAR=value` token whose value
/// contains a quote character or a backslash is refused (the function returns
/// `None` and the caller exits 1), because such a value would need a full
/// shell tokenizer to re-emit safely, and we choose the conservative option.
fn rewrite_segment(segment: &str, patterns: &[Pattern]) -> Option<String> {
    // A bare newline (or carriage return) outside quotes is not a command
    // separator (only `&&`/`||`/`;` are), so the segment spans multiple
    // lines. Rewriting it would join lines into one, which changes semantics.
    // Refuse conservatively.
    // A bare newline (or carriage return) *outside* quotes is not a command
    // separator (only `&&`/`||`/`;` are), so the segment spans multiple
    // lines. Rewriting it would join lines into one, which changes semantics.
    // Refuse conservatively. Quoted newlines are fine (they are part of a
    // single token and preserved verbatim).
    if has_unquoted_newline(segment) {
        return None;
    }
    let tokens = scan_tokens(segment)?;
    let first = tokens
        .first()
        .map(|t| segment[t.start..t.end].to_owned())
        .filter(|w| !w.is_empty())?;
    if first == "oo" {
        return None;
    }
    // Count the leading `NAME=value` tokens; `rest_start` is the byte offset
    // where `oo ` is inserted (start of the first non-env token, or 0 when
    // there is no env prefix).
    let mut env_count = 0;
    for (i, t) in tokens.iter().enumerate() {
        let word = &segment[t.start..t.end];
        let is_env = word
            .split_once('=')
            .is_some_and(|(name, _)| is_valid_var_name(name));
        if !is_env {
            env_count = i;
            break;
        }
        env_count = i + 1;
    }
    // `rest_start` is where `oo ` is inserted: the start of the first
    // non-env token, or byte 0 when the segment has no env prefix.
    let rest_start = if env_count == 0 {
        0
    } else if env_count < tokens.len() {
        tokens[env_count].start
    } else {
        return None; // all tokens are env: nothing to rewrite
    };
    let rest = normalize_whitespace(&segment[rest_start..]);
    // The `regex` crate guarantees linear-time matching, so unanchored
    // pattern matching over user-supplied input is not a ReDoS vector.
    pattern::find_matching(&rest, patterns)?;
    // The output is the original segment with `oo ` spliced in at
    // `rest_start`; everything else is copied verbatim.
    let mut out = String::with_capacity(segment.len() + 3);
    out.push_str(&segment[..rest_start]);
    out.push_str("oo ");
    out.push_str(&segment[rest_start..]);
    Some(out)
}

/// True when `name` is a valid shell variable name: non-empty and made up
/// solely of ASCII alphanumerics and underscores.
fn is_valid_var_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A token's byte range within its source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TokenRange {
    start: usize,
    end: usize,
}

/// Scan `segment` into byte-ranged tokens, honoring the same quote and
/// backslash rules as [`split_segments`]: outside single quotes a backslash
/// escapes the next character; inside single quotes a backslash is literal.
/// A quoted region (single or double) is consumed as part of the token it
/// belongs to, so whitespace inside quotes does not delimit tokens — this is
/// what makes a quoted newline inside a segment stay part of one token.
///
/// A leading `VAR=value` token whose value contains a quote or backslash is
/// refused (returns `None`): such a value would need a full shell tokenizer
/// to re-emit safely, and we choose the conservative option (documented in
/// [`rewrite_segment`]).
///
/// Returns `None` when the scan hits an unterminated quote or a trailing
/// lone backslash — the same conservative refusal as [`split_segments`].
fn scan_tokens(segment: &str) -> Option<Vec<TokenRange>> {
    let chars: Vec<(usize, char)> = segment.char_indices().collect();
    let n = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < n {
        let (byte_idx, c) = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // Start of a token at byte offset `byte_idx`.
        let token_start = byte_idx;
        let token_end = scan_token(&chars, &mut i)?;
        tokens.push(TokenRange {
            start: token_start,
            end: token_end,
        });
        // `i` now points at the first char after the token (whitespace or
        // end); the outer loop skips whitespace.
    }
    Some(tokens)
}

/// Scan one token starting at `chars[*i]` (a non-whitespace char) and return
/// the byte offset where the token ends (exclusive). Advances `*i` to the
/// first character after the token. Quote and backslash handling mirrors
/// [`split_segments`]: a backslash outside single quotes escapes the next
/// char; a quoted region is consumed whole. A quote or backslash appearing
/// inside a `NAME=value` env token is refused — `None`.
fn scan_token(chars: &[(usize, char)], i: &mut usize) -> Option<usize> {
    let n = chars.len();
    let start = chars[*i].0;
    let mut in_sq = false;
    let mut in_dq = false;
    let mut escaped = false;
    let mut env_value = false;
    let mut j = *i;
    while j < n {
        let (_, c) = chars[j];
        if escaped {
            escaped = false;
            j += 1;
            continue;
        }
        if in_sq {
            if c == '\'' {
                in_sq = false;
                j += 1;
                continue;
            }
            j += 1;
            continue;
        }
        if in_dq {
            if c == '\\' {
                escaped = true;
                j += 1;
                continue;
            }
            if c == '"' {
                in_dq = false;
                j += 1;
                continue;
            }
            j += 1;
            continue;
        }
        // Outside quotes.
        if c == '\'' {
            if env_value {
                return None; // env value with a quote: refused
            }
            in_sq = true;
            j += 1;
            continue;
        }
        if c == '"' {
            if env_value {
                return None; // env value with a quote: refused
            }
            in_dq = true;
            j += 1;
            continue;
        }
        if c == '\\' {
            if env_value {
                return None; // env value with a backslash: refused
            }
            escaped = true;
            j += 1;
            continue;
        }
        if c == '=' && !env_value && is_env_token_prefix(chars, j) {
            // This is a `NAME=value` token; the value half is opaque. Any
            // quote or backslash inside the value is refused.
            env_value = true;
            j += 1;
            continue;
        }
        // A plain whitespace ends the token.
        if c.is_whitespace() {
            break;
        }
        j += 1;
    }
    *i = j;
    // The token ends where the next char starts (or at the end of the string).
    let end = if *i < n {
        chars[*i].0
    } else {
        segment_len(chars)
    };
    Some(end.max(start))
}

/// True when the token containing `chars[i]` (i.e. the run of non-whitespace
/// chars ending at `=`) is a `NAME=value` env token: a valid variable name
/// followed by `=`. The name must contain no `=`, quotes, backslashes, or
/// whitespace.
fn is_env_token_prefix(chars: &[(usize, char)], i: usize) -> bool {
    // Walk back over the token's name portion (all non-whitespace chars
    // before `chars[i]` in the current token).
    let mut j = i;
    while j > 0 {
        let jc = chars[j - 1].1;
        if jc.is_whitespace() || jc == '=' || jc == '\'' || jc == '"' || jc == '\\' {
            return false;
        }
        j -= 1;
    }
    true
}

/// True when `segment` contains a newline or carriage return *outside* single
/// or double quotes. A bare (unquoted) newline inside a segment is not a
/// command separator (only `&&`/`||`/`;` are), so a segment that spans
/// multiple lines via an unquoted newline cannot be safely rewritten — the
/// rewrite would join lines into one, changing semantics. Quoted newlines
/// are part of a single token and are preserved verbatim.
fn has_unquoted_newline(segment: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in segment.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some('"') => {
                if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    quote = None;
                }
            }
            Some('\'') if c == '\'' => quote = None,
            Some('\'') => {}
            None => {
                if c == '\n' || c == '\r' {
                    return true;
                }
                if c == '\'' || c == '"' {
                    quote = Some(c);
                } else if c == '\\' {
                    escaped = true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Collapse internal whitespace runs to a single space for matching purposes
/// only (the output is never rebuilt from this string).
fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Total byte length of the string described by `chars`.
fn segment_len(chars: &[(usize, char)]) -> usize {
    chars.last().map(|(idx, c)| idx + c.len_utf8()).unwrap_or(0)
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

/// Tests live in `rewrite_tests.rs` and `rewrite_ws_tests.rs` (sibling
/// modules, see `#[path]` below) — this file holds only production code to
/// stay under the 500-line cap.
#[cfg(test)]
#[path = "rewrite_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "rewrite_ws_tests.rs"]
mod ws_tests;
