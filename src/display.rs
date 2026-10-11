//! Savings suffix for compressed indicator lines, shared by the render arms.

use humansize::{BINARY, format_size};

use crate::classify;

/// Build the `[saved {humansize}]` suffix for a compressed indicator line.
///
/// `original_bytes` is the merged output size in bytes (`merged_lossy().len()`);
/// `indicator_bytes` is the byte length of the indicator line EXCLUDING the
/// savings suffix (call sites pass `line.len()` before the suffix is
/// appended), so the reported figure overstates displayed bytes by the
/// suffix's own length (~15 B — immaterial at the sizes where a suffix can
/// appear). Returns `None` when the saving is ≤ [`classify::MIN_SAVINGS`]
/// (i.e. the suffix appears only when `saved > MIN_SAVINGS`; saturating
/// arithmetic makes underflow impossible), so the caller prints the line
/// unchanged. Only the indicator line counts as displayed — the filtered
/// output lines printed after it (Failure arm) are not included.
pub fn savings_suffix(original_bytes: usize, indicator_bytes: usize) -> Option<String> {
    let saved = original_bytes.saturating_sub(indicator_bytes);
    (saved > classify::MIN_SAVINGS).then(|| format!(" [saved {}]", format_size(saved, BINARY)))
}
