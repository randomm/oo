```
  ██████   ██████ 
 ███▒▒███ ███▒▒███
▒███ ▒███▒███ ▒███
▒███ ▒███▒███ ▒███
▒▒██████ ▒▒██████ 
 ▒▒▒▒▒▒   ▒▒▒▒▒▒ 
```

<br /><br />

[![CI](https://github.com/randomm/oo/actions/workflows/ci.yml/badge.svg)](https://github.com/randomm/oo/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/double-o.svg)](https://crates.io/crates/double-o)
[![docs.rs](https://docs.rs/double-o/badge.svg)](https://docs.rs/double-o)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-brightgreen.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

*"Double-o, agent's best friend."* 

*Or: how I learned to stop worrying and love my context-efficient command runner for AI coding agents.*

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
Output handling depends on command category: content commands like `git show` and `git diff` always pass through regardless of size, while data commands like `git log` and `ls` are indexed when large. See the [patterns guide](docs/patterns.md#command-categories) for details.

---

## Installation

### Pre-built binaries (recommended)

Download from [GitHub Releases](https://github.com/randomm/oo/releases/latest):

```bash
# macOS (Apple Silicon)
curl -LO https://github.com/randomm/oo/releases/latest/download/double-o-aarch64-apple-darwin.tar.xz
tar xf double-o-aarch64-apple-darwin.tar.xz
sudo mv oo /usr/local/bin/

# Linux (x86_64)
curl -LO https://github.com/randomm/oo/releases/latest/download/double-o-x86_64-unknown-linux-gnu.tar.xz
tar xf double-o-x86_64-unknown-linux-gnu.tar.xz
sudo mv oo /usr/local/bin/

# Linux (ARM64)
curl -LO https://github.com/randomm/oo/releases/latest/download/double-o-aarch64-unknown-linux-gnu.tar.xz
tar xf double-o-aarch64-unknown-linux-gnu.tar.xz
sudo mv oo /usr/local/bin/
```

### From crates.io

```bash
cargo install double-o
```

### From source

```bash
git clone https://github.com/randomm/oo.git
cd oo
cargo build --release
cp target/release/oo /usr/local/bin/
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

> **Note:** `oo help` sources from [cheat.sh](https://cheat.sh) which covers common Unix tools. For modern CLIs not yet in cheat.sh (e.g., `gh`, `kamal`), use `oo learn <cmd>` to teach `oo` the command's output patterns.

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
