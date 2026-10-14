# Agent Integration

`oo` can rewrite the shell commands an AI agent runs so they go through `oo`
and come back compressed. This page covers the supported agents (pi and Claude
Code), other agents, disabling, uninstalling, and what to commit.

For flags, exit codes and file paths, see the [CLI reference](cli-reference.md#oo-init---format-format---agent-agent---global).

## Scope: only matching commands are rewritten

An install only rewrites commands that match an `oo` pattern (built-in, project
or user). Everything else runs unchanged. For example, `pytest -q` becomes
`oo pytest -q`, while `ls -la` is left alone.

- Built-in patterns: `oo patterns` lists them with their command regexes.
- Extend coverage: write a pattern in [Custom Patterns](patterns.md), or generate one with `oo learn <cmd>` ([Learning Patterns](learn.md)).
- Rewrite rules in detail (pipes, redirections, `&&`/`||`/`;`, env prefixes): [`oo rewrite`](oo-rewrite.md).

Because only matching commands are rewritten, the agent is not taught `oo recall`
by the installer. Add the recall line yourself (see [Recall instruction](#recall-instruction)).

## Recall instruction

`oo init --agent` prints no prompt snippet. Add this line to the file your agent reads:

```
Prefix all shell commands with `oo`. Use `oo recall "<query>"` to search large outputs.
```

- Claude Code: `CLAUDE.md` (Claude Code does not read `AGENTS.md`).
- AGENTS.md-aware agents (pi, Codex and others): `AGENTS.md`.

Without this line the agent only learns about `oo recall` from the `●` hint on
indexed output.

## pi

### Install

```bash
oo init --agent pi           # project: <git-root>/.pi/extensions/oo.ts
oo init --agent pi --global  # user:    ~/.pi/agent/extensions/oo.ts
```

With `OO_PI_EXTENSIONS_DIR` set, `--global` writes `$OO_PI_EXTENSIONS_DIR/oo.ts`.
The variable is the final directory, so no `/oo.ts` suffix is appended to it. Outside a git repository the project file goes in the current directory.

Re-running prints `… already installed — nothing to do`. An existing file that
differs from the generated one is not overwritten.

The extension hooks pi's bash tool calls and calls `oo rewrite` on each command.
It does not touch nested (codemode) tool calls.

### Verify

```bash
oo rewrite pytest -q   # prints: oo pytest -q          (exit 0)
oo rewrite ls -la      # prints nothing                 (exit 1, runs unchanged)
oo patterns            # lists the loaded patterns
```

A working session shows these lines in the tool output:

- `✓ <label> (<summary>)`: a matched command succeeded.
- `✗ <label>`: a matched command failed; the failure tail follows.
- `● <label> (output truncated … use oo recall …)`: the full output was indexed. Query it with `oo recall`.

### Disable

Set `OO_DISABLE=1` in the environment pi starts with. The extension then passes
every command through unchanged.

### Uninstall

Delete the installed `oo.ts` file (project or global).

### Failure mode: `oo` not on pi's PATH

The extension probes `oo --version` once, when it loads. If the probe fails
(binary missing, error or timeout), it sets `ooAvailable` to false and every
command runs unchanged. The status bar shows `oo disabled: <reason>`.

- Symptom: `oo disabled` in the status bar; commands show raw output.
- Fix: make `oo` resolvable on the PATH that launches pi (for example, add `~/.cargo/bin` to it), then restart pi. The probe runs only at load.

## Claude Code

### Install

```bash
oo init --agent claude-code           # project: <git-root>/.claude/settings.json
oo init --agent claude-code --global  # user:    ~/.claude/settings.json
```

With `OO_CLAUDE_DIR` set, `--global` writes `$OO_CLAUDE_DIR/settings.json`.
The variable is the final config directory. The installer merges a
`oo hook claude` PreToolUse entry for the `Bash` matcher into the file, keeps
other settings, and is idempotent. It refuses to overwrite a malformed
`settings.json`.

Each Bash call is piped to `oo hook claude`, which prints the rewritten command
as `updatedInput.command`. Other tools pass through.

### Verify

```bash
printf '%s' '{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"cargo test"}}' | oo hook claude
# {"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecisionReason":"oo auto-rewrite","updatedInput":{"command":"oo cargo test"}}}

printf '%s' '{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls -la"}}' | oo hook claude
# (no output, exit 0: no pattern matches, so the command runs unchanged)
```

Then run a matched command in a Claude Code session and look for the `✓`, `✗` or `●` lines above.

### Disable

Set `OO_DISABLE=1` in the environment Claude Code starts with. The hook then
passes every command through (no output, exit 0). This does not disable the `oo` runner itself.

### Uninstall

Remove the `oo hook claude` entry from `settings.json` (project or global).

### Failure modes

- **Hook fails open for its own errors.** Empty input, invalid JSON or missing fields print nothing and exit 0, so the original command runs.
- **`oo` not on PATH.** The hook command is `oo hook claude`, looked up by name. If `oo` is not found, the shell cannot start the hook and the command is not rewritten. The fail-open behaviour above does not cover a missing binary.

## What to commit

| Install | Files | Commit? |
|---|---|---|
| Project (`--agent pi`, `--agent claude-code`) | `.pi/extensions/oo.ts`, `.claude/settings.json` | Yes, if the team uses `oo`. Teammates without `oo` on PATH run commands unrewritten. |
| Global (`--global`) | `~/.pi/agent/extensions/oo.ts`, `~/.claude/settings.json` | Not in the repo; these are per-user. |

- Project `settings.json` is shared as a whole. Review the diff before committing if it holds other settings.
- Project installs are fail-open for pi: without `oo`, the extension disables itself and runs commands unchanged.
- Project installs are not fail-open for Claude Code when `oo` is missing (see [Failure modes](#failure-modes)). Commit the hook only if every teammate has `oo` on PATH.
- The recall line (see [Recall instruction](#recall-instruction)) belongs in `CLAUDE.md` or `AGENTS.md`, which you commit.

## Other agents

For Codex, Cursor, Aider, opencode and other agents without an `oo` installer:

```bash
oo init --format generic
```

This prints the AGENTS.md instruction and writes nothing. Put the printed text in the instruction file the agent reads.

Without a hook, nothing rewrites commands. The agent must choose to prefix commands with `oo` itself. Check its output for the `✓`, `✗` and `●` lines.

## Legacy: plain `oo init`

Plain `oo init` (same as `oo init --format claude`) is legacy. It writes `.claude/hooks.json`, which only blocks `--help` and `-h` calls and tells the agent to use `oo help <cmd>`. It does **not** rewrite commands and is independent of `--agent claude-code`. It also prints an AGENTS.md snippet. It does not overwrite an existing `hooks.json`.

To get rewriting in Claude Code, use `oo init --agent claude-code`. Install both only if you want the legacy block as well.
