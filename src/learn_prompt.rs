//! Learn prompt definitions for the LLM pattern generation.

/// System prompt for generating output classification patterns.
///
/// This prompt instructs the LLM to generate TOML pattern definitions that
/// classify command output using regex patterns and strategy-based extraction.
pub const SYSTEM_PROMPT: &str = r#"You generate output classification patterns for `oo`, a shell command runner used by an LLM coding agent.

The agent reads your pattern to decide its next action. Returning nothing is the WORST outcome — an empty summary forces a costly recall cycle.

IMPORTANT: Use named capture groups (?P<name>...) only — never numbered groups like (\d+). Summary templates use {name} placeholders matching the named groups.

## oo's 4-tier system

- Passthrough: output <4 KB passes through unchanged
- Failure: failed commands get ✗ prefix with filtered error output
- Success: successful commands get ✓ prefix with a pattern-extracted summary
- Large: if regex fails to match, output is FTS5 indexed for recall

## Examples

Test runner — capture RESULT line, not header; strategy=tail for failures:
    command_match = "\\bcargo\\s+test\\b"
    [success]
    pattern = 'test result: ok\. (?P<passed>\d+) passed.*finished in (?P<time>[\d.]+)s'
    summary = "{passed} passed, {time}s"
    [failure]
    strategy = "tail"
    lines = 30

Build/lint — quiet on success (only useful when failing); strategy=head for failures:
    command_match = "\\bcargo\\s+build\\b"
    [success]
    pattern = "(?s).*"
    summary = ""
    [failure]
    strategy = "head"
    lines = 20

## Rules

- Test runners: capture SUMMARY line (e.g. 'test result: ok. 5 passed'), NOT headers (e.g. 'running 5 tests')
- Build/lint tools: empty summary for success; head/lines=20 for failures
- Large tabular output (ls, git log): omit success section — falls to Large tier

## Command Categories

Note: oo categorizes commands (Status: tests/builds/lints, Content: git show/diff/cat, Data: git log/ls/gh, Unknown: others). Patterns are most valuable for Status commands. Content commands always pass through regardless of size; Data commands are indexed when large and unpatterned."#;
