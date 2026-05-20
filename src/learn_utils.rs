//! Utility functions for the learn module.

/// Truncate a UTF-8 string to a maximum byte count, respecting character boundaries.
pub(crate) fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Strip markdown code fences (``` and ~~~) from a string.
///
/// Removes leading/trailing fence markers and optional language identifiers,
/// returning only the content between the fences.
pub(crate) fn strip_fences(s: &str) -> String {
    let trimmed = s.trim();
    // Remove ```toml, ```rust, etc., or plain ```
    let start = if let Some(rest) = trimmed.strip_prefix("```") {
        // Skip language identifier if present (e.g., "```toml")
        rest.lines().next().map(|l| l.trim()).unwrap_or(rest).len() + 3
    } else if let Some(rest) = trimmed.strip_prefix("~~~") {
        // Similar handling for ~~~ fences
        rest.lines().next().map(|l| l.trim()).unwrap_or(rest).len() + 3
    } else {
        0
    };

    let content = if start > 0 && start < trimmed.len() {
        trimmed[start..].trim()
    } else {
        trimmed
    };

    // Remove trailing fence
    content
        .rsplit_once("\n```")
        .map(|(c, _)| c)
        .or_else(|| content.rsplit_once("\n~~~").map(|(c, _)| c))
        .unwrap_or(content)
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_utf8_short_string() {
        let s = "hello";
        assert_eq!(truncate_utf8(s, 10), s);
    }

    #[test]
    fn test_truncate_utf8_exact_length() {
        let s = "hello";
        assert_eq!(truncate_utf8(s, 5), s);
    }

    #[test]
    fn test_truncate_utf8_multibyte() {
        let s = "你好世界"; // 4 Chinese characters, each 3 bytes in UTF-8
        assert_eq!(truncate_utf8(s, 6), "你好"); // 6 bytes = 2 characters
    }

    #[test]
    fn test_truncate_utf8_char_boundary() {
        let s = "hello世界";
        let result = truncate_utf8(s, 7); // "hello" (5 bytes) + partial of first Chinese char
        assert_eq!(result, "hello"); // Should truncate to valid UTF-8 boundary
    }

    #[test]
    fn test_strip_fences_code_blocks() {
        let s = "```toml\ncommand_match = \"test\"\n```";
        assert_eq!(strip_fences(s), "command_match = \"test\"");
    }

    #[test]
    fn test_strip_fences_without_language() {
        let s = "```\ncommand_match = \"test\"\n```";
        assert_eq!(strip_fences(s), "command_match = \"test\"");
    }

    #[test]
    fn test_strip_fences_tilde_fences() {
        let s = "~~~\ncommand_match = \"test\"\n~~~";
        assert_eq!(strip_fences(s), "command_match = \"test\"");
    }

    #[test]
    fn test_strip_fences_no_fence() {
        let s = "command_match = \"test\"";
        assert_eq!(strip_fences(s), "command_match = \"test\"");
    }
}
