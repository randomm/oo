# LLM-Assisted Pattern Learning

`oo learn <cmd>` runs a command, observes its output and exit code, then
generates and saves a pattern in the background using an LLM.

```
oo learn cargo test --release
```

The command executes normally. After it finishes, a background process sends
the command, output, and exit code to the configured LLM and writes a TOML
pattern to the user patterns directory (`~/.config/oo/patterns/` on Linux, `~/Library/Application Support/oo/patterns/` on macOS; see [configuration.md](configuration.md#directories)).

## Config

`config.toml` in the configuration directory (`~/.config/oo/config.toml` on Linux, `~/Library/Application Support/oo/config.toml` on macOS):

```toml
[learn]
provider    = "anthropic"                    # the only accepted value
model       = "claude-haiku-4-5"             # model identifier
api_key_env = "ANTHROPIC_API_KEY"            # env var holding the key
```

When a `[learn]` table is present, all three keys are required: a table that sets only some of them
fails to load with `missing field` (for example ``missing field `provider` ``). With no `config.toml`, or
no `[learn]` table, the defaults apply whatever environment variables are set: provider `anthropic`,
model `claude-haiku-4-5`, and `api_key_env` `ANTHROPIC_API_KEY`.

### Providers

| Provider | Env Var | Default Model |
|----------|---------|---------------|
| Anthropic | `ANTHROPIC_API_KEY` | `claude-haiku-4-5` |

The `provider` value must be `anthropic`. Any other value fails with `unknown provider`, but only after oo reads the `api_key_env` variable (an unset variable is reported as missing first) and `ANTHROPIC_API_URL` passes its URL check.

> **Note:** `api_key_env` must name the environment variable that holds your provider's key. The
> default model is `claude-haiku-4-5` (see [Configuration — `[learn]` section](configuration.md#learn-section)).

## What happens if no LLM is configured

If the env var named by `api_key_env` is unset, the background learn step cannot
call the LLM. The command itself still runs and its output is not affected; the
learn error is written to `learn-status.log` and reported as a status line on a later
`oo` invocation. The config directory is created if it is missing (see
[CLI reference](cli-reference.md#oo-learn-cmd-args)). If `learn-status.log` cannot be written
either, the failure is not reported anywhere and nothing is printed.

## Where patterns are saved

`<user patterns dir>/<label>.toml` — `~/.config/oo/patterns/` on Linux, `~/Library/Application Support/oo/patterns/` on macOS (see [configuration.md](configuration.md#directories)); `<label>` is derived from the command name.

Existing files are overwritten, so running `oo learn` again refines the pattern.
See [Custom Patterns](patterns.md) for the TOML format the generated files use, and
[examples/](../examples/) for runnable pattern examples.
The filename is derived from the first word of the command (the binary name), plus the
second word if it is a subcommand rather than a flag — for example, `cargo test` produces
`cargo-test.toml`, while `cargo --version` produces `cargo.toml`. This means each
binary/subcommand pair gets its own file and patterns don't collide across subcommands.

## Overwrite Behavior

Running `oo learn <command>` again for the same command overwrites the existing
pattern file. No backup is made and no warning is shown.

The filename is derived from the command name plus the second word when that word
is not a flag (e.g. `cargo-test.toml` for `cargo test --release`). This means re-running
`oo learn cargo test` after updating oo or switching LLM providers will silently
replace the previous pattern.

To preserve a pattern, rename or move the TOML file in the user patterns directory
before re-running learn.

## Command Categories & Pattern Value

Patterns are most useful for Status commands (tests, linters, builds). Failed commands are always filtered, at any size, and their full output is indexed when the displayed slice differs from it. Otherwise, successful output up to 4 KiB (4096 bytes) passes through unchanged for every category. Larger successful output shows the summary of a matching success pattern whose extraction yields something; otherwise Status is quiet, Data (git log, ls, gh) is indexed with an indicator line, and Content (git show, git diff, cat) and Unknown commands show a bounded head+tail slice with the full output indexed. See [Patterns: Command Categories](patterns.md#command-categories)
for category definitions.

## Best-effort semantics

Learning runs in the background after your command completes. If the LLM
returns invalid TOML or a bad regex, the pattern is silently discarded.
No partial files are written.
