# CLI Reference

## Commands

| Command | Description |
|---------|-------------|
| `oo <cmd> [args...]` | Run a command with context-efficient output |
| `oo recall <query>` | Search indexed output from this session |
| `oo forget` | Clear all indexed output for this session |
| `oo learn <cmd> [args...]` | Run command and learn an output pattern via LLM |
| `oo help <cmd>` | Fetch a cheat sheet for `cmd` from cheat.sh |
| `oo init` | Generate `.claude/hooks.json` and print AGENTS.md snippet |
| `oo version` | Print version |
| `oo patterns` | List all learned patterns |

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

oo classifies command output into four tiers:

| Tier | Indicator | Condition |
|------|-----------|-----------|
| **Passthrough** | None | Output ≤ 4 KB (unchanged) |
| **Success** | `✓ label (summary)` | Output > 4 KB with pattern match |
| **Failure** | `✗ label` followed by error output | Non-zero exit code |
| **Large** | `● label (indexed N → use oo recall)` | Output > 4 KB without pattern (Data commands only) |

### Command categories

When no pattern matches, oo uses command category to determine behavior:

| Category | Examples | Behavior |
|----------|----------|----------|
| **Status** | `cargo test`, `pytest`, `eslint`, `cargo build` | Quiet success if output > 4 KB (empty summary) |
| **Content** | `git show`, `git diff`, `cat`, `bat` | Always pass through, never index |
| **Data** | `git log`, `gh issue list`, `ls` | Index for recall if output > 4 KB |
| **Unknown** | `curl`, `docker`, custom scripts | Pass through (safe default) |

Patterns always take priority over category defaults.

### Exit codes

Returns the exit code of the wrapped command.

---

## `oo recall <query>`

Search indexed output from the current session.

### Usage

```bash
oo recall "error message"
oo recall "test passed"
oo recall "127.0.0"
```

### Query behavior

- **Search**: Full-text search across all indexed outputs
- **Minimum length**: Queries of 2+ characters use FTS5 (full-text search); single characters use LIKE pattern matching
- **Limit**: Returns up to 5 most relevant results
- **Scope**: Only searches outputs from the current session (project and process)

### Output format

Each result includes metadata and indented content:

```
[session] gh issue list (2m ago):
  #1: Feature request
  #2: Bug report

[session] cargo test (5m ago):
  test result: ok. 47 passed; 0 failed
```

"project memory" indicates entries from Vipune (when `vipune-store` feature is enabled) without session metadata.

### Exit codes

| Exit | Condition |
|------|-----------|
| 0 | Success (results may be empty) |
| 1 | Query empty or store error |

---

## `oo forget`

Clear all indexed output for the current session.

### Usage

```bash
oo forget
```

### Behavior

Deletes all outputs indexed in the current session identified by project ID and process ID. This affects only data stored for `oo recall` — it does not affect patterns or configuration files.

### Output format

```
Cleared session data (12 entries)
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
```

### Behavior

1. Runs the command normally and displays oo-classified output
2. Sends command, output, and exit code to the configured LLM in the background
3. Generates a TOML pattern file and saves it to `~/.config/oo/patterns/<label>.toml`
4. On the next invocation, prints status: `oo: learned pattern for "<cmd>" -> <path>`

### Requirements

- `ANTHROPIC_API_KEY` environment variable must be set
- Optional: `ANTHROPIC_API_URL` for custom endpoints
- Configured in `~/.config/oo/config.toml` (optional, uses defaults if absent)

### Pattern file naming

The filename is derived from the command:

- `cargo test` → `cargo-test.toml`
- `gh issue list` → `gh-issue-list.toml`
- `npm --version` → `npm.toml` (flags don't affect naming)

### Overwrite behavior

Running `oo learn` for the same command overwrites the existing pattern file without warning. To preserve a pattern, rename or move the TOML file in `~/.config/oo/patterns/` before re-running.

### Exit codes

Returns the exit code of the wrapped command. Learning failures are reported in stderr on the next invocation.

---

## `oo help <cmd>`

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

## `oo init [--format <format>]`

Generate Claude-specific hook configuration and print AGENTS.md integration snippet.

### Usage

```bash
oo init
oo init --format claude
oo init --format generic
```

### Formats

| Format | Description |
|--------|-------------|
| `claude` (default) | Generates `.claude/hooks.json` and Claude-specific AGENTS.md instructions |
| `generic` | Prints AGENTS.md instructions only (no hooks file) |

### Behavior

- Creates `.claude/hooks.json` if it doesn't exist (for Claude format)
- Prints a snippet to add to your project's `AGENTS.md` file
- The snippet instructs agents to prefix commands with `oo`

### Output format

```
Generated .claude/hooks.json

Add this to your AGENTS.md:

Prefix all shell commands with `oo`. Use `oo recall "<query>"` to search large outputs.
```

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
oo 0.4.0
```

### Exit codes

Always returns 0.

---

## `oo patterns`

List all learned patterns from `~/.config/oo/patterns/`.

### Usage

```bash
oo patterns
```

### Output format

Each pattern shows its command regex and components:

```
\\bcargo\\s+test\\b  [success] [failure]
\\bpytest\\b
\\bterraform\\s+plan\\b  [success]
```

If the directory doesn't exist or contains no valid TOML files:

```
no learned patterns yet
```

Invalid or corrupt TOML files are skipped silently.

### Exit codes

| Exit | Condition |
|------|-----------|
| 0 | Success (directory may be empty) |

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