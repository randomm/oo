//! Kept out of classify_tests.rs, which is far over the 500-line cap.
//! Smart-truncation boundary tests (issue #194). Totals up to MAX_LINES return
//! the input byte-for-byte; joining head and tail there would fuse lines.

use super::*;

#[test]
fn test_smart_truncation_81_to_120_lines_returned_unchanged() {
    for n in [81, 100, 120] {
        let lines: String = (0..n).map(|i| format!("line {i}\n")).collect();
        assert_eq!(smart_truncate(&lines), lines, "{n} lines must be unchanged");
    }
}

#[test]
fn test_smart_truncation_preserves_crlf_and_trailing_newline_at_boundaries() {
    for n in [81, 120] {
        let crlf: String = (0..n).map(|i| format!("line {i}\r\n")).collect();
        assert_eq!(
            smart_truncate(&crlf),
            crlf,
            "{n} CRLF lines must be unchanged"
        );
        let no_newline = (0..n)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(smart_truncate(&no_newline), no_newline);
    }
}

#[test]
fn test_smart_truncation_121_lines_keeps_marker_and_split() {
    let lines: String = (0..121).map(|i| format!("line {i}\n")).collect();
    let result = smart_truncate(&lines);
    // head = ceil(120 * 0.6) = 72, tail = 48, one line dropped
    assert!(result.contains("line 71\n... [1 lines truncated] ...\n"));
    assert!(result.contains("line 73\nline 74"));
    assert!(!result.contains("line 72"));
    assert!(result.ends_with("line 120"));
}

#[test]
fn test_bounded_truncate_indexed_false_marker_omits_recall_hint() {
    let big: String = (0..2000).map(|i| format!("row {i}\n")).collect();
    let unindexed = bounded_truncate_indexed(&big, false);
    assert!(
        !unindexed.contains("oo recall"),
        "unindexed marker must not advertise recall"
    );
    assert!(unindexed.contains("bytes truncated] ..."));
    assert_eq!(
        bounded_truncate_indexed(&big, true),
        bounded_truncate(&big),
        "indexed marker must stay byte-for-byte unchanged"
    );
}
