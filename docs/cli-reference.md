# CLI Reference

## Commands

| Command | Description |
|---------|-------------|
| `oo <cmd> [args...]` | Run a command with context-efficient output |
| `oo recall [--full] <query>` | Search indexed output for this project |
| `oo forget` | Clear all indexed output for this project |
| `oo learn <cmd> [args...]` | Run command and learn an output pattern via LLM |
| `oo help <cmd>` | Fetch a cheat sheet for `cmd` from cheat.sh |
| `oo init` | Set up hooks for agent frameworks (`--format claude` or `--format generic`; or `--agent pi` or `--agent claude-code`, with optional `--global`) |
| `oo hook <agent>` | Agent hook processor (e.g. `oo hook claude` reads PreToolUse JSON on stdin) |
| `oo version` | Print version |
| `oo patterns` | List all loaded patterns (built-in, project, and user) |
| `oo rewrite <command...>` | Print an `oo`-prefixed form of a command for agent hooks (reserved) |

---

## `oo <cmd> [args...]`

Run a shell command through oo's output classification system.

### Usage

```bash
oo cargo test
oo pytest tests/
oo gh issue list --limit 10
```

### Output behavior

oo classifies command output into five tiers:

| Tier | Indicator | Condition |
|------|-----------|-----------|
| **Passthrough** | None | Exit 0 and output ≤ 4 KiB (4096 bytes, unchanged) |
| **Success** | `✓ label (summary) [saved N]` (suffix when the saving exceeds 4 KiB) | Output > 4 KiB with pattern summary |
| **Status** | `✓ label [saved N]` (no parentheses; savings suffix as for Success) | Output > 4 KiB, success, Status category (test runners, builds, linters), no pattern summary; not indexed |
| **Failure** | `✗ label [saved N]` (suffix when the saving exceeds 4 KiB), then a blank line and the filtered error output, then (only when the filtered output does not match the full output line for line, ignoring trailing empty lines) ``● label (output truncated: N total → use `oo recall` to query)``. Full output is indexed in that case. If indexing fails: ``● label (output truncated: N total — NOT indexed, recall unavailable)`` and stderr `oo: warning: could not index output for recall — full output LOST (not recoverable); what follows is a truncated slice only` | Non-zero exit code |
| **Large** | ``● label (indexed N → use `oo recall` to query)``. If indexing fails: the `smart_truncate` view with no indicator line and no stderr warning (known defect, #182) | Output > 4 KiB without a pattern summary, Data category |
| **Bounded** | ``● label (output truncated: N total → use `oo recall` to query)`` then a byte-bounded head+tail slice | Output > 4 KiB without a pattern summary, Content or Unknown category (full output indexed; display is always the bounded slice. If indexing fails: ``● label (output truncated: N total — NOT indexed, recall unavailable)``, the same slice, and stderr `oo: warning: could not index output for recall — full output LOST (not recoverable); what follows is a truncated slice only`. The slice's own marker still reads `use oo recall to query` in both cases, which advertises a recall that does not work when indexing failed; known defect, #182) |

The `saved` figure uses binary units (1 KiB = 1024 bytes), chosen by magnitude: `KiB`, `MiB`, `GiB`, and so on. `B` never appears, because a suffix needs more than 4 KiB saved. A value that is an exact whole number of its unit prints with no decimals (`[saved 5 KiB]`, `[saved 2 MiB]`, `[saved 1 GiB]`); any other value is rounded to two decimal places and always shows both (`[saved 8.68 KiB]`, `[saved 46.10 KiB]`, `[saved 1.50 MiB]`). Rounding can reach a whole number, so a value just under 1 MiB displays as `1024.00 KiB`, and 4,097 bytes saved displays as `4.00 KiB`.

Large unpatterned Data output is indexed in full and retrievable via `oo recall`; entries older than the retention period (7 days by default) are purged the next time the same project indexes output (see [configuration](configuration.md#store-section)); the
display is only the indicator line. If indexing fails, the display falls back to
`smart_truncate` with no indicator line: output of 80 lines or fewer is printed in full.
Output of 81 to 120 lines prints no truncation marker, and its last head line and first tail line are joined into one line with no newline between them (for example, `seq 1 81` shows `4950` for lines 49 and 50; known defect, tracked in #182). Longer output keeps its first 72 and last 48 lines around a
`... [N lines truncated] ...` marker.

Unpatterned Content and Unknown output (the Bounded tier) is indexed in full and displayed as
a byte-bounded head+tail slice of at most 4 KiB of output, separated by a truncation marker of
the form ``... [N bytes truncated → use `oo recall` to query] ...``. The truncation marker is inserted once, between the head and tail pieces (the output itself may contain identical text). It is not necessarily on a line of its own: the head is cut at a byte offset, so it can end partway through a line and the marker then follows that partial line. The tail can likewise start partway through a line. The head and tail shown are an exact prefix and suffix of the full
output. If indexing fails, the indicator line says the output was NOT indexed, a warning is
written to stderr, and the slice is still displayed.

### Savings indicator

When oo compresses output (Success and Failure arms), the savings are reported on the
indicator line itself:

```
✓ cargo test (47 passed, 2.1s) [saved 46.10 KiB]
```

The exact format is `{base_line} [saved {humansize}]` — bracketed, single space after
the existing line content, using `humansize::format_size(saved, BINARY)` (the same
binary-unit idiom the Large tier's `indexed N` figure uses; no new formatter, no new
dependency). The metric is `saved = merged_lossy().len() - rendered_indicator_line_bytes`, where the rendered line is the indicator line EXCLUDING the savings suffix — call sites measure the line before the suffix is appended, so the figure overstates displayed bytes by the suffix's own length (about 14 to 20 bytes, immaterial at the sizes where a suffix can appear). The subtracted amount, per arm, is the indicator line only; printed text beyond it is never subtracted:

- **Success** and **Status** (`✓` line): the indicator line is the whole displayed output, so `saved` is the merged output size minus the displayed text.
- **Failure** (`✗` line): `render_classification` passes the full merged output size (`original_size`) and subtracts only the `✗ label` line. The filtered error output printed after it is not subtracted, so the figure overstates the saving: a failure with 8,893 bytes of output shows `[saved 8.68 KiB]` while its filtered error body is still printed.

Because `saved` subtracts the indicator line, output just over 4 KiB can print no suffix: a 4,100-byte successful `make` prints `✓ make`, and 4,105 bytes prints `✓ make [saved 4.00 KiB]`.

The figure appears on:
- **Success** — both the `✓ label (summary)` form and the quiet `✓ label` empty-summary
  form (the quiet form is the largest compression win in the product: for quiet success
  the figure is therefore approximately the full merged output size minus the short
  indicator line — the intended meaning, not an error)
- **Failure** — the `✗ label` indicator line

The figure does NOT appear on:
- **Passthrough** — output is verbatim, nothing is saved
- **Large** — already reports its size as `indexed N`; no double-reporting
- **Bounded** — the display IS the bounded head+tail slice; the ``● (output truncated:
  N total → use `oo recall` to query)`` framing line already communicates the size
  relationship. The Bounded arm's design purpose is transparency (bounded view +
  recall), not compression, so a savings figure would misframe the arm and
  double-report the size relationship.

The suffix is suppressed unless `saved > MIN_SAVINGS` (4096 bytes, a named constant in
`src/classify.rs` beside `SMALL_THRESHOLD`) — a `[saved 12 B]` suffix on every command
would itself waste context. The threshold is on the same binary scale as
`SMALL_THRESHOLD` to keep the policy coherent.

### Command categories

When no pattern matches, oo uses command category to determine behavior:

| Category | Examples | Behavior |
|----------|----------|----------|
| **Status** | `cargo test`, `cargo build`, `cargo nextest run`, `pytest`, `eslint` | Quiet success if output > 4 KiB and no success pattern summary matches (empty summary) |
| **Content** | `git show`, `git diff`, `cat`, `bat` | Full output indexed; bounded head+tail slice displayed if output > 4 KiB and no success pattern summary matches |
| **Data** | `git log`, `gh issue list`, `ls` | Index for recall if output > 4 KiB and no success pattern summary matches |
| **Unknown** | `curl`, `docker`, `sh -c`, custom scripts | Full output indexed; bounded head+tail slice displayed if output > 4 KiB and no success pattern summary matches |

A pattern whose success strategy yields a summary takes priority over category defaults. Among patterns, project-local, then user, then built-in are checked in that order, and the first match wins. Within one directory, pattern files are loaded in unsorted directory-listing order, so when two patterns in the same directory match a command, which one wins is not defined; keep their `command_match` values disjoint.

### Exit codes

Returns the exit code of the wrapped command.

---

## `oo recall [--full] <query>`

Search indexed output for the current project.

### Usage

```bash
oo recall "error message"
oo recall "test passed"
oo recall "127.0.0"
oo recall --full "error message"
```

### Flags

| Flag | Description |
|------|-------------|
| `--full` | Print the complete stored content instead of the bounded excerpt (deliberately **unbounded** escape hatch — can flood an agent's context; prefer the bounded default) |

`--full` is recognised in any argument position (`oo recall --full q` and
`oo recall q --full` are equivalent). A query that *literally contains* the
word `--full` cannot be searched — the flag is stripped wherever it appears.

### Query behavior

- **Search**: Full-text search across all indexed outputs
- **Minimum length**: Queries of 2+ characters use FTS5 (full-text search); single characters use LIKE pattern matching
- **Limit**: Returns up to 5 most relevant results
- **Scope**: Searches all indexed output for the current project (across all sessions)

### Output format

**Default (bounded excerpt)** — each hit shows at most a 512-char excerpt
(indented per line; the same indent applies to `--full` output):

```
[oo] cargo test (2m ago):
  test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
  running 47 tests [oo: truncated]
```

When the excerpt (FTS5 snippet or client-side prefix) exceeds 512 chars, the
output is truncated to 512 characters plus the unambiguous truncation marker
` [oo: truncated]`. The sentinel — not a bare `…` — means an agent can tell
detectably that content was withheld, and it cannot collide with ellipses in
the stored content or with FTS5's own `…` omission markers.

**`--full`** — each hit shows the complete stored content, indented line-by-line
(**unbounded by design**: a deliberate escape hatch for when the bounded excerpt
isn't enough — it can flood an agent's context, so prefer the bounded default):

```
[oo] cargo test (2m ago):
  test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
  
  running 47 tests
  test tests::it_works ... ok
  ...
```

"project memory" indicates entries from Vipune (when `vipune-store` feature is enabled) without oo metadata.

### Exit codes

| Exit | Condition |
|------|-----------|
| 0 | Success (results may be empty) |
| 1 | Query empty or store error |

---

## `oo forget`

Clear all indexed output for the current project.

### Usage

```bash
oo forget
```

### Behavior

Deletes all outputs indexed for the current project, regardless of which session stored them. With the `vipune-store` build, the deletion can report success while entries remain, because the loop stops after any short page and after any full page with no successful deletion; see the stop rule in [Security Model](security-model.md#command-output). This affects only data stored for `oo recall` — it does not affect patterns or configuration files.

### Output format

```
Cleared project data (12 entries)
```

### Exit codes

| Exit | Condition |
|------|-----------|
| 0 | Success |
| 1 | Store error |

---

## `oo learn <cmd> [args...]`

Run a command and teach oo a new output pattern via LLM.

### Usage

```bash
oo learn terraform plan
oo learn make -j4
oo learn npm test -- --coverage
oo learn --hint "compact test summary" pytest tests/
```

### Behavior

1. Runs the command normally and displays oo-classified output
2. Sends command, output, and exit code to the configured LLM in the background
3. Generates a TOML pattern file and saves it to the user patterns directory as `<label>.toml` (`~/.config/oo/patterns/` on Linux, `~/Library/Application Support/oo/patterns/` on macOS; see [configuration.md](configuration.md#directories))
4. On the next invocation, prints status: `oo: learned pattern for <label> → <path>` (e.g. `oo: learned pattern for cargo-test → …/cargo-test.toml`). Failures are reported on stderr in a `oo: learn failed for <label> — …` form, but only when the config directory already exists; see Exit codes

`--hint <text>` (before the command) passes extra context to the pattern generator.

### Requirements

- The environment variable named by `api_key_env` must be set (default `ANTHROPIC_API_KEY`)
- Optional: `ANTHROPIC_API_URL` for custom endpoints
- Configured in `config.toml` in the configuration directory (`~/.config/oo/config.toml` on Linux, `~/Library/Application Support/oo/config.toml` on macOS; optional, uses defaults if absent; a present `[learn]` table must set `provider`, `model` and `api_key_env`)

### Pattern file naming

The filename is derived from the command:

- `cargo test` → `cargo-test.toml`
- `gh issue list` → `gh-issue.toml` (only the first two words are used)
- `npm --version` → `npm.toml` (flags don't affect naming)

### Overwrite behavior

Running `oo learn` for the same command overwrites the existing pattern file without warning. To preserve a pattern, rename or move the TOML file in the user patterns directory before re-running.

### Exit codes

Returns the exit code of the wrapped command. A background learning failure is written to `learn-status.log` in the config directory and printed to stderr on the next invocation, but only if that config directory already exists when the failure occurs (`oo learn` does not create it on failure). If it does not exist, the failure is silent.

---

## `oo help <cmd>`

`oo help <reserved-subcommand>` (for example `oo help init`) prints the local
usage text instead of a cheat sheet. Other names still come from cheat.sh.

Every reserved subcommand (`recall`, `forget`, `learn`, `help`, `init`, `patterns`,
`rewrite`, `hook`, `version`) accepts `--help` or `-h` as its **first**
argument after the name: it prints that subcommand's usage and exits 0. The subcommand
itself does not run (nothing is searched, learned, rewritten, deleted or installed), but a
pending background-learn status message is printed to stderr and cleared first.
Top-level `oo --help`, `oo -h`, `oo --version` and `oo -V` are handled by argument parsing
before that check: they print nothing from the status file and leave it in place. `oo version`
(no flag) prints and clears it. `oo recall --help` therefore does not search for the text
`--help`. Other commands (e.g. `oo cargo --help`) run as normal.

Fetch a cheat sheet for `cmd` from [cheat.sh](https://cheat.sh).

### Usage

```bash
oo help git
oo help rg
oo help docker
```

### Behavior

- Downloads a cheat sheet from cheat.sh for the specified command
- cheat.sh aggregates content from tldr-pages and other sources
- Modern CLIs not yet in cheat.sh (e.g., `gh`, `kamal`) return an error — use `oo learn` instead

### Exit codes

| Exit | Condition |
|------|-----------|
| 0 | Cheat sheet fetched |
| 1 | Network error or cheat sheet not found |

---

## `oo init [--format <format>] [--agent <agent>] [--global]`

Set up hooks for agent frameworks and print the AGENTS.md integration snippet.
`--agent` and `--format` are mutually exclusive — passing both is an error
(`oo: --agent and --format cannot be used together`, exit 1, nothing
written). `--agent` alone selects the agent installer; `--format` alone and
plain `oo init` behave exactly as before.

Unknown options (`--bogus`), an unknown `--format` value, a bare or flag-like
`--format` with no value, and stray positional arguments are errors (`oo: init:
unknown option '--bogus' (try: oo init --help)`, exit 1, nothing written).

Each flag may be given once; a repeated `--agent`, `--format` or `--global` is an
error, in any order.

**CLI contract change (since v0.8.0; these inputs were accepted in v0.7.0):** they now
exit 1 with nothing written: an unknown option such as `--bogus` (silently ignored), including
`--format=<value>` and `--agent=<value>` (the `=` form was ignored, so the claude format was written);
a stray positional argument (silently ignored); a repeated `--format`, `--agent` or `--global` (silently
accepted, the first `--format` or the last `--agent` won); an unknown `--format` value (a warning on
stderr, then the claude format); a bare `--format` with no value (the claude format, silently); and
`--format` followed by a flag-like token such as `--bogus` (the token was taken as the value, with
the unknown-value warning, then the claude format).

### Usage

```bash
oo init
oo init --format claude
oo init --format generic
oo init --agent pi
oo init --agent pi --global
```

### Formats (`--format`)

| Format | Description |
|--------|-------------|
| `claude` (default) | Generates `.claude/hooks.json` and Claude-specific AGENTS.md instructions |
| `generic` | Prints AGENTS.md instructions only (no hooks file) |

### Agents (`--agent`)

| Agent | Description |
|-------|-------------|
| `pi` | Installs a pi (pi-coding-agent) TypeScript extension that rewrites bash tool calls via `oo rewrite` |
| `claude-code` | Merges a `oo hook claude` PreToolUse hook entry into `.claude/settings.json` (project) or `~/.claude/settings.json` (`--global`) |

`--agent` and `--format` cannot be combined (the combination errors — see
above). Unknown agent values, and `--agent` with no value, error naming the
supported values (`pi`, `claude-code`).

### File locations (`--agent pi`)

| Scope | Path |
|-------|------|
| project (default) | `<git-root>/.pi/extensions/oo.ts` (cwd when not in a git repo) |
| global (`--global`) | `$OO_PI_EXTENSIONS_DIR/oo.ts` when `OO_PI_EXTENSIONS_DIR` is set (the variable is the final extensions directory — no path suffix is appended); otherwise `~/.pi/agent/extensions/oo.ts`. Never consults the git root |

### File locations (`--agent claude-code`)

| Scope | Path |
|-------|------|
| project (default) | `<git-root>/.claude/settings.json` (cwd when not in a git repo) |
| global (`--global`) | `$OO_CLAUDE_DIR/settings.json` when `OO_CLAUDE_DIR` is set (the variable is the final config directory — no path suffix is appended); otherwise `~/.claude/settings.json`. Never consults the git root |

`OO_PI_EXTENSIONS_DIR` and `OO_CLAUDE_DIR` are trusted paths: when set, their values are used as-is — the file is written directly into the named directory, with no sanitisation or additional path components. A set-but-empty variable is an error (exit 1, nothing written); with the variable unset, `HOME` is used instead (unset or empty `HOME` is likewise an error).

### Pi extension behaviour

- At load, probes `oo --version` and updates its `ooAvailable` flag (initially true) so that a missing or erroring `oo` (non-zero exit, timeout, exception) hard-disables the handler: it returns undefined for every tool call without invoking `oo rewrite` (status note only).
- On bash `tool_call`: skips empty commands, commands already starting with `oo ` (including `oo rewrite` itself), and nested calls (`parentToolCallId` set — codemode scripts see raw output).
- Honors the `OO_DISABLE=1` environment opt-out.
- Calls `oo rewrite <cmd>` (2000 ms timeout) and swaps in the rewritten command only when it is exit 0, non-empty, and different.
- Fails open: any handler error passes the command through unmodified.

Install is idempotent: an existing identical file is a no-op ("already installed"); an existing different file is not overwritten and the command explains how to proceed. Uninstall by deleting the installed `oo.ts` file.

### Behavior

- `--agent pi` creates `.pi/extensions/oo.ts` (idempotent, never overwrites a different file)
- `--agent claude-code` merges a `oo hook claude` PreToolUse entry into `.claude/settings.json` (project) or `~/.claude/settings.json` (`--global`); idempotent, never overwrites a malformed file
- `--format claude` (default) and plain `oo init` create `.claude/hooks.json` (legacy)
- Prints a snippet to add to your project's `AGENTS.md` file (format modes)
- The snippet instructs agents to prefix commands with `oo`

`--format claude` (the legacy `.claude/hooks.json`, which only blocks `--help`/`-h` calls) and `--agent claude-code` (the rewriting hook in `.claude/settings.json`) are **independent installers** — one does not imply the other; install both for full Claude Code coverage. `OO_CLAUDE_DIR` is a trusted path used as-is (like `OO_PI_EXTENSIONS_DIR`).

### Output format

```
Created /path/to/project/.claude/hooks.json

Add this to your AGENTS.md:

## Shell commands
Prefix all shell commands with `oo`. Use `oo recall "<query>"` to search large outputs (use `--full` for the complete output).
```

If `.claude/hooks.json` already exists it is left untouched: an `oo init: <path> already exists — skipping (delete it to regenerate)` line goes to stderr, the AGENTS.md snippet is still printed, and the exit code is 0.

### Exit codes

| Exit | Condition |
|------|-----------|
| 0 | Success |
| 1 | File write error |

---

## `oo version`

Print the oo version.

### Usage

```bash
oo version
```

### Output format

```
oo <version>
```

### Exit codes

Always returns 0.

---

## `oo patterns`

List all loaded patterns: built-in patterns, project-local patterns, and user patterns from the user patterns directory (`oo patterns` prints its path).

### Usage

```bash
oo patterns
```

### Output format

Patterns are grouped under section headers — `Built-in (N patterns):`, `Project (dir):` (only when project-local patterns exist), and `User (dir):`. Each pattern line shows its command regex plus the `[success]` and/or `[failure]` flags for the strategies it defines (two-space indent):

```
Built-in (N patterns):
  \\bcargo\\s+test\\b  [success] [failure]
  \\bpytest\\b

User (<user patterns dir>):
  \\bterraform\\s+plan\\b  [success]
```

If the user directory doesn't exist or contains no valid TOML files, its `User (dir):` header is still printed with no pattern lines beneath it. `oo patterns` does not print a "no learned patterns" message.

Files that are not valid TOML are skipped silently. Any file that parses as TOML is listed, including a file with no `command_match` (shown as `(unknown)`); a file with valid TOML but an invalid regex is also still listed, although it is not loaded. The `Built-in` and `User` headers are always printed; the `Project` header is printed only when the project patterns directory exists.

### Exit codes

| Exit | Condition |
|------|-----------|
| 0 | Success (directory may be empty) |

---

## `oo rewrite <command...>`

Print an `oo`-prefixed version of a shell command for agent hooks. It never
executes the command and never reads from or writes to the output index store.
This is a reserved subcommand — see [`oo-rewrite.md`](oo-rewrite.md) for the
full reference: behavior, exit code contract, examples, and the list of
refused constructs.

Matching is **unanchored and per segment**: a segment is rewritten when any
pattern's `command_match` regex matches anywhere in the segment text (the
same patterns and matching as `oo <command>`; quoted text is not excluded).
The rewritten output preserves the original text byte-for-byte except for
the inserted `oo ` prefix(es) and canonical separator spacing. Exit 1 also
covers a pattern file that failed to load, an unquoted newline inside a
segment, and an env value containing a quote or backslash.

---

## `oo hook claude`

Stdin-JSON processor for Claude Code PreToolUse hooks (installed by `oo init --agent claude-code`). Reads a PreToolUse event as JSON on stdin and, for a Bash tool call whose command has an `oo` rewrite, prints a `hookSpecificOutput` object with the rewritten command on stdout and exits 0. For everything else (no rewrite, non-Bash, invalid/empty JSON, missing fields) it prints nothing and exits 0 — **fail-open**.

### Stdin / stdout contract

**Input (stdin):** `PreToolUse` JSON object
```json
{
  "tool_name": "Bash",
  "tool_input": { "command": "cargo test", "description": "view history", "timeout": 12000 }
}
```

**Output (stdout, when a rewrite applies):**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecisionReason": "oo auto-rewrite",
    "updatedInput": { "command": "oo cargo test", "description": "view history", "timeout": 12000 }
  }
}
```

**Output (no rewrite / non-Bash / invalid input):** empty (no output, exit 0).

- Only `Bash` tool calls are rewritten; other tools pass through unchanged.
- All `tool_input` fields are preserved in `updatedInput` (only `command` is replaced); JSON escaping is handled by `serde_json`.
- Compound commands are rewritten per segment and pipes are left alone (per `oo rewrite` semantics).
- `OO_DISABLE=1` disables only the hook's rewriting: the hook passes commands through untouched (no rewrite, no output, exit 0). It does **not** disable the `oo` runner itself.

---

## Exit Codes for Automation

All oo commands return standard exit codes:

| Exit | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (invalid args, command failure, or store error) |
| non-zero | Wrapped command's exit code (for `oo <cmd>` and `oo learn`) |

For automation scripts, check the exit code:

```bash
oo cargo test --release
if [ $? -eq 0 ]; then
  echo "Tests passed"
fi
```

`oo recall` returns 0 even when no results are found — it distinguishes between "search successful but empty" and "search failed".