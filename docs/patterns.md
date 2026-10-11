# Custom Patterns

Patterns are `.toml` files — one per command. They are loaded from two locations
in order, with the first regex match winning:

1. **Project:** `<git-root>/.oo/patterns/` — repo-specific, checked in with the project
2. **User:** the user patterns directory (`~/.config/oo/patterns/` on Linux, `~/Library/Application Support/oo/patterns/` on macOS) — personal patterns across all projects

Both layers are checked before built-in patterns, so custom patterns override built-ins for normal command classification. The order is project, then user, then built-in, and the first matching pattern wins. Files within one directory are loaded in unsorted directory-listing order, so if two patterns in the same directory match a command, which one wins is not defined; keep their `command_match` values disjoint. (The foreground display in `oo learn` uses built-in patterns only; see #151.)

## TOML format

```toml
# Regex matched against the full command string (e.g. "make -j4 all")
command_match = "^make\\b"

[success]
# Regex with named captures run against stdout+stderr
pattern = '(?P<target>\S+) is up to date'
# Template: {name} is replaced with the capture of the same name
summary = "{target} up to date"

[failure]
# Strategy: tail | head | grep | between
strategy = "grep"
# For grep: lines matching this regex are kept
grep = "Error:|error\\["
```

## `[success]` section

`strategy` is optional and defaults to `"regex"`.

| `strategy` | Required fields | Optional fields | Behaviour |
|------------|-----------------|-----------------|-----------|
| `regex` (default) | `pattern` (regex with named captures), `summary` (template) | — | Named captures become template variables in `summary` |
| `tail` | — | `lines` (default 30) | Summary is the last N lines of output |
| `head` | — | `lines` (default 20) | Summary is the first N lines of output |
| `grep` | `grep` (regex) | — | Lines matching the regex become the summary |

An empty `summary = ""` suppresses output on success (quiet pass) — the indicator
line is `✓ label` with no summary. The savings figure (`[saved N]`) is still
appended when the saving exceeds `MIN_SAVINGS` (4 KiB), so the compression win is
visible even on the quiet form. This makes the quiet path the largest compression
win in the product: a 100 KiB build log becomes a single line with a savings figure,
and the agent can see exactly how much was saved.

## `[failure]` section

`strategy` is optional and defaults to `"tail"`.

| `strategy` | Extra fields | Behaviour |
|------------|-------------|-----------|
| `tail` | `lines` (default 30) | Last N lines of output |
| `head` | `lines` (default 20) | First N lines of output |
| `grep` | `grep` (regex, required) | Lines matching regex |
| `between` | `start`, `end` (strings, required) | Lines from first `start` match to first `end` match (inclusive) |

Omit `[failure]` to show all output on failure.

## Examples

### `docker build`

```toml
command_match = "\\bdocker\\s+build\\b"

[success]
pattern = 'Successfully built (?P<id>[0-9a-f]+)'
summary = "built {id}"

[failure]
strategy = "tail"
lines = 20
```

### `terraform plan`

```toml
command_match = "\\bterraform\\s+plan\\b"

[success]
pattern = 'Plan: (?P<add>\d+) to add, (?P<change>\d+) to change, (?P<destroy>\d+) to destroy'
summary = "+{add} ~{change} -{destroy}"

[failure]
strategy = "grep"
grep = "Error:|error:"
```

### `make`

```toml
command_match = "^make\\b"

[success]
pattern = '(?s).*'   # always matches; empty summary = quiet
summary = ""

[failure]
strategy = "between"
start = "make["
end = "Makefile:"
```

> **Note:** `start` and `end` are plain substring matches, not regexes.

## Command Categories

oo categorizes commands to determine default behavior when no pattern matches:

| Category | Examples | Default Behavior |
|----------|----------|------------------|
| **Status** | `cargo test`, `pytest`, `eslint`, `cargo build` | Quiet success (empty summary) if output > 4 KB |
| **Content** | `git show`, `git diff`, `cat`, `bat` | Full output indexed, bounded head+tail slice displayed if output > 4 KB |
| **Data** | `git log`, `git status`, `gh api`, `ls`, `find` | Index for recall if output > 4 KB and unpatterned |
| **Unknown** | Anything else (curl, docker, `sh -c`, etc.) | Full output indexed, bounded head+tail slice displayed if output > 4 KB |

**Important:** A pattern whose success strategy yields a summary takes priority over category defaults, regardless of category. If a pattern has no success strategy, or its extraction yields nothing, the category default applies. A failure strategy, if present, selects the lines shown for failing runs; without one, the default truncation applies.

The full set of detected binaries and their categories is defined by `detect_category()` in [`src/classify.rs`](../src/classify.rs) — the code is the source of truth; the table above shows representative examples per category.

The savings indicator (when and where the `[saved N]` figure appears) is specified in [docs/cli-reference.md](cli-reference.md#savings-indicator).
