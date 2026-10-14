# oo

[![CI status](https://github.com/randomm/oo/actions/workflows/ci.yml/badge.svg)](https://github.com/randomm/oo/actions/workflows/ci.yml) [![crates.io version](https://img.shields.io/crates/v/double-o.svg)](https://crates.io/crates/double-o) [![docs.rs documentation](https://docs.rs/double-o/badge.svg)](https://docs.rs/double-o) [![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0) [![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-brightgreen.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

**oo shrinks verbose shell output for AI coding agents.** It runs the command, keeps the result and the errors, and indexes the rest for `oo recall`. It supports [pi](docs/agents.md#pi) and [Claude Code](docs/agents.md#claude-code); other agents use a prompt instruction (see [Agent Integration](docs/agents.md)).

```bash
cargo install double-o        # the crate is double-o; it installs the `oo` binary
oo init --agent pi            # or: oo init --agent claude-code
```

Legend: ✓ passed · ✗ failed · ● indexed or truncated.

Before and after:

```
$ cargo test                  # without oo: every line of the run reaches the agent
$ oo cargo test               # with oo, output over 4 KiB
✓ cargo (47 passed, 2.1s) [saved 46.10 KiB]
```

---

## Why oo?

AI agents see everything a command prints, and they pay context tokens for all of it. A
`cargo build` can produce thousands of lines the agent must read but rarely needs. A
`cargo test` run producing 47 KB costs roughly 12,000 tokens (at about 4 bytes per token,
an illustrative estimate) just to learn that the tests passed, and across a session the
context window fills with noise.

oo runs the command for the agent and keeps what it needs:
- Commands run normally — oo wraps them, not replaces them
- Output is classified and compressed using patterns
- Agents get the signal (pass/fail, errors, summaries) without the noise
- Pi and Claude Code need no prompt change: `oo init --agent pi` or `oo init --agent claude-code` installs a hook or extension that rewrites commands. Other agents need one line in their prompt (see [Agent Integration](#agent-integration))

Unlike manual truncation (`head`/`tail`), oo understands command semantics. Unlike
agent-native output limits, oo preserves the information the agent actually needs.

---

## What oo prints

Each command falls into one of four categories, which decides what is shown when no
pattern matches:

- **Status**: test runners, builds, and linters.
- **Data**: listings and logs such as `git log` and `ls`.
- **Content**: file-like output such as `git show`, `git diff`, and `cat`.
- **Unknown**: everything else, such as `jq` or `sh -c`.

For output over 4 KiB with no matching pattern summary: Status collapses to a quiet `✓` line; Data shows only an indicator line; Content and Unknown show a head+tail slice. Successful output of 4 KiB or less passes through unchanged; failed commands are always filtered. See [command categories](docs/patterns.md#command-categories) for the full rules.

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
✓ cargo (47 passed, 2.1s) [saved 46.10 KiB]
```

The `[saved …]` figure is appended only when oo saved more than 4 KiB, and the value shown is illustrative. The 47-test run above stands for a large run: a successful run of 4 KiB or less passes through unchanged and prints as-is.

Large output with a known success pattern collapses to a single summary line.

**When things fail:** Actionable errors, no noise.
```
$ oo pytest tests/
✗ pytest

FAILED tests/test_api.py::test_login - AssertionError: expected 200, got 401
FAILED tests/test_api.py::test_create_user - ValueError: email already exists
=== 2 failed, 45 passed in 1.8s ===
```
Failure output is filtered to the actionable tail. When the displayed output does not match the full output of the failed run line for line (ignoring trailing empty lines), that full output is indexed too, so `oo recall` can find what the display left out.

**Large unrecognised output:** Indexed for retrieval.
```
$ oo gh issue list
● gh (indexed <size> → use `oo recall` to query)
```
Query indexed output with `oo recall "<terms>"`. Indexed output is kept 7 days by default (`[store] retention`); entries older than that are purged the next time the same project indexes output (see [configuration](docs/configuration.md#store-section)). Small successful outputs (≤ 4 KiB) pass through
unchanged; failed commands are always filtered (see above). Larger unpatterned Data, Content, and Unknown output is indexed in full; successful Status output (test runners, builds, linters) over 4 KiB with no pattern summary gets a quiet `✓` line and is not indexed. Data commands like
`git log` and `ls` show only an indicator line; content commands like `git show` and `git diff`
and unknown commands like `jq` or `sh -c` show a byte-bounded head+tail slice with a truncation
marker (``... [N bytes truncated → use `oo recall` to query] ...``), so large `cat`, `jq`, or
`sh -c` output cannot blow an agent's context window. See the [patterns guide](docs/patterns.md#command-categories)
for details.

---

## Installation

**Supported platforms:** Apple Silicon macOS, and Linux x86_64 and ARM64 with glibc 2.39 or newer (the installer enforces the glibc floor). Intel macOS, Windows and older-glibc Linux are not covered by the pre-built binaries or the installer: use `cargo install double-o` (Rust 1.85+) or build from source.

### Quick install (Apple Silicon macOS and Linux)

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/randomm/oo/releases/latest/download/double-o-installer.sh | sh
```

> The installer detects your platform, downloads the `oo` binary and installs it to `$CARGO_HOME/bin` (default `~/.cargo/bin/`). If you don't use Rust, restart your shell or run `export PATH="$HOME/.cargo/bin:$PATH"` to make `oo` available.

### Pre-built binaries (supported platforms)

Download from [GitHub Releases](https://github.com/randomm/oo/releases/latest):

```bash
# macOS (Apple Silicon)
curl -LO https://github.com/randomm/oo/releases/latest/download/double-o-aarch64-apple-darwin.tar.xz
tar xf double-o-aarch64-apple-darwin.tar.xz
sudo mv double-o-aarch64-apple-darwin/oo /usr/local/bin/

# Linux (x86_64)
curl -LO https://github.com/randomm/oo/releases/latest/download/double-o-x86_64-unknown-linux-gnu.tar.xz
tar xf double-o-x86_64-unknown-linux-gnu.tar.xz
sudo mv double-o-x86_64-unknown-linux-gnu/oo /usr/local/bin/

# Linux (ARM64)
curl -LO https://github.com/randomm/oo/releases/latest/download/double-o-aarch64-unknown-linux-gnu.tar.xz
tar xf double-o-aarch64-unknown-linux-gnu.tar.xz
sudo mv double-o-aarch64-unknown-linux-gnu/oo /usr/local/bin/
```

### From crates.io

```bash
cargo install double-o
```

The crate is named `double-o`; it installs the `oo` binary. Requires Rust 1.85+. Use this on platforms without a pre-built binary (Intel macOS, Windows, glibc older than 2.39).

### From source

```bash
git clone https://github.com/randomm/oo.git
cd oo
cargo build --release
cp target/release/oo /usr/local/bin/
```

---

## Quick Start

```bash
# 1. Set up your agent (pi or Claude Code): commands that match a pattern are rewritten through oo
$ oo init --agent pi

# 2. Wrap any command — output is classified and compressed (figures are illustrative)
$ oo cargo test
✓ cargo (47 passed, 2.1s) [saved 46.10 KiB]

# 3. Output over 4 KiB that has no pattern is indexed — query it later
$ oo ls -la /usr/share/doc
● ls (indexed <size> → use `oo recall` to query)
$ oo recall "<term>"

# Teach oo a new pattern for an unrecognised command
# (sends the command (first 100 bytes), up to the first 4,000 bytes of its output, and its exit code to Anthropic; needs ANTHROPIC_API_KEY;
# avoid if the output may contain secrets; see docs/security-model.md)
$ oo learn rspec spec/
```

**Shell semantics:** `oo` runs programs directly — it does not invoke a shell. Use
`oo sh -c '<command>'` when you need shell syntax (pipes, `&&`, globbing):

```bash
oo sh -c 'git log --oneline | head -20'
```

See the [savings indicator spec](docs/cli-reference.md#savings-indicator) for how the
`[saved N]` figure on the indicator line works.

---

## Commands

| Command | Description |
|---|---|
| `oo <cmd> [args...]` | Run a shell command with context-efficient output |
| `oo recall [--full] <query>` | Search indexed output for this project (kept 7 days by default, see [configuration](docs/configuration.md#store-section)) |
| `oo forget` | Clear all indexed output for this project |
| `oo learn <cmd> [args...]` | Run command and teach `oo` a new output pattern via LLM (sends the command (first 100 bytes), up to the first 4,000 bytes of its output, and its exit code to Anthropic; requires `ANTHROPIC_API_KEY`; avoid if output may contain secrets, see [Security Model](docs/security-model.md)) |
| `oo help <cmd>` | Fetch a cheat sheet for `cmd` from cheat.sh (`oo help <reserved-subcommand>` prints local usage) |
| `oo init --agent <pi\|claude-code>` | Install the agent hook (optional `--global`); see [Agent Integration](docs/agents.md) |
| `oo patterns` | List loaded output patterns (built-in, project and user) |
| `oo rewrite <command...>` | Print an `oo`-prefixed form of a command for agent hooks (experimental) |
| `oo hook <agent>` | Agent hook processor (e.g. `oo hook claude` reads PreToolUse JSON on stdin) |
| `oo version` | Print version |

> **Note:** Any reserved subcommand accepts `--help` / `-h` as its first argument to print its usage and exit 0 (e.g. `oo init --help`). The subcommand itself does not run, but a pending background-learn status message is still printed and cleared first.
>
> `oo help` sources from [cheat.sh](https://cheat.sh) which covers common Unix tools. For modern CLIs not yet in cheat.sh (e.g., `gh`, `kamal`), use `oo learn <cmd>` to teach `oo` the command's output patterns.

---

## Agent Integration

Install the hook for your agent:

```bash
oo init --agent pi           # pi: <git-root>/.pi/extensions/oo.ts
oo init --agent claude-code  # Claude Code: <git-root>/.claude/settings.json
```

`--global` installs for every project instead. Only commands that match an `oo`
pattern are rewritten to their `oo`-prefixed form; everything else runs unchanged.
Setup, verification, disabling, uninstalling and what to commit are in
[Agent Integration](docs/agents.md).

---

## Built-in Patterns

`oo` ships with 24 patterns that match commands automatically:

| Command | Success | Failure strategy |
|---|---|---|
| `pytest` | `{passed} passed, {time}s` | tail 30 lines |
| `cargo test` | `{passed} passed, {time}s` | tail 40 lines |
| `go test` | `ok ({time}s)` | tail 30 lines |
| `jest` / `vitest` / `npx jest` | `{passed} passed, {time}s` | tail 30 lines |
| `ruff check` | quiet (no output on pass) | smart truncate |
| `eslint` | quiet | smart truncate |
| `cargo build` | quiet | head 20 lines |
| `go build` | quiet | head 20 lines |
| `tsc` | quiet | head 20 lines |
| `cargo clippy` | quiet | smart truncate |
| `npm test` | `{passed} passed, {time}s` | tail 30 lines |
| `yarn test` | `{passed} passed, {time}s` | tail 30 lines |
| `pnpm test` | `{passed} passed, {time}s` | tail 30 lines |
| `bun test` | `{passed} passed, {time}s` | tail 30 lines |
| `cargo tarpaulin` | `{cov}% coverage` | tail 20 lines |
| `cargo fmt` | quiet | grep for "Diff in" |
| `mypy` | `Success: no issues found` / error count | head 20 lines |
| `rubocop` | `{offenses} offenses` | head 30 lines |
| `ruff format` | `{count} files reformatted` | head 20 lines |
| `prettier` | quiet | head 20 lines |
| `npm run build` | quiet | head 20 lines |
| `yarn build` | `Done in {time}s` | head 20 lines |
| `pnpm build` | quiet | head 20 lines |
| `bun build` | quiet | head 20 lines |

Add your own patterns with `oo learn <cmd>` (generates a TOML pattern file via
LLM) or write one manually in the user patterns directory (`oo patterns` prints the path).

---

## FAQ

**What if oo doesn't recognize my command?**
Successful unknown commands pass through unchanged at up to 4 KiB; failed ones are always filtered. Larger Data, Content, and Unknown output
is indexed in full for later retrieval via `oo recall` (unless a matching success pattern shows its summary instead); entries older than the retention period (7 days by default) are purged the next time the same project indexes output (see [configuration](docs/configuration.md#store-section)). Data commands show only an indicator
line; other commands show a byte-bounded head+tail slice (marked with a truncation line).
Successful Status commands such as `make` with more than 4 KiB of output and no pattern summary print only a `✓ label` line, with ` [saved N]` appended when the output minus that line is over 4 KiB (binary units from KiB upward, chosen by magnitude; see the [savings indicator spec](docs/cli-reference.md#savings-indicator)), and are not indexed. Use `oo learn <command>` to teach oo a compression pattern.

**Can I disable compression for a command?**
Larger unpatterned unknown commands already show a bounded head+tail slice with the full output stored for
`oo recall`; entries older than the retention period (7 days by default) are purged the next time the same project indexes output (see [configuration](docs/configuration.md#store-section)). For commands with built-in patterns, you can override with
a custom TOML pattern. See [Custom Patterns](docs/patterns.md).

**Does oo work in CI?**
It can, but oo is designed for interactive AI agent sessions where context tokens matter.
In CI, full output is usually fine.

**Which LLM providers does `oo learn` support?**
Anthropic only. `oo learn` sends the command line (first 100 bytes), up to the first 4,000 bytes of its output, and its exit code to Anthropic, and requires `ANTHROPIC_API_KEY` in your environment. Avoid it for commands whose output may contain secrets. See [Learning Patterns](docs/learn.md) and [Security Model](docs/security-model.md).

---

## Troubleshooting

**`oo: command not found`**
Ensure `~/.cargo/bin` is in your PATH, or use the full path to the binary.

**Pattern not matching my command**
Run `oo patterns` to see all loaded patterns and their command regexes.
Project patterns (`.oo/patterns/` in the git root, or the current directory when there is no git root) and user patterns are checked before built-ins: project first, then user, then built-in; the first match wins. The foreground display in `oo learn` uses built-ins only (#151).

**`oo learn` fails or produces bad patterns**
Ensure `ANTHROPIC_API_KEY` is set. Run the command normally first so oo has
real output to analyze. See [Learning Patterns](docs/learn.md).

---

## Documentation

**[Documentation Index](docs/README.md)** — Extensive project documentation:
- [Agent Integration](docs/agents.md) — Installing, verifying and removing the pi and Claude Code hooks
- [`oo rewrite`](docs/oo-rewrite.md) — Which commands are rewritten, and how (experimental)
- [CLI Reference](docs/cli-reference.md) — All subcommands, output tiers, and the savings indicator spec
- [Custom Patterns](docs/patterns.md) — Creating patterns for command output compression
- [Learning Patterns](docs/learn.md) — Using `oo learn` to generate patterns automatically
- [Configuration](docs/configuration.md) — Environment variables, config files, and platform paths
- [Security Model](docs/security-model.md) — Trust assumptions and data handling
- [Testing Guide](docs/testing.md) — How to run, write, and understand tests
- [Architecture](docs/architecture.md) — System design and module responsibilities
- [Changelog](CHANGELOG.md) — Release history

**For contributors**: See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and workflow.
**For agents**: See [AGENTS.md](AGENTS.md) for project-specific agent conventions.

---

## License

Apache-2.0 — see [LICENSE](LICENSE).

`oo help` fetches content from [cheat.sh](https://cheat.sh), which includes
[tldr-pages](https://github.com/tldr-pages/tldr) content (CC BY 4.0). See
[NOTICE](NOTICE).
