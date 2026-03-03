/// Returns the current time as seconds since the Unix epoch.
pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_secs() as i64
}

/// Formats a Unix timestamp as a human-readable age string relative to now.
pub fn format_age(timestamp: i64) -> String {
    let age = now_epoch() - timestamp;
    if age < 60 {
        format!("{age}s ago")
    } else if age < 3600 {
        format!("{}min ago", age / 60)
    } else if age < 86400 {
        format!("{}h ago", age / 3600)
    } else {
        format!("{}d ago", age / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_age_seconds() {
        // A timestamp ~30s in the past must format with the "s ago" suffix.
        // Use ends_with so a one-second clock skew between the two now_epoch()
        // calls cannot cause a flap (the number may be 29 or 30, but suffix is stable).
        let ts = now_epoch() - 30;
        assert!(
            format_age(ts).ends_with("s ago"),
            "expected 's ago' suffix for 30s"
        );
    }

    #[test]
    fn test_format_age_minutes() {
        // 120 seconds → "min ago" tier
        let ts = now_epoch() - 120;
        assert!(
            format_age(ts).ends_with("min ago"),
            "expected 'min ago' suffix for 120s"
        );
    }

    #[test]
    fn test_format_age_hours() {
        // 7200 seconds → "h ago" tier
        let ts = now_epoch() - 7200;
        assert!(
            format_age(ts).ends_with("h ago"),
            "expected 'h ago' suffix for 7200s"
        );
    }

    #[test]
    fn test_format_age_days() {
        // 172800 seconds → "d ago" tier
        let ts = now_epoch() - 172_800;
        assert!(
            format_age(ts).ends_with("d ago"),
            "expected 'd ago' suffix for 172800s"
        );
    }
}
