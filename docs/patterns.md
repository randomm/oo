# Custom Patterns

Patterns live in `~/.config/oo/patterns/` as `.toml` files — one per command.
User patterns are checked before built-ins, so they override existing behaviour.

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

| Field | Type | Description |
|-------|------|-------------|
| `pattern` | regex | Named captures become template variables |
| `summary` | string | Template; `{capture_name}` replaced at runtime |

An empty `summary = ""` suppresses output on success (quiet pass).

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
