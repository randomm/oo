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
/// longer than [`MAX_REWRITE_INPUT_BYTES`], or contains a construct we refuse
/// to reason about: pipes, redirections, heredocs, command substitution, or
/// background `&`.
///
/// The input is a **raw shell string**, and the printed output is meant to be
/// shell-executed by the hook. Quote content is opaque to the refused-
/// construct scan (a quoted `|`/`&&`/`$()` does not trigger the refusal),
/// but the shell re-parses the output: metacharacters inside quotes become
/// live there. Rewriting therefore never *introduces* a new unquoted token,
/// but it preserves whatever the caller already placed inside quotes — a hook
/// that shell-executes the output inherits shell semantics of that quoted
/// content, exactly as the original command would have. The rewrite is a safe
/// passthrough only under that contract.
pub fn rewrite(command: &str, patterns: &[Pattern]) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_REWRITE_INPUT_BYTES {
        return None;
    }

    // One quote-aware pass over the command: it both rejects refused
    // constructs and splits on `&&`/`||`/;` separators (outside quotes), so
    // the input is decoded to `Vec<char>` exactly once per call.
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
/// The separator is the canonical form with a single leading space:
/// `" && "`, `" || "`, or `" ; "`. The last segment has an empty separator.
fn split_segments(command: &str) -> Option<Vec<(String, &'static str)>> {
    let chars: Vec<char> = command.chars().collect();
    let n = chars.len();
    let mut result: Vec<(String, &'static str)> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut quote: Option<char> = None;

    while i < n {
        let c = chars[i];
        match quote {
            Some(q) if c == q => quote = None,
            None if c == '\'' || c == '"' => quote = Some(c),
            None => {
                // Separator checks first: `&&`/`||` are split points, not
                // refused constructs (a lone `|` or `&` is refused below).
                if c == '&' && i + 1 < n && chars[i + 1] == '&' {
                    let seg: String = command[start..byte_offset(&chars, i)].trim().to_string();
                    result.push((seg, " && "));
                    i += 2;
                    start = i;
                    continue;
                }
                if c == '|' && i + 1 < n && chars[i + 1] == '|' {
                    let seg: String = command[start..byte_offset(&chars, i)].trim().to_string();
                    result.push((seg, " || "));
                    i += 2;
                    start = i;
                    continue;
                }
                if c == ';' {
                    let seg: String = command[start..byte_offset(&chars, i)].trim().to_string();
                    result.push((seg, " ; "));
                    i += 1;
                    start = i;
                    continue;
                }
                // Anything else is a refused construct: the command cannot be
                // rewritten safely.
                if c == '|' || c == '<' || c == '>' || c == '&' {
                    return None;
                }
                if (c == '$' && i + 1 < n && chars[i + 1] == '(') || c == '`' {
                    return None;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let last: String = command[byte_offset(&chars, start)..].trim().to_string();
    result.push((last, ""));
    Some(result)
}

/// Byte offset in the original string of char-index `i` (i.e. the length of
/// the first `i` chars), so `command[..byte_offset(&chars, i)]` always slices
/// on a char boundary.
fn byte_offset(chars: &[char], i: usize) -> usize {
    chars
        .get(..i.min(chars.len()))
        .unwrap_or(&[])
        .iter()
        .map(|c| c.len_utf8())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns() -> Vec<Pattern> {
        pattern::builtins().to_vec()
    }

    #[test]
    fn rewrites_builtin_pattern_command() {
        let out = rewrite("pytest -q", &patterns()).expect("pytest must be rewritten");
        assert_eq!(out, "oo pytest -q");
    }

    #[test]
    fn no_pattern_returns_none() {
        assert!(rewrite("git log --stat", &patterns()).is_none());
        assert!(rewrite("git diff", &patterns()).is_none());
        assert!(rewrite("ls -la", &patterns()).is_none());
    }

    #[test]
    fn per_segment_matching_only_matching_segment_rewritten() {
        let out = rewrite("git status && pytest -q", &patterns())
            .expect("pytest segment must be rewritten");
        assert_eq!(out, "git status && oo pytest -q");
    }

    #[test]
    fn multiple_segments_rewritten() {
        let out =
            rewrite("ls && cd src && cargo build", &patterns()).expect("cargo build rewrites");
        assert_eq!(out, "ls && cd src && oo cargo build");
    }

    #[test]
    fn or_separator() {
        let out = rewrite("pytest -q || pytest --verbose", &patterns()).expect("both match");
        assert_eq!(out, "oo pytest -q || oo pytest --verbose");
    }

    #[test]
    fn semicolon_separator() {
        let out = rewrite("echo hi ; cargo build", &patterns()).expect("cargo build matches");
        assert_eq!(out, "echo hi ; oo cargo build");
    }

    #[test]
    fn segments_inside_quotes_are_not_split() {
        assert!(rewrite("echo \"a && b\"", &patterns()).is_none());
        assert!(rewrite("echo 'a && b'", &patterns()).is_none());
    }

    #[test]
    fn quote_containing_pattern_still_rewritten_as_one_segment() {
        let out = rewrite("pytest -q \"x && y\"", &patterns()).expect("must rewrite");
        assert_eq!(out, "oo pytest -q \"x && y\"");
    }

    #[test]
    fn already_oo_prefixed_is_not_double_wrapped() {
        assert!(rewrite("oo pytest -q", &patterns()).is_none());
        let out = rewrite("cargo build && oo pytest", &patterns()).expect("cargo build rewrites");
        assert_eq!(out, "oo cargo build && oo pytest");
        assert!(!out.contains("oo oo"));
    }

    #[test]
    fn env_prefix_preserved_in_front_of_oo() {
        let out = rewrite("FOO=1 pytest -q", &patterns()).expect("must rewrite");
        assert_eq!(out, "FOO=1 oo pytest -q");
    }

    #[test]
    fn multiple_env_prefixes_preserved() {
        let out = rewrite("FOO=1 BAR=2 cargo test", &patterns()).expect("must rewrite");
        assert_eq!(out, "FOO=1 BAR=2 oo cargo test");
    }

    #[test]
    fn non_ascii_env_prefix_does_not_panic() {
        // A multi-byte separator context must not panic on a byte-boundary
        // split. A non-ASCII *name* (`Ü`) is not a valid env-var name, so
        // `Ü=1` is treated as the command word and no prefix is applied —
        // but the scan must still run without a mid-codepoint panic.
        let out = rewrite("Ü=1 pytest -q", &patterns()).expect("must rewrite");
        assert_eq!(out, "oo Ü=1 pytest -q");
        // A genuine multi-byte env prefix: the value `é` is non-ASCII (2
        // bytes), so the old index-based code sliced mid-codepoint here and
        // panicked. The new code must preserve the multi-byte prefix intact.
        let out = rewrite("A=é pytest -q", &patterns()).expect("must rewrite");
        assert_eq!(out, "A=é oo pytest -q");
    }

    #[test]
    fn non_ascii_before_separator_does_not_panic() {
        // A multi-byte char before a separator: slicing on the char index
        // would land mid-codepoint and panic; byte-offset slicing must not.
        let out = rewrite("é && pytest -q", &patterns()).expect("must rewrite");
        assert_eq!(out, "é && oo pytest -q");
        let out = rewrite("héllo ; cargo build", &patterns()).expect("must rewrite");
        assert_eq!(out, "héllo ; oo cargo build");
    }

    #[test]
    fn overlong_input_is_refused_not_panicked() {
        let long = format!("{} pytest -q", "a".repeat(MAX_REWRITE_INPUT_BYTES));
        assert!(rewrite(&long, &patterns()).is_none());
    }

    #[test]
    fn env_prefix_on_unmatched_segment_is_untouched() {
        assert!(rewrite("FOO=1 ls -la", &patterns()).is_none());
    }

    #[test]
    fn pipe_outside_quotes_returns_none() {
        assert!(rewrite("cargo test | tee out.txt", &patterns()).is_none());
        assert!(rewrite("pytest | head", &patterns()).is_none());
    }

    #[test]
    fn pipe_inside_quotes_is_allowed() {
        let out = rewrite("pytest -q \"a | b\"", &patterns()).expect("must rewrite");
        assert_eq!(out, "oo pytest -q \"a | b\"");
    }

    #[test]
    fn redirection_returns_none() {
        assert!(rewrite("cargo test > log.txt", &patterns()).is_none());
        assert!(rewrite("pytest < input.txt", &patterns()).is_none());
    }

    #[test]
    fn heredoc_marker_returns_none() {
        assert!(rewrite("cat <<EOF\nhi\nEOF", &patterns()).is_none());
    }

    #[test]
    fn command_substitution_returns_none() {
        assert!(rewrite("echo $(pytest)", &patterns()).is_none());
        assert!(rewrite("pytest `date`", &patterns()).is_none());
    }

    #[test]
    fn background_ampersand_returns_none() {
        assert!(rewrite("cargo build & sleep 1", &patterns()).is_none());
        let out = rewrite("pytest -q \"a & b\"", &patterns()).expect("must rewrite");
        assert_eq!(out, "oo pytest -q \"a & b\"");
    }

    #[test]
    fn empty_and_whitespace_input_returns_none() {
        assert!(rewrite("", &patterns()).is_none());
        assert!(rewrite("   ", &patterns()).is_none());
        assert!(rewrite("\t\n ", &patterns()).is_none());
    }

    #[test]
    fn custom_pattern_is_honored() {
        let custom = Pattern {
            command_match: regex::Regex::new(r"^mytest\b").unwrap(),
            success: None,
            failure: None,
        };
        let out = rewrite("mytest --fast", std::slice::from_ref(&custom))
            .expect("custom pattern must rewrite");
        assert_eq!(out, "oo mytest --fast");
    }
}
