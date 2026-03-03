<div style="font-size: 20px">
o̵̥̟͓̿͛̚õ̵̙͈̝̚
</div>

Context-efficient command runner for AI coding agents.

---

## The Problem

AI agents see everything you print. A `cargo test` run producing 8 KB of output
costs the agent hundreds of tokens just to learn "tests passed." Multiply that
across a session and the context window fills with noise, not signal. `oo` runs
commands for you and collapses their output to what the agent actually needs.

---

## Output Tiers

```
$ oo ls -la
total 48
drwxr-xr-x  8 user user 4096 Mar  2 09:00 .
...
```
Small output (≤ 4 KB) passes through verbatim — no wrapping, no prefix.

```
$ oo pytest tests/
✓ pytest (47 passed, 3.2s)
```
Large output with a known success pattern collapses to a single summary line.

```
$ oo pytest tests/
✗ pytest

FAILED tests/test_api.py::test_login - AssertionError: expected 200, got 401
...
[last 30 lines of output]
```
Failure output is filtered to the actionable tail (or custom strategy per tool).

```
$ oo gh issue list
● gh (indexed 47.2 KiB → use `oo recall` to query)
```
Large unrecognised output is indexed locally; query it with `oo recall`.

---

## Install

```bash
cargo install double-o
```

---

## Commands

| Command | Description |
|---|---|
| `oo <cmd> [args...]` | Run a shell command with context-efficient output |
| `oo recall <query>` | Search indexed output from this session |
| `oo forget` | Clear all indexed output for this session |
| `oo learn <cmd> [args...]` | Run command and teach `oo` a new output pattern via LLM |
| `oo help <cmd>` | Fetch a cheat sheet for `cmd` from cheat.sh |
| `oo init` | Generate `.claude/hooks.json` and print AGENTS.md snippet |
| `oo version` | Print version |

---

## Agent Integration

Add this to your system prompt or `CLAUDE.md`:

```
Prefix all shell commands with `oo`. Use `oo recall "<query>"` to search large outputs.
```

That's it. The agent runs `oo cargo test`, gets `✓ cargo test (53 passed, 1.4s)`,
and moves on.

---

## Built-in Patterns

`oo` ships with 10 patterns that match commands automatically:

| Command | Success | Failure strategy |
|---|---|---|
| `pytest` | `{passed} passed, {time}s` | tail 30 lines |
| `cargo test` | `{passed} passed, {time}s` | tail 40 lines |
| `go test` | `ok ({time}s)` | tail 30 lines |
| `jest` / `vitest` | `{passed} passed, {time}s` | tail 30 lines |
| `ruff check` | quiet (no output on pass) | smart truncate |
| `eslint` | quiet | smart truncate |
| `cargo build` | quiet | head 20 lines |
| `go build` | quiet | head 20 lines |
| `tsc` | quiet | head 20 lines |
| `cargo clippy` | quiet | smart truncate |

Add your own patterns with `oo learn <cmd>` (generates a TOML pattern file via
LLM) or write one manually in `~/.local/share/oo/patterns/`.

---

## License

Apache-2.0 — see [LICENSE](LICENSE).

`oo help` fetches content from [cheat.sh](https://cheat.sh), which includes
[tldr-pages](https://github.com/tldr-pages/tldr) content (CC BY 4.0). See
[NOTICE](NOTICE).
