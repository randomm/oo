use super::*;
use crate::classify::smart_truncate;

#[test]
fn identical_output_shows_all_lines() {
    let full = "error: one\nerror: two\nerror: three";
    assert!(shows_all_lines(full, full));
}

#[test]
fn dropped_middle_lines_are_not_shown() {
    let full = "first\nmiddle-a\nmiddle-b\nlast";
    let slice = "first\nlast";
    assert!(!shows_all_lines(full, slice));
}

#[test]
fn smart_truncate_100_lines_is_shown_in_full() {
    // 100 lines: within MAX_LINES, so smart_truncate returns them unchanged;
    // the 60/40 boundary lines must stay on separate lines.
    let full: String = (0..100).map(|i| format!("line {i}\n")).collect();
    let slice = smart_truncate(&full);
    assert_eq!(slice, full);
    assert!(shows_all_lines(&full, &slice));
}

#[test]
fn trailing_empty_lines_and_final_newline_do_not_count() {
    let full = "alpha\nbeta";
    assert!(shows_all_lines(full, "alpha\nbeta\n"));
    assert!(shows_all_lines(full, "alpha\nbeta\n\n\n"));
    assert!(shows_all_lines("alpha\nbeta\n\n", "alpha\nbeta"));
}

#[test]
fn crlf_versus_lf_is_not_a_difference() {
    assert!(shows_all_lines("alpha\r\nbeta\r\n", "alpha\nbeta"));
    assert!(shows_all_lines("alpha\nbeta", "alpha\r\nbeta\r\n"));
}

#[test]
fn empty_versus_empty_shows_all_lines() {
    assert!(shows_all_lines("", ""));
}

#[test]
fn empty_slice_of_nonempty_output_is_not_shown() {
    assert!(!shows_all_lines("something failed", ""));
}
