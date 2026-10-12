//! API key presence check for `oo learn`.

/// Return the key only when it has non-whitespace content; trimmed.
///
/// An empty or whitespace-only value is treated as unset so `oo` never sends
/// an empty credential and gets a 401 instead of the missing-key error.
pub fn non_empty_key(raw: Option<String>) -> Option<String> {
    raw.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::non_empty_key;

    #[test]
    fn unset_key_is_none() {
        assert_eq!(non_empty_key(None), None);
    }

    #[test]
    fn empty_and_whitespace_keys_are_none() {
        assert_eq!(non_empty_key(Some(String::new())), None);
        assert_eq!(non_empty_key(Some("  \t ".to_string())), None);
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        assert_eq!(
            non_empty_key(Some("  sk-ant-123\n".to_string())),
            Some("sk-ant-123".to_string())
        );
    }
}
