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
        // Asserts only on the returned vector, never on a live re-read of the
        // user or project directories: user and project tiers vary per machine
        // and across the process-global `PATTERNS` snapshot. The guard keeps a
        // concurrent cwd or HOME mutation from changing the result mid-call.
        let _guard = crate::test_env::env_guard();
        let all = all_patterns();
        let builtins = pattern::builtins();

        // Every builtin must be present, so dropping any one of them fails.
        for builtin in builtins {
            assert!(
                all.iter()
                    .any(|p| p.command_match.as_str() == builtin.command_match.as_str()),
                "builtin {:?} missing from all_patterns()",
                builtin.command_match.as_str()
            );
        }

        // Builtins are the last tier, in their declared order: first-match-wins
        // gives project and user patterns priority over them.
        let tail_start = all
            .len()
            .checked_sub(builtins.len())
            .expect("all_patterns must contain every builtin");
        for (got, want) in all[tail_start..].iter().zip(builtins) {
            assert_eq!(
                got.command_match.as_str(),
                want.command_match.as_str(),
                "builtin tier must be last, in declared order"
            );
        }
    }
}
