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
provider    = "anthropic"                    # default
model       = "claude-haiku-4-5"             # optional
api_key_env = "ANTHROPIC_API_KEY"            # env var holding the key
```

All keys are optional — Anthropic is the default provider and will use
the `ANTHROPIC_API_KEY` environment variable if the `[learn]` section is absent.

### Providers

| Provider | Env Var | Default Model |
|----------|---------|---------------|
| Anthropic | `ANTHROPIC_API_KEY` | `claude-haiku-4-5` |

The provider is determined from the configured provider or auto-detected from
the available API key.

> **Note:** When configuring manually, set `api_key_env` to match the env var for
> your chosen provider. The default model is `claude-haiku-4-5` (see
> [Configuration — model reference](configuration.md#learn-section)); the example
> below just shows the shape of the section:
>
> ```toml
> [learn]
> provider    = "anthropic"
> model       = "<model identifier>"
> api_key_env = "ANTHROPIC_API_KEY"
> ```

## What happens if no LLM is configured

If the env var named by `api_key_env` is unset, the background learn step cannot
call the LLM. The command itself still runs and its output is not affected; the
learn error is reported as a status line on a later `oo` invocation, but only if the
config directory already exists; otherwise the failure is silent (see
[CLI reference](cli-reference.md#oo-learn-cmd-args)).

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

Patterns are most useful for Status commands (tests, linters, builds). Content and Data commands are shown verbatim only when the output is at most 4 KiB (4096 bytes; git show, git diff, cat, git log, ls, gh and the like). Above that, a matching success pattern shows its summary and nothing is indexed. Without a matching success pattern, Data output is indexed and shows an indicator line, and Content output shows a bounded head+tail slice with the full output indexed. Unknown commands follow the Content rule. See [Patterns: Command Categories](patterns.md#command-categories)
for category definitions.

## Best-effort semantics

Learning runs in the background after your command completes. If the LLM
returns invalid TOML or a bad regex, the pattern is silently discarded.
No partial files are written.
