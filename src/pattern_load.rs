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

use std::path::Path;
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
    compose(load_project_patterns(), &learn::patterns_dir())
}

/// Precedence composition over explicit tier roots. Split out so the precedence
/// test passes its fixtures in rather than mutating the process cwd or env.
fn compose(mut all: Vec<Pattern>, user_dir: &Path) -> Vec<Pattern> {
    all.extend(pattern::load_user_patterns(user_dir));
    all.extend_from_slice(pattern::builtins());
    all
}

/// Load project-local patterns from `<git-root>/.oo/patterns/`.
///
/// Returns an empty vec when cwd cannot be determined or the directory does
/// not exist (gracefully handled by `load_user_patterns`).
pub fn load_project_patterns() -> Vec<Pattern> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    project_patterns_in(&cwd)
}

fn project_patterns_in(cwd: &Path) -> Vec<Pattern> {
    pattern::load_user_patterns(&find_root(cwd).join(".oo").join("patterns"))
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
        // Each fixture has a unique `command_match`, so the tiers are located by
        // marker and no live user or project directory is consulted. The test never
        // changes cwd or env: a cwd left inside a temp dir that is then deleted
        // leaks a dead cwd to any child process spawned meanwhile.
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join(".git")).unwrap();
        let project_dir = project.path().join(".oo").join("patterns");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(
            project_dir.join("project.toml"),
            "command_match = \"^oo-precedence-project$\"\n",
        )
        .unwrap();

        let config = tempfile::tempdir().unwrap();
        let user_dir = config.path().join("patterns");
        std::fs::create_dir_all(&user_dir).unwrap();
        std::fs::write(
            user_dir.join("user.toml"),
            "command_match = \"^oo-precedence-user$\"\n",
        )
        .unwrap();

        let all = compose(project_patterns_in(project.path()), &user_dir);
        let position = |marker: &str| all.iter().position(|p| p.command_match.as_str() == marker);
        let project_at = position("^oo-precedence-project$").expect("project tier missing");
        let user_at = position("^oo-precedence-user$").expect("user tier missing");

        // Builtins are the last tier, in declared order. First-match-wins then gives
        // project over user over builtin.
        let builtins = pattern::builtins();
        let builtin_start = all
            .len()
            .checked_sub(builtins.len())
            .expect("all_patterns must contain every builtin");
        assert!(
            project_at < user_at && user_at < builtin_start,
            "precedence must be project({project_at}) < user({user_at}) < builtin({builtin_start})"
        );
        for (got, want) in all[builtin_start..].iter().zip(builtins) {
            assert_eq!(
                got.command_match.as_str(),
                want.command_match.as_str(),
                "builtin tier must be last, in declared order"
            );
        }
    }
}
