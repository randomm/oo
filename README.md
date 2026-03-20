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

## Why oo?

AI coding agents waste context tokens on verbose command output. A single `cargo build`
can produce thousands of lines that the agent must process but rarely needs in full.

**oo solves this transparently:**
- Commands run normally — oo wraps them, not replaces them
- Output is classified and compressed using patterns
- Agents get the signal (pass/fail, errors, summaries) without the noise
- No agent modification required — just prefix commands with `oo`

Unlike manual truncation (`head`/`tail`), oo understands command semantics. Unlike
agent-native output limits, oo preserves the information the agent actually needs.

---

## The Problem

AI agents see everything you print. A `cargo test` run producing 8 KB of output
costs the agent hundreds of tokens just to learn "tests passed." Multiply that
across a session and the context window fills with noise, not signal. `oo` runs
commands for you and collapses their output to what the agent actually needs.

---

## Output Tiers

**Without oo:** Your agent receives the full output.
```
$ cargo test
   Compiling myapp v0.1.0 (/path/to/myapp)
    Finished test [unoptimized + debuginfo] target(s) in 0.52s
     Running unittests src/lib.rs (target/debug/deps/myapp)

running 47 tests
test auth::tests::login_success ... ok
test auth::tests::login_invalid_password ... ok
test db::tests::connection_pool ... ok
... 44 more tests ...
test result: ok. 47 passed; 0 failed; finished in 2.1s
```

**With oo:** Your agent gets the signal.
```
$ oo cargo test
✓ cargo test (47 passed, 2.1s)
```

Large output with a known success pattern collapses to a single summary line.

**When things fail:** Actionable errors, no noise.
```
$ oo pytest tests/
✗ pytest

FAILED tests/test_api.py::test_login - AssertionError: expected 200, got 401
FAILED tests/test_api.py::test_create_user - ValueError: email already exists
=== 2 failed, 45 passed in 1.8s ===
```
Failure output is filtered to the actionable tail.

**Large unrecognised output:** Indexed for retrieval.
```
$ oo gh issue list
● gh (indexed 47.2 KiB → use `oo recall` to query)
```
Query indexed output with `oo recall "<terms>"`. Small outputs pass through unchanged.
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

That's it. The agent runs `oo cargo test`, gets `✓ cargo test (47 passed, 2.1s)`,
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

## FAQ

**What if oo doesn't recognize my command?**
Unknown commands pass through unchanged (under 4KB) or get indexed for later retrieval
via `oo recall`. Use `oo learn <command>` to teach oo a compression pattern.

**Can I disable compression for a command?**
Unknown commands already pass through by default. For commands with built-in patterns,
you can override with a custom TOML pattern. See [Custom Patterns](docs/patterns.md).

**Does oo work in CI?**
It can, but oo is designed for interactive AI agent sessions where context tokens matter.
In CI, full output is usually fine.

**Which LLM providers does `oo learn` support?**
Anthropic only. Set `ANTHROPIC_API_KEY` in your environment.
See [Learning Patterns](docs/learn.md).

---

## Troubleshooting

**`oo: command not found`**
Ensure `~/.cargo/bin` is in your PATH, or use the full path to the binary.

**Pattern not matching my command**
Run `oo patterns` to see all loaded patterns and their command regexes.
Custom patterns in `~/.config/oo/patterns/` override built-ins.

**`oo learn` fails or produces bad patterns**
Ensure `ANTHROPIC_API_KEY` is set. Run the command normally first so oo has
real output to analyze. See [Learning Patterns](docs/learn.md).

---

## Documentation

**[Documentation Index](docs/README.md)** — Extensive project documentation:
- [Testing Guide](docs/testing.md) — How to run, write, and understand tests
- [Architecture](docs/architecture.md) — System design and module responsibilities
- [Security Model](docs/security-model.md) — Trust assumptions and data handling
- [Custom Patterns](docs/patterns.md) — Creating patterns for command output compression
- [Learning Patterns](docs/learn.md) — Using `oo learn` to generate patterns automatically

**For contributors**: See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and workflow.
**For agents**: See [AGENTS.md](AGENTS.md) for project-specific agent conventions.

---

## License

Apache-2.0 — see [LICENSE](LICENSE).

`oo help` fetches content from [cheat.sh](https://cheat.sh), which includes
[tldr-pages](https://github.com/tldr-pages/tldr) content (CC BY 4.0). See
[NOTICE](NOTICE).
