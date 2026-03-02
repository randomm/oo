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
provider    = "anthropic"                    # anthropic | openai
model       = "claude-haiku-4-5-20251001"    # default
api_key_env = "ANTHROPIC_API_KEY"            # env var holding the key
```

All three keys are optional — the defaults above apply if the section is absent.

## Supported providers

| `provider` | API endpoint |
|------------|--------------|
| `anthropic` | `https://api.anthropic.com/v1/messages` |
| `openai` | `https://api.openai.com/v1/chat/completions` |

> **Important:** `api_key_env` always defaults to `ANTHROPIC_API_KEY` regardless of provider.
> If you use OpenAI, you **must** set `api_key_env = "OPENAI_API_KEY"` explicitly:
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
