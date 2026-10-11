//! Pattern-set assembly for `oo <cmd>`, `oo rewrite`, and the hook processors.
//!
//! The canonical pattern precedence set (project, user, then built-in; first match
//! wins), built by [`all_patterns`]. `oo <cmd>` (`commands::run_command_args`) calls
//! `all_patterns()` on each run. `oo rewrite` and `oo hook` use the [`PATTERNS`]
//! static, which holds `all_patterns()` computed once per process. Imports `pattern`
//! (loading and builtins), `learn` (user config directory via
//! `learn::patterns_dir`) and `init::find_root` (git root for project-local
//! patterns).
//!
//! Exception: the foreground classification in `commands::cmd_learn` (issue
//! #151) deliberately uses `pattern::builtins()` only, not this set, so the
//! `oo learn` display does not consult project or user patterns.

use std::sync::LazyLock;

use crate::{
    init::find_root,
    learn,
    pattern::{self, Pattern},
};

/// The canonical pattern set: project-local, then user config, then builtins,
/// first-match-wins (project patterns override user patterns override
/// builtins). Used by `oo rewrite` (`commands::cmd_rewrite`, via
/// `rewrite::PATTERNS`) and `oo hook` (`hook.rs`). `oo <cmd>` does not use this
/// static; it calls [`all_patterns`] on each run. Initialised once per process,
/// on first use.
pub static PATTERNS: LazyLock<Vec<Pattern>> = LazyLock::new(all_patterns);

/// Load all patterns in the canonical precedence order — project-local, then
/// user config, then builtins — so first-match-wins gives project patterns
/// priority over user patterns over builtins. Reads the project and user pattern
/// directories from disk on every call (no caching); built-in patterns are a
/// `&'static` slice and are not re-read. Called by `oo <cmd>` and by the
/// [`PATTERNS`] initializer.
pub fn all_patterns() -> Vec<Pattern> {
    let mut all_patterns = load_project_patterns();
    all_patterns.extend(pattern::load_user_patterns(&learn::patterns_dir()));
    all_patterns.extend_from_slice(pattern::builtins());
    all_patterns
}

/// Load project-local patterns from `<git-root>/.oo/patterns/`.
///
/// Returns an empty vec when cwd cannot be determined or the directory does
/// not exist (gracefully handled by `load_user_patterns`).
pub fn load_project_patterns() -> Vec<Pattern> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    pattern::load_user_patterns(&find_root(&cwd).join(".oo").join("patterns"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_patterns_is_nonempty_and_includes_builtins() {
        let all = all_patterns();
        assert!(
            !all.is_empty(),
            "the canonical pattern set must not be empty"
        );
        // The builtins are the last precedence tier, so at least one builtin
        // command must appear.
        let builtin_matches: Vec<&str> = pattern::builtins()
            .iter()
            .map(|p| p.command_match.as_str())
            .collect();
        assert!(
            builtin_matches
                .iter()
                .any(|bm| { all.iter().any(|p| p.command_match.as_str() == *bm) }),
            "all_patterns must include the builtin patterns"
        );
    }

    #[test]
    fn all_patterns_precedence_is_project_user_builtin() {
        // The precedence order is load order: project patterns first, then
        // user config, then builtins. User patterns (from
        // `learn::patterns_dir`) appear before the builtin tier; verify the
        // assembly does not drop a tier by checking the counts add up.
        let all = all_patterns();
        let user_dir = learn::patterns_dir();
        let user_count = if user_dir.exists() {
            pattern::load_user_patterns(&user_dir).len()
        } else {
            0
        };
        let builtin_count = pattern::builtins().len();
        assert!(
            all.len() >= user_count + builtin_count,
            "all_patterns({}) must contain at least user({}) + builtin({}) entries",
            all.len(),
            user_count,
            builtin_count
        );
    }
}
