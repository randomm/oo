#[cfg(test)]
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
    let out =
        rewrite("git status && pytest -q", &patterns()).expect("pytest segment must be rewritten");
    assert_eq!(out, "git status && oo pytest -q");
}

#[test]
fn multiple_segments_rewritten() {
    let out = rewrite("ls && cd src && cargo build", &patterns()).expect("cargo build rewrites");
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
    // split. A non-ASCII *name* is not a valid env-var name, so the
    // word is treated as the command word and no prefix is applied.
    let out = rewrite("Ü=1 pytest -q", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo Ü=1 pytest -q");
    // A genuine multi-byte env prefix: the value is non-ASCII (2 bytes).
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
fn multibyte_after_separator_does_not_panic() {
    // Multi-byte content in the *following* segment: the end offset of
    // the next segment must also slice on a char boundary.
    let out = rewrite("git log && éé pytest -q", &patterns()).expect("must rewrite");
    assert_eq!(out, "git log && oo éé pytest -q");
}

#[test]
fn emoji_and_multibyte_inside_quotes_do_not_panic() {
    let out = rewrite("pytest -q \"😂 && 🎉\"", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -q \"😂 && 🎉\"");
    let out = rewrite("pytest -q 'café ; test'", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -q 'café ; test'");
}

#[test]
fn multibyte_input_never_panics_any_input() {
    // Property-style sweep: rewrite() must not panic on any UTF-8 input,
    // in particular multi-byte chars immediately before/after separators
    // and backslash/quote combinations (see the backslash-awareness tests
    // below for the expected rewrites).
    let strings: Vec<String> = vec![
        "é".to_string(),
        "&&".to_string(),
        "é &&".to_string(),
        "&& é".to_string(),
        "é && pytest -q".to_string(),
        "éé; pytest -q".to_string(),
        "pytest -q; éé".to_string(),
        "héllo && wörld || pytest".to_string(),
        "🚀 && cargo test".to_string(),
        "& & &".to_string(),
        ";; ;".to_string(),
        "é=1 pytest -q".to_string(),
        "A=é && B=ü; cargo build".to_string(),
        "'é && é' pytest -q".to_string(),
        "\"é||é\" pytest".to_string(),
        "\t\n é && \t".to_string(),
        "café && naïve ; résumé || pytest -q".to_string(),
        "🚀🌙é".to_string(),
        "ééééé && pytest".to_string(),
        " && pytest -q".to_string(),
        "&&&&&& pytest".to_string(),
        "a\\ && pytest".to_string(),
        "a\\ && b || c".to_string(),
        "pytest -q \\\\\\&&".to_string(),
        "\\".to_string(),
        "\\\\".to_string(),
        "\"unterminated && pytest".to_string(),
        "'unterminated && pytest".to_string(),
        "pytest \"a\\ && rm -rf /\"".to_string(),
        "pytest 'a\\\\' && pytest".to_string(),
        "echo \\&\\& pytest".to_string(),
        "pytest \"x && cargo build".to_string(),
        "é\\ && pytest -q".to_string(),
        "\\é && pytest".to_string(),
        "\\\\\\\\\\\\\\ pytest".to_string(),
        "\\\\\" && pytest".to_string(),
        "a \\\\\" && pytest".to_string(),
    ];
    for s in strings {
        // Any Option (Some or None) is fine - the point is no panic.
        let _ = rewrite(&s, &patterns());
        let _ = split_segments(&s);
        for seg in split_segments(&s).unwrap_or_default() {
            let _ = rewrite_segment(seg.0.trim(), &patterns());
        }
    }
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
fn unanchored_matching_matches_anywhere_in_segment() {
    // Pin the unanchored matching rule: a pattern's command_match regex
    // matching *anywhere* in the segment text rewrites the whole
    // segment, not just the matched substring. This is intentional and
    // matches `oo <command>`'s own matching semantics.
    let out = rewrite("nope pytest -q", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo nope pytest -q");
}

// ---- Backslash awareness ------------------------------------------------

#[test]
fn escaped_quote_does_not_toggle_quoting() {
    // The escaped quote inside the double-quoted string does not close
    // the quote, so the quoted `&&` is not a separator. The whole
    // command is one segment, preserved byte-for-byte (backslash
    // included).
    let out = rewrite("pytest \"a\\ && rm -rf /\"", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest \"a\\ && rm -rf /\"");
}

#[test]
fn escaped_ampersands_are_not_a_separator() {
    // Both `&` characters are escaped outside quotes, so this is not a
    // separator (and not a refused background `&`). One segment,
    // rewritten as a whole, text preserved exactly.
    let out = rewrite("echo \\&\\& pytest", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo echo \\&\\& pytest");
}

#[test]
fn backslash_inside_single_quotes_is_literal() {
    // Inside single quotes the backslash is literal and the quote after
    // it closes the quoted region. The `&&` after the close quote is a
    // real separator; the second segment is rewritten.
    let out = rewrite("pytest -k 'a\\' && pytest", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -k 'a\\' && oo pytest");
}

#[test]
fn escaped_quote_outside_quotes_is_just_a_quote_char() {
    // The escaped quote outside quotes is just a literal quote
    // character - it does not open a quoted region, so the `&&` is a
    // real separator.
    let out = rewrite("pytest \\\"x && cargo build", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest \\\"x && oo cargo build");
}

#[test]
fn trailing_lone_backslash_returns_none() {
    // A trailing lone backslash has no character to escape; the input
    // cannot be re-parsed identically, so the rewrite is refused
    // conservatively.
    assert!(rewrite("pytest -q \\", &patterns()).is_none());
    assert!(rewrite("\\", &patterns()).is_none());
}

#[test]
fn unterminated_quote_returns_none() {
    // An unterminated quote cannot be re-parsed identically either.
    assert!(rewrite("echo \"abc && pytest", &patterns()).is_none());
    assert!(rewrite("echo 'abc && pytest", &patterns()).is_none());
}

#[test]
fn backslash_before_multibyte_char_does_not_panic_or_break() {
    // Escaping a multi-byte character consumes the whole codepoint (no
    // mid-codepoint slice), and the text is preserved exactly.
    let out = rewrite("echo \\é && pytest -q", &patterns()).expect("must rewrite");
    assert_eq!(out, "echo \\é && oo pytest -q");
    let out = rewrite("\\é && pytest -q", &patterns()).expect("must rewrite");
    assert_eq!(out, "\\é && oo pytest -q");
}

#[test]
fn escaped_separators_and_constructs_are_ignored() {
    // Escaped `;`/`|`/`<`/`>`/`$`/backtick do not split or refuse; the
    // text is preserved exactly.
    let out = rewrite("pytest -q \\; cargo build", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -q \\; cargo build");
    let out = rewrite("pytest -q \\| cargo build", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -q \\| cargo build");
    let out = rewrite("pytest -q \\< in.txt \\> out.txt", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -q \\< in.txt \\> out.txt");
    let out = rewrite("pytest -q \\(date)", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -q \\(date)");
    let out = rewrite("pytest -q \\`date\\`", &patterns()).expect("must rewrite");
    assert_eq!(out, "oo pytest -q \\`date\\`");
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
