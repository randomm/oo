//! Byte-bounded head+tail truncation for the Bounded (Content/Unknown) tier.
//!
//! Kept apart from `classify.rs` so that file stays under the 500-line cap.

use crate::classify::DISPLAY_CAP;

/// Floor a byte offset to the nearest preceding char boundary.
///
/// Equivalent to `str::floor_char_boundary` (stable since 1.91) but implemented
/// for MSRV 1.85 compatibility.
fn floor_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Ceil a byte offset to the nearest following char boundary.
///
/// Equivalent to `str::ceil_char_boundary` (stable since 1.91) but implemented
/// for MSRV 1.85 compatibility.
fn ceil_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Byte-based truncation with UTF-8 char-boundary safety.
///
/// Produces a head+tail slice bounded by [`DISPLAY_CAP`] bytes total, with a
/// single truncation marker line between them. Cuts snap to `\n` boundaries
/// (at most one line of drift) as a nicety, but the cap is enforced on the
/// ASSEMBLED slices: line-snapping may only ever shrink a slice relative to
/// its byte budget, never grow it past it (a single line can be arbitrarily
/// long — minified JS/JSON, base64). If the output has fewer than 2 newlines,
/// cuts fall back to char-boundary-only slicing. Never splits a multi-byte
/// UTF-8 sequence.
///
/// Returns the input unchanged when it is ≤ [`DISPLAY_CAP`] bytes.
pub fn bounded_truncate(output: &str) -> String {
    bounded_truncate_indexed(output, true)
}

/// [`bounded_truncate`] with the marker chosen by whether the full output was
/// indexed. When `indexed` is false the marker must not advertise `oo recall`,
/// because there is nothing to recall.
pub fn bounded_truncate_indexed(output: &str, indexed: bool) -> String {
    if output.len() <= DISPLAY_CAP {
        return output.to_string();
    }

    let head_budget = (DISPLAY_CAP as f64 * 0.6) as usize; // 2457
    let tail_budget = DISPLAY_CAP - head_budget; // 1639

    let (head_end, tail_start) = cut_boundaries(output, head_budget, tail_budget);
    // Enforce the cap on the assembled result, not just the budgets: line
    // snapping must never let head + tail grow past the byte budget. Hard-clamp
    // the head down to its budget (floor) and the tail up to its budget (ceil).
    let clamped_head = floor_char_boundary(output, head_budget).min(head_end);
    // `.max(tail_start)` is NOT redundant: cut_boundaries' overlap guard can
    // return tail_start = output.len(), in which case ceil(raw_tail) would
    // otherwise restore a tail slice whose head and tail regions OVERLAP
    // (head_end >= raw_tail means a non-empty output[raw_tail..head_end] would
    // appear twice — head, then again as tail). The guard's `len` encodes
    // "no tail slice" and must win over the budget-based fallback. (The head
    // side's `.min(head_end)` IS load-bearing for the long-line case: a line
    // can exceed its budget, so the floor must win there.)
    let clamped_tail =
        ceil_char_boundary(output, output.len().saturating_sub(tail_budget)).max(tail_start);
    debug_assert!(
        clamped_head + (output.len() - clamped_tail) <= DISPLAY_CAP,
        "head+tail slices ({clamped_head} + {} bytes) must not exceed DISPLAY_CAP",
        output.len() - clamped_tail
    );

    let truncated_bytes = output.len() - clamped_head - (output.len() - clamped_tail);
    let marker = if indexed {
        format!("... [{truncated_bytes} bytes truncated → use `oo recall` to query] ...\n")
    } else {
        format!("... [{truncated_bytes} bytes truncated] ...\n")
    };

    let mut result = String::with_capacity(DISPLAY_CAP + marker.len());
    result.push_str(&output[..clamped_head]);
    result.push_str(&marker);
    result.push_str(&output[clamped_tail..]);
    result
}

/// Compute head/tail cut byte offsets for [`bounded_truncate`].
///
/// Snaps the head cut forward to the next `\n` (≤ one line) and the tail cut
/// backward to the previous `\n` (≤ one line). A single forward O(1)-memory
/// pass finds the only three facts that matter: whether ≥ 2 newlines exist,
/// the first newline at/after `head_budget`, and the last newline strictly
/// before `raw_tail`. When fewer than 2 newlines exist in the entire output,
/// falls back to char-boundary-only cuts.
///
/// NOTE: the snapped positions may EXCEED the byte budgets (a line can be
/// arbitrarily long). [`bounded_truncate`] enforces the cap on the assembled
/// slices — line-snapping is a nicety that may only shrink, never grow.
pub(crate) fn cut_boundaries(
    output: &str,
    head_budget: usize,
    tail_budget: usize,
) -> (usize, usize) {
    let raw_tail = output.len().saturating_sub(tail_budget);
    let mut newline_count = 0usize;
    let mut first_nl_at_or_after_head: Option<usize> = None;
    let mut last_nl_before_raw_tail: Option<usize> = None;
    for (i, b) in output.as_bytes().iter().enumerate() {
        if *b != b'\n' {
            continue;
        }
        newline_count += 1;
        // Early-exit when both facts are settled: we have a newline at/after
        // head_budget AND this newline is >= raw_tail (so it can no longer be
        // the last newline before raw_tail). Honest win: it skips only the
        // trailing tail-budget window; with dense newlines the loop still
        // scans essentially the whole buffer (it exits at max(raw_tail, first
        // nl at/after head_budget)). The real win is O(1) memory — no Vec of
        // newline positions is ever allocated.
        if i >= raw_tail && first_nl_at_or_after_head.is_some() {
            break;
        }
        if i >= head_budget && first_nl_at_or_after_head.is_none() {
            first_nl_at_or_after_head = Some(i);
        }
        if i < raw_tail {
            last_nl_before_raw_tail = Some(i);
        }
    }

    // Fewer than 2 newlines: line-snapping has nothing to snap to (there is
    // at most one newline in the entire output, so neither cut can land on
    // the "right" side of a line and still keep its slice near budget), so
    // use char-boundary-only cuts directly — exactly the bounds the clamp in
    // `bounded_truncate` would apply, keeping the display within budget.
    if newline_count < 2 {
        let head = floor_char_boundary(output, head_budget);
        let tail = ceil_char_boundary(output, raw_tail);
        return (head, tail);
    }

    // Snap head cut forward to next \n (at most one line of drift). When no
    // newline falls at/after head_budget, the raw budget itself is used as the
    // fallback and must be snapped to a char boundary before `+1` — the `+1`
    // is only a safe "skip the newline" when the offset actually is a newline.
    let head_end = match first_nl_at_or_after_head {
        Some(pos) => pos + 1, // include the newline in the head slice
        None => floor_char_boundary(output, head_budget),
    };

    // Snap tail cut backward to previous \n (at most one line of drift). Same
    // fallback hazard on the tail side: when no newline falls before raw_tail,
    // the raw budget must be ceiled to a char boundary. (In practice this
    // fallback is also shielded by the overlap guard below, but snapping it
    // keeps the invariant local and symmetric with the head cut.)
    let tail_start = match last_nl_before_raw_tail {
        Some(pos) => pos + 1, // start after the newline
        None => ceil_char_boundary(output, raw_tail),
    };

    // Ensure head doesn't overlap tail: returning `tail_start = output.len()`
    // encodes "no tail slice" — the head covers everything shown (in this
    // branch head_end ≤ output.len(), so the head is non-empty and the tail
    // is empty, which is always a valid display).
    if head_end >= tail_start {
        return (head_end, output.len());
    }

    (head_end, tail_start)
}
