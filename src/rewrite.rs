//! `oo rewrite` — print an `oo`-prefixed version of a command for agent hooks.
//!
//! Shared rewriter that hook preexec handlers (pi, Claude Code) call:
//! exit 0 with the rewritten command on stdout when at least one shell
//! segment matches an oo pattern, exit 1 with no output otherwise. It never
//! executes the command and never touches the store.

use crate::pattern::{self, Pattern};

/// Rewrite `command` if at least one of its shell segments has an oo pattern.
///
/// Returns `None` (→ exit 1, no output) when the command is empty/blank or
/// contains a construct we refuse to reason about: pipes, redirections,
/// heredocs, command substitution, or background `&`.
///
/// The input is a **raw shell string**, and the printed output is meant to be
/// shell-executed by the hook. Quote content is opaque to
/// [`contains_unsafe_construct`] (a quoted `|`/`&&`/`$(...)` does not trigger
/// the refusal), but the shell re-parses the output: metacharacters inside
/// quotes become live there. Rewriting therefore never *introduces* a new
/// unquoted token, but it preserves whatever the caller already placed
/// inside quotes — a hook that shell-executes the output inherits shell
/// semantics of that quoted content, exactly as the original command would
/// have. The rewrite is a safe passthrough only under that contract.
pub fn rewrite(command: &str, patterns: &[Pattern]) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    if contains_unsafe_construct(trimmed) {
        return None;
    }

    let segments = split_segments(trimmed);
    let mut out = String::with_capacity(trimmed.len());
    let mut changed = false;

    for (seg_text, sep_after) in segments.iter() {
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
    if segment.is_empty() {
        return None;
    }
    let first = segment.split_whitespace().next().unwrap_or("");
    if first == "oo" {
        return None;
    }
    // Walk the leading `NAME=value` tokens, consuming each token plus the
    // separator whitespace that follows it. `consumed` tracks how many bytes
    // of `segment` have been consumed so far — always a real char boundary
    // because it is built up from `next.len()` (a &str byte length) plus the
    // separator bytes, never a hand-computed byte offset.
    let mut consumed = 0usize;
    let mut rest = segment;
    while let Some(next) = rest.split_whitespace().next() {
        let is_env = next
            .split_once('=')
            .is_some_and(|(name, _)| is_valid_var_name(name));
        if !is_env {
            break;
        }
        // After the token comes one or more separator bytes (spaces/tabs).
        // `after` is the remainder of `rest` once the token is dropped; the
        // separator bytes are the gap between `next`'s end and `after`'s
        // first non-whitespace byte.
        let after = &rest[next.len()..];
        let sep = after.len() - after.trim_start().len();
        consumed = consumed + next.len() + sep;
        rest = after.trim_start();
    }
    let env_prefix = &segment[..consumed];
    if rest.is_empty() {
        return None;
    }
    pattern::find_matching(rest, patterns)?;
    Some(format!("{env_prefix}oo {rest}"))
}

/// True when `name` is a valid shell variable name: non-empty and made up
/// solely of ASCII alphanumerics and underscores.
fn is_valid_var_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Split a command into (trimmed_segment, separator_after) pairs on `&&`,
/// `||`, and `;` outside single or double quotes.
///
/// The separator is the canonical form with a single leading space:
/// `" && "`, `" || "`, or `" ; "`. The last segment has an empty separator.
fn split_segments(command: &str) -> Vec<(String, &'static str)> {
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
            None if c == '&' && i + 1 < n && chars[i + 1] == '&' => {
                let seg: String = command[start..i].trim().to_string();
                result.push((seg, " && "));
                i += 2;
                start = i;
                continue;
            }
            None if c == '|' && i + 1 < n && chars[i + 1] == '|' => {
                let seg: String = command[start..i].trim().to_string();
                result.push((seg, " || "));
                i += 2;
                start = i;
                continue;
            }
            None if c == ';' => {
                let seg: String = command[start..i].trim().to_string();
                result.push((seg, " ; "));
                i += 1;
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    let last: String = command[start..].trim().to_string();
    result.push((last, ""));
    result
}

/// True when `command` contains a pipe, redirection, heredoc, command
/// substitution, or background `&` outside of quotes.
fn contains_unsafe_construct(command: &str) -> bool {
    let chars: Vec<char> = command.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut quote: Option<char> = None;

    while i < n {
        let c = chars[i];
        match quote {
            Some(q) if c == q => quote = None,
            None if c == '\'' || c == '"' => quote = Some(c),
            None => {
                if c == '|' && i + 1 < n && chars[i + 1] == '|' {
                    i += 2;
                    continue;
                }
                if c == '|' || c == '<' || c == '>' {
                    return true;
                }
                if c == '&' && i + 1 < n && chars[i + 1] == '&' {
                    i += 2;
                    continue;
                }
                if c == '&' {
                    return true;
                }
                if c == '$' && i + 1 < n && chars[i + 1] == '(' {
                    return true;
                }
                if c == '`' {
                    return true;
                }
            }
            Some(_) => {}
        }
        i += 1;
    }
    false
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
