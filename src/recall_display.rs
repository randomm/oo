//! Display helpers for `oo recall` bounded output.
//!
//! When FTS5 snippets are not available (LIKE fallback, VipuneStore, or the
//! snippet field is `None`), client-side truncation keeps the output bounded.

use crate::session;
use crate::store;
use crate::util::format_age;

/// Character ceiling for a single hit's default (non-`--full`) display.
///
/// The cap is on **character count**, not bytes (byte slicing would panic on
/// multi-byte UTF-8 boundaries), so the displayed output is at most 512
/// characters — for multi-byte content the byte length can approach ~4× that.
/// 512 chars is well under the ~4 KB context budget per hit that the feature
/// targets and matches the empirical ~200-char FTS5 snippet target (tokens are
/// clamped to 1-64, which is ~200-400 chars for ASCII; 512 chars gives headroom
/// for the truncation marker and multi-byte characters without exceeding the budget).
pub const SNIPPET_CAP: usize = 512;

/// The marker appended when a hit's display is truncated to `SNIPPET_CAP`.
///
/// Deliberately a distinctive sentinel rather than a lone `…`: a bare ellipsis
/// occurs naturally in stored shell output AND is the omission marker FTS5's
/// own `snippet()` uses, so "output ends in …" would not reliably mean "we
/// truncated it". The unambiguous sentinel means an agent can always tell
/// truncation from content (see `bounded_display`).
pub const ELLIPSIS_MARKER: &str = " [oo: truncated]";

/// Bounded display for a recall hit's default (non-`--full`) output.
///
/// Prefers the store-provided FTS5 `snippet` when present (the whole point of
/// the FTS5 path — an excerpt centered on the best match) and falls back to a
/// client-side bounded prefix of `content` when the snippet is `None` (the
/// LIKE short-query branch, or backends without FTS5 such as `VipuneStore`).
///
/// The FTS5 `snippet()` window is bounded in *tokens*, so a single token made
/// of multi-byte characters (e.g. a 10 000-char unbroken UTF-8 token) can be
/// much wider than the token count suggests. The store's char-count guard
/// (`snippet.chars().count() >= content.chars().count() → None`) only maps the
/// full-row case to `None`; it does not guarantee the snippet fits the display
/// budget. So the preferred path still runs the chosen snippet through
/// `bounded_display`, guaranteeing `SNIPPET_CAP` chars plus a detectable
/// truncation marker whenever the snippet itself exceeds the budget.
pub fn display_hit(content: &str, snippet: &Option<String>, cap: usize) -> String {
    match snippet {
        Some(snip) => bounded_display(snip, cap),
        None => bounded_display(content, cap),
    }
}

/// Truncate `content` to at most `cap` chars, appending `ELLIPSIS_MARKER` if truncated.
///
/// The cap is on character count, not bytes, to avoid panics on multi-byte UTF-8
/// boundaries. The ellipsis marker is appended only when the content exceeded `cap`.
pub fn bounded_display(content: &str, cap: usize) -> String {
    let char_count = content.chars().count();
    if char_count <= cap {
        return content.to_string();
    }

    let truncated: String = content.chars().take(cap).collect();
    format!("{truncated}{ELLIPSIS_MARKER}")
}

/// Execute `oo recall`: search the store and print results.
///
/// `full = true` prints the complete stored content line-by-line (legacy behaviour).
/// `full = false` prints a bounded excerpt per hit (the default).
pub fn cmd_recall(query: &str, full: bool) -> i32 {
    if query.is_empty() {
        eprintln!("oo: recall requires a query");
        return 1;
    }

    let mut store = match store::open() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("oo: {e}");
            return 1;
        }
    };

    let project_id = session::project_id();

    match store.search(&project_id, query, 5) {
        Ok(results) if results.is_empty() => {
            println!("No results found.");
            0
        }
        Ok(results) => {
            for r in &results {
                if let Some(meta) = &r.meta {
                    let age = format_age(meta.timestamp);
                    println!("[session] {} ({age}):", meta.command);
                } else {
                    println!("[memory] project memory:");
                }
                if full {
                    // `--full` is a deliberate escape hatch: complete stored
                    // content, no cap. Can flood an agent's context — callers
                    // should prefer the bounded default.
                    for line in r.content.lines() {
                        println!("  {line}");
                    }
                } else {
                    // Bounded display: prefer the store-provided FTS5 snippet when
                    // available, else fall back to a bounded prefix of `content`
                    // (LIKE short-query branch / non-FTS5 backends). Indent every
                    // line so a multi-line excerpt groups identically to --full.
                    let display = display_hit(&r.content, &r.snippet, SNIPPET_CAP);
                    for line in display.lines() {
                        println!("  {line}");
                    }
                }
                println!();
            }
            0
        }
        Err(e) => {
            eprintln!("oo: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_content_unchanged() {
        let input = "short";
        assert_eq!(bounded_display(input, 512), input);
    }

    #[test]
    fn content_exactly_at_cap_unchanged() {
        let input: String = "x".repeat(512);
        assert_eq!(bounded_display(&input, 512), input);
    }

    #[test]
    fn content_over_cap_truncated_with_marker() {
        let input: String = "x".repeat(600);
        let result = bounded_display(&input, 512);
        // 512 chars + marker (" [oo: truncated]" = 16 chars) = 528 chars total
        assert_eq!(result.chars().count(), 512 + 16);
        assert!(result.ends_with(ELLIPSIS_MARKER));
        // Must not contain the tail of the original content
        assert!(!result.ends_with(&"x".repeat(100)));
    }

    #[test]
    fn truncation_marker_is_unambiguous_sentinel() {
        // The marker must be a sentinel that cannot plausibly occur in captured
        // shell output (and does not collide with FTS5's own `…` omission
        // markers), so "ends with the marker" reliably means "we truncated it".
        assert_eq!(ELLIPSIS_MARKER, " [oo: truncated]");
        assert!(ELLIPSIS_MARKER.contains('['));
    }

    // Guard tests for display_hit — the wiring that connects the store's FTS5
    // snippet to what actually gets printed.

    #[test]
    fn display_hit_prefers_snippet_when_present() {
        // When the store provides an FTS5 snippet, the display must come from
        // the snippet, not from `content`. If the wiring is removed (display
        // falls back to content), the matched-token excerpt is lost and the
        // full blob prefix is shown instead — this test fails either way
        // (excerpt != first 512 chars of content for the blob below).
        let content: String = (0..200)
            .map(|i| format!("line{i:04}_padding_{} ", "x".repeat(40)))
            .collect();
        let snippet = "…line0100_padding_".to_string() + &"y".repeat(50) + "…";
        let display = display_hit(&content, &Some(snippet.clone()), SNIPPET_CAP);
        assert_eq!(
            display, snippet,
            "display must be the store-provided snippet when one is present"
        );
        assert!(
            !content.starts_with(&snippet) && !content.starts_with(&display),
            "display must not be a prefix of the full content — that would prove \nthe FTS5 snippet was thrown away in favour of content"
        );
    }

    #[test]
    fn display_hit_snippet_oversized_still_bounded_and_marked() {
        // An FTS5 snippet() window is bounded in tokens, and one token can be
        // arbitrarily many multi-byte chars — a single 10 000-char UTF-8 token
        // returns a ~10 KB snippet from FTS5. The display must still respect
        // SNIPPET_CAP and carry the truncation marker (detectable truncation).
        let content: String = std::iter::repeat('€').take(10_000).collect();
        let snippet: String = std::iter::repeat('€').take(10_000).collect();
        let display = display_hit(&content, &Some(snippet), SNIPPET_CAP);
        assert_eq!(
            display.chars().count(),
            SNIPPET_CAP + ELLIPSIS_MARKER.chars().count()
        );
        assert!(display.ends_with(ELLIPSIS_MARKER));
    }

    #[test]
    fn display_hit_falls_back_to_content_when_snippet_none() {
        // LIKE short-query branch and VipuneStore yield snippet: None — the
        // display must be the bounded prefix of the full content.
        let content: String = (0..200).map(|i| format!("word{i} ")).collect();
        let display = display_hit(&content, &None, SNIPPET_CAP);
        assert_eq!(display, bounded_display(&content, SNIPPET_CAP));
        assert!(
            display.starts_with("word0"),
            "fallback must be a prefix of content"
        );
    }

    #[test]
    fn multi_byte_chars_respect_char_boundary() {
        // 100 '€' chars (3 bytes each) = 100 chars — under 512 cap
        let input: String = std::iter::repeat('€').take(100).collect();
        assert_eq!(bounded_display(&input, 512), input);

        // 600 '€' chars = over cap → truncated to 512 + marker
        let long: String = std::iter::repeat('€').take(600).collect();
        let result = bounded_display(&long, 512);
        assert_eq!(
            result.chars().count(),
            512 + ELLIPSIS_MARKER.chars().count()
        );
        assert!(result.ends_with(ELLIPSIS_MARKER));
    }
}
