# LLM-Assisted Pattern Learning

`oo learn <cmd>` runs a command, observes its output and exit code, then
generates and saves a pattern in the background using an LLM.

```
oo learn cargo test --release
```

The command executes normally. After it finishes, a background process sends
the command, output, and exit code to the configured LLM and writes a TOML
pattern to `~/.config/oo/patterns/`.

## Config

`~/.config/oo/config.toml`:

```toml
[learn]
provider    = "anthropic"                    # anthropic | openai | cerebras
model       = "claude-haiku-4-5-20251001"    # default
api_key_env = "ANTHROPIC_API_KEY"            # env var holding the key
```

All three keys are optional — the provider is auto-detected from available API keys
if the section is absent (see below).

### Providers

| Provider | Env Var | Default Model |
|----------|---------|---------------|
| Anthropic | `ANTHROPIC_API_KEY` | `claude-haiku-4-5-20251001` |
| OpenAI | `OPENAI_API_KEY` | `gpt-4o-mini` |
| Cerebras | `CEREBRAS_API_KEY` | `zai-glm-4.7` |

> **Cerebras model**: The default model (`zai-glm-4.7`) is current at time of writing.
> Check the [Cerebras model catalog](https://cloud.cerebras.ai/models) for newer models
> and override via `config.toml` if needed.

The provider is auto-detected from available API keys (checked in order above).
If you have `CEREBRAS_API_KEY` set and nothing else, `oo learn` just works.

> **Note:** When configuring manually, set `api_key_env` to match the env var for
> your chosen provider:
>
> ```toml
> [learn]
> provider    = "openai"
> model       = "gpt-4o-mini"
> api_key_env = "OPENAI_API_KEY"
> ```

## What happens if no LLM is configured

If the env var named by `api_key_env` is unset, `oo learn` will not be
available. The command itself still runs and its output is not affected.

## Where patterns are saved

`~/.config/oo/patterns/<binary>.toml` — where `<binary>` is the first word of
the command (e.g. `cargo.toml` for `cargo test`).

Existing files are overwritten, so running `oo learn` again refines the pattern.

## Best-effort semantics

Learning runs in the background after your command completes. If the LLM
returns invalid TOML or a bad regex, the pattern is silently discarded.
No partial files are written.
