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

    /// Writes one fixture pattern file `dir/name` whose `command_match` is unique,
    /// so the test can locate it in the result.
    fn write_fixture(dir: &std::path::Path, name: &str, command_match: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join(name),
            format!("command_match = \"{command_match}\"\n"),
        )
        .unwrap();
    }

    #[test]
    fn all_patterns_precedence_is_project_user_builtin() {
        // Builds a project tier (`<root>/.git` + `.oo/patterns`) and a user tier
        // (`OO_CONFIG_DIR/patterns`) with unique fixtures, then calls
        // `all_patterns()` directly. It does not read the memoized `PATTERNS`,
        // so the result depends only on the fixtures written here.
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        write_fixture(
            &root.path().join(".oo").join("patterns"),
            "project.toml",
            "^zz-project-fixture",
        );
        let config = tempfile::tempdir().unwrap();
        write_fixture(
            &config.path().join("patterns"),
            "user.toml",
            "^zz-user-fixture",
        );

        let _guard = crate::test_env::env_guard();
        let prev_config = std::env::var_os("OO_CONFIG_DIR");
        let original_cwd = std::env::current_dir().unwrap();
        unsafe { std::env::set_var("OO_CONFIG_DIR", config.path()) };
        std::env::set_current_dir(root.path()).unwrap();
        let all = all_patterns();
        // Restore before asserting so a failure does not leak cwd/env.
        std::env::set_current_dir(&original_cwd).unwrap();
        match prev_config {
            Some(v) => unsafe { std::env::set_var("OO_CONFIG_DIR", v) },
            None => unsafe { std::env::remove_var("OO_CONFIG_DIR") },
        }

        let builtins = pattern::builtins();
        let position = |m: &str| all.iter().position(|p| p.command_match.as_str() == m);
        let project = position("^zz-project-fixture").expect("project fixture loaded");
        let user = position("^zz-user-fixture").expect("user fixture loaded");
        let first_builtin = all.len() - builtins.len();
        assert!(
            project < user && user < first_builtin,
            "precedence must be project({project}) < user({user}) < builtin({first_builtin})"
        );
        // The builtin tier is the exact tail, in builtin order.
        let tail: Vec<&str> = all[first_builtin..]
            .iter()
            .map(|p| p.command_match.as_str())
            .collect();
        let expected: Vec<&str> = builtins.iter().map(|p| p.command_match.as_str()).collect();
        assert_eq!(tail, expected, "builtin tier must be the trailing entries");
    }
}
