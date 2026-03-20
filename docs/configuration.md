# Configuration Reference

## Directories

oo uses standard XDG Base Directory locations for its files.

| Purpose | Location |
|---------|----------|
| Configuration | `~/.config/oo/` |
| Data (SQLite index) | `~/.local/share/.oo/oo.db` |
| Patterns (user-defined) | `~/.config/oo/patterns/` |

Override the config directory with the `OO_CONFIG_DIR` environment variable:

```bash
export OO_CONFIG_DIR=/custom/path
```

> **Note:** Data directory cannot be customized with an environment variable and uses the system default.

## Config File

Location: `~/.config/oo/config.toml`

The config file is optional. If it doesn't exist, oo uses defaults.

### `[learn]` section

Configuration for the `oo learn` LLM integration.

```toml
[learn]
provider    = "anthropic"
model       = "claude-haiku-4-5"
api_key_env = "ANTHROPIC_API_KEY"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `provider` | string | `"anthropic"` | LLM provider (only `"anthropic"` is supported) |
| `model` | string | `"claude-haiku-4-5"` | Model identifier for the provider |
| `api_key_env` | string | `"ANTHROPIC_API_KEY"` | Environment variable containing the API key |

### Default behavior

If the `[learn]` section is absent or `config.toml` doesn't exist, oo defaults to:

```toml
[learn]
provider    = "anthropic"
model       = "claude-haiku-4-5"
api_key_env = "ANTHROPIC_API_KEY"
```

## Environment Variables

### `OO_CONFIG_DIR`

Overrides the configuration directory.

- **Default**: `~/.config/oo/`
- **Purpose**: Specify a custom location for `config.toml` and `patterns/`

```bash
export OO_CONFIG_DIR="/some/custom/dir"
```

### `ANTHROPIC_API_KEY`

Required for `oo learn`.

- **Required**: Only when using `oo learn`
- **Default**: None

Set this to enable LLM-assisted pattern learning:

```bash
export ANTHROPIC_API_KEY="your-api-key"
```

### `ANTHROPIC_API_URL`

Optional custom endpoint for Anthropic API.

- **Default**: `https://api.anthropic.com/v1/messages`
- **Purpose**: Use a custom Anthropic-compatible endpoint

```bash
export ANTHROPIC_API_URL="https://your-proxy.example.com/v1/messages"
```

> **Security warning:** HTTP URLs only allowed for `localhost` or `127.0.0.1`. All other hosts must use HTTPS.

## Runtime Thresholds

These constants are built into the binary and cannot be configured.

| Constant | Value | Description |
|----------|-------|-------------|
| `SMALL_THRESHOLD` | 4096 bytes | Output below this size passes through unchanged |
| `TRUNCATION_THRESHOLD` | 80 lines | Failure output truncation starts after this many lines |
| `MAX_LINES` | 120 lines | Hard cap on lines shown after smart truncation |
| Entry TTL | 86400 seconds (24 hours) | Stale indexed entries are auto-cleaned |

## Feature Flags

| Flag | Description |
|------|-------------|
| `vipune-store` | Enables Vipune backend for indexed output with semantic search. When enabled, outputs are stored using Vipune instead of SQLite. Build with `cargo build --features vipune-store`. |

To build with Vipune support:

```bash
cargo build --release --features vipune-store
```

The `vipune-store` feature changes only the storage backend for `oo recall`. All other oo behavior remains identical.

## Files in Configuration Directory

`~/.config/oo/` contains:

- `config.toml` - Optional user configuration
- `patterns/` - User-defined pattern files (`.toml`)
- `learn-status.log` - Transient status from background learn processes (auto-cleaned)

User patterns in `~/.config/oo/patterns/` override the 10 built-in patterns.