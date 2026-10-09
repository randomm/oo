//! Static usage text for the reserved subcommands.
//!
//! Single source for both `oo <sub> --help|-h` and `oo help <sub>`, so the two
//! help surfaces cannot drift apart. Lookup is local: no cheat.sh, no I/O.

/// Usage text per reserved subcommand. Order matches the top-level help.
const USAGE: &[(&str, &str)] = &[
    (
        "recall",
        "Usage: oo recall [--full] <query>\n\
         \n\
         Search output indexed by earlier oo runs in this project.\n\
         \n\
         Options:\n  --full    Print full matching entries instead of summaries\n\
         \n\
         Note: --full is stripped from anywhere in the query, so it cannot be searched for literally.\n",
    ),
    (
        "forget",
        "Usage: oo forget\n\
         \n\
         Delete all indexed output for the current project. No confirmation is asked.\n",
    ),
    (
        "learn",
        "Usage: oo learn [--hint <text>] <command> [args...]\n\
         \n\
         Run <command>, then ask an LLM (in the background) to generate an output\n\
         compression pattern for it. The pattern is saved under ~/.config/oo/patterns/.\n\
         \n\
         Options:\n  --hint <text>    Extra context for the pattern generator\n",
    ),
    (
        "init",
        "Usage:\n\
         \x20 oo init [--format claude|generic]\n\
         \x20 oo init --agent pi|claude-code [--global]\n\
         \n\
         Set up oo hooks for an agent framework in the current project.\n\
         \n\
         Modes:\n\
         \x20 (no flags)            Write .claude/hooks.json (same as --format claude)\n\
         \x20 --format claude       Write .claude/hooks.json\n\
         \x20 --format generic      Print generic instructions, write nothing\n\
         \x20 --agent pi            Write the pi extension to <project>/.pi/extensions/oo.ts\n\
         \x20 --agent claude-code   Install the Claude Code PreToolUse hook (settings.json)\n\
         \n\
         Options:\n\
         \x20 --agent <agent>       pi | claude-code\n\
         \x20 --global              With --agent: install user-wide instead of per project\n\
         \x20 --format <format>     claude | generic\n\
         \n\
         Rules:\n\
         \x20 --agent and --format cannot be combined.\n\
         \x20 --global requires --agent.\n\
         \x20 Unknown options and stray arguments are errors; nothing is written.\n\
         \n\
         Environment (--global only):\n\
         \x20 OO_PI_EXTENSIONS_DIR  Override the pi extensions directory\n\
         \x20 OO_CLAUDE_DIR         Override the Claude config directory\n",
    ),
    (
        "patterns",
        "Usage: oo patterns\n\
         \n\
         List the output compression patterns in effect (project, user, built-in).\n",
    ),
    (
        "rewrite",
        "Usage: oo rewrite <command>\n\
         \n\
         Print the oo-prefixed form of <command> for agent hooks. Does not run the command.\n",
    ),
    (
        "hook",
        "Usage: oo hook <agent>\n\
         \n\
         Agent hook processor. `oo hook claude` reads a PreToolUse JSON payload on\n\
         stdin.\n",
    ),
    (
        "version",
        "Usage: oo version\n\
         \n\
         Print the oo version.\n",
    ),
];

/// Return the usage text for a reserved subcommand, or `None` for any other name.
pub fn for_subcommand(sub: &str) -> Option<&'static str> {
    USAGE
        .iter()
        .find(|(name, _)| *name == sub)
        .map(|(_, text)| *text)
}
