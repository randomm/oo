//! Display helpers for `oo recall` bounded output.
//!
//! When FTS5 snippets are not available (LIKE fallback, VipuneStore, or the
//! snippet field is `None`), client-side truncation keeps the output bounded.

use crate::session;
use crate::store;
use crate::util::format_age;

/// Hard byte ceiling for a single hit's default (non-`--full`) display.
///
/// 512 chars is well under the ~4 KB context budget per hit that the feature
/// targets and matches the empirical ~200-char FTS5 snippet target (tokens are
/// clamped to 1-64, which is ~200-400 chars for ASCII; 512 chars gives headroom
/// for the ellipsis marker and multi-byte characters without exceeding the budget).
pub const SNIPPET_CAP: usize = 512;

/// The marker appended to truncated output.
pub const ELLIPSIS_MARKER: &str = " \u{2026}";

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
                    for line in r.content.lines() {
                        println!("  {line}");
                    }
                } else {
                    // Bounded display: use the store-provided snippet when available
                    // (task-a), otherwise fall back to client-side truncation.
                    // The snippet field will be `Some` once task-a adds FTS5 snippet()
                    // to SqliteStore::search; until then, client-side truncation keeps
                    // output bounded for all backends.
                    let display = bounded_display(&r.content, SNIPPET_CAP);
                    println!("  {display}");
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
    fn content_over_cap_truncated_with_ellipsis() {
        let input: String = "x".repeat(600);
        let result = bounded_display(&input, 512);
        // 512 chars + " \u{2026}" (2 chars) = 514 chars total
        assert_eq!(result.chars().count(), 514);
        assert!(result.ends_with(ELLIPSIS_MARKER));
        // Must not contain the tail of the original content
        assert!(!result.ends_with(&"x".repeat(100)));
    }

    #[test]
    fn ellipsis_marker_is_unicode_ellipsis() {
        assert_eq!(ELLIPSIS_MARKER, " \u{2026}");
        assert_eq!(ELLIPSIS_MARKER.chars().count(), 2);
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
