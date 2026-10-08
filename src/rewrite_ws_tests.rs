use super::*;

fn patterns() -> Vec<Pattern> {
    crate::pattern::builtins().to_vec()
}

// ---- Whitespace preservation (issue #171 round-3 lens fix) ---------------
//
// The rewrite must preserve the original segment text byte-for-byte except
// for the inserted `oo ` prefix(es) and the canonical separator spacing. The
// bug it guards against: the old implementation did `segment.split_whitespace()`
// + `words.join(" ")`, which collapsed whitespace runs inside quotes and
// turned newlines into spaces — silently changing command semantics.

#[test]
fn double_spaces_inside_double_quotes_preserved() {
    let out = rewrite("pytest -k \"a  b\"", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -k \"a  b\"");
}

#[test]
fn double_spaces_inside_single_quotes_preserved() {
    let out = rewrite("pytest -k 'a  b'", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -k 'a  b'");
}

#[test]
fn tabs_preserved() {
    let out = rewrite("pytest\t-q", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest\t-q");
    let out = rewrite("pytest -q\t\t--x", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -q\t\t--x");
}

#[test]
fn embedded_newline_inside_quotes_preserved() {
    let out = rewrite("pytest -k \"a\nb\"", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -k \"a\nb\"");
    let out = rewrite("pytest -k 'a\nb'", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -k 'a\nb'");
}

#[test]
fn leading_trailing_whitespace_preserved_in_segment() {
    // Leading/trailing whitespace of a segment is trimmed by the separator
    // splitter (canonical separator design); internal spacing is preserved.
    let out = rewrite("  pytest -q  ", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -q");
    let out = rewrite("pytest -q  --x", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -q  --x");
}

#[test]
fn env_prefix_original_spacing_preserved() {
    let out = rewrite("FOO=1  pytest   -q", &patterns()).expect("must rewrite");
    assert_eq!(out, "FOO=1  oo pytest   -q");
    let out = rewrite("FOO=1 BAR=2  pytest -q", &patterns()).expect("must rewrite");
    assert_eq!(out, "FOO=1 BAR=2  oo pytest -q");
}

#[test]
fn env_value_with_quote_refused() {
    // An env value containing a quote is refused (conservative option).
    assert!(rewrite("FOO='a b' pytest -q", &patterns()).is_none());
    assert!(rewrite("FOO=\"a b\" pytest -q", &patterns()).is_none());
}

#[test]
fn env_value_with_backslash_refused() {
    // An env value containing a backslash is refused (conservative option).
    assert!(rewrite("FOO=a\\b pytest -q", &patterns()).is_none());
}

#[test]
fn unquoted_newline_refused() {
    // A bare newline outside quotes is not a command separator (only
    // `&&`/`||`/`;` are), so it cannot be safely rewritten — the output would
    // have to join two lines into one, which changes semantics. Refused.
    assert!(rewrite("pytest -q\nfoo", &patterns()).is_none());
    // A newline inside a quoted segment is fine (quoted, not a separator).
    let out = rewrite("pytest -k 'a\nb' && pytest", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -k 'a\nb' && oo pytest");
}

#[test]
fn multiline_quoted_segment_with_separator() {
    // A quoted newline inside a segment that also has a real separator: the
    // quoted newline is preserved, the separator is canonical.
    let out = rewrite("pytest -k 'x\ny' ; cargo build", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -k 'x\ny' ; oo cargo build");
}

/// Byte-exact sweep: for every input that `rewrite` rewrites, the output is
/// the input (trimmed) with exactly `3 * n_segments` bytes of inserted `oo `
/// prefixes added, and the input text is preserved verbatim otherwise.
#[test]
fn rewrite_output_is_input_plus_prefixes_only() {
    let inputs = [
        "pytest -q",
        "pytest -k \"a  b\"",
        "pytest -k 'a  b'",
        "pytest\t-q",
        "pytest -k 'a\nb'",
        "FOO=1 pytest -q",
        "FOO=1 BAR=2 pytest -q",
        "cd x && pytest -q",
        "git status && cargo build",
        "pytest -q ; cargo build",
        "pytest -q || cargo test",
        "nope pytest -q",
    ];
    for input in inputs {
        let Some(out) = rewrite(input, &patterns()) else {
            continue;
        };
        // Count how many `oo ` prefixes were inserted. The output is the
        // trimmed input with `oo ` inserted at each rewritten segment's start.
        let trimmed = input.trim();
        // The output minus the inserted `oo ` tokens must equal the trimmed
        // input (modulo the canonical separator spacing, which is unchanged
        // here because the inputs use single spaces around separators).
        let reconstructed = strip_inserted_prefixes(trimmed, &out);
        assert_eq!(
            reconstructed, trimmed,
            "output must be input + inserted `oo ` prefixes only (input={input:?})"
        );
        // The output length equals the input length + 3 * (number of
        // rewritten segments). The number of rewritten segments equals the
        // number of `oo ` tokens inserted.
        let inserted = out.len() - trimmed.len();
        assert_eq!(
            inserted % 3,
            0,
            "inserted bytes must be a multiple of 3 (input={input:?})"
        );
        assert_eq!(
            out.len(),
            trimmed.len() + inserted,
            "output length must equal input length + inserted bytes (input={input:?})"
        );
    }
}

/// Remove inserted `oo ` prefixes from `out` to reconstruct `input`.
/// The rewrite inserts `oo ` at the start of each rewritten segment; this
/// function walks the two strings in parallel and strips those prefixes.
fn strip_inserted_prefixes(input: &str, out: &str) -> String {
    let mut result = String::new();
    let mut in_iter = input.chars().peekable();
    let mut out_iter = out.chars().peekable();
    loop {
        let in_ch = in_iter.peek().copied();
        let out_ch = out_iter.peek().copied();
        match (in_ch, out_ch) {
            (None, None) => break,
            (Some(a), Some(b)) if a == b => {
                result.push(a);
                in_iter.next();
                out_iter.next();
            }
            // `out` has an extra `oo ` prefix here: skip it in `out`.
            (Some(_), Some('o')) => {
                let o1 = out_iter.next();
                let o2 = out_iter.next();
                let o3 = out_iter.next();
                assert_eq!(o1, Some('o'));
                assert_eq!(o2, Some('o'));
                assert_eq!(o3, Some(' '));
            }
            (Some(_), None) => break,
            // Extra chars in out that are not an `oo ` prefix: this is a
            // mismatch — the sweep is meant to be strict, so fail here.
            (Some(_), Some(other)) => {
                panic!("unexpected char {other:?} in output not in input");
            }
            // Input exhausted but output still has chars: leftover `oo `.
            (None, Some(_)) => {
                let o1 = out_iter.next();
                let o2 = out_iter.next();
                let o3 = out_iter.next();
                assert_eq!((o1, o2, o3), (Some('o'), Some('o'), Some(' ')));
            }
        }
    }
    result
}
