# Configuration Reference

## Directories

oo uses standard per-OS base directories (via the `dirs` crate): `dirs::config_dir()` for configuration and `dirs::data_dir()` for indexed data. These match XDG paths on Linux and the platform conventions on macOS and Windows.

| Purpose | Linux | macOS | Windows |
|---------|-------|-------|---------|
| Configuration | `~/.config/oo/` | `~/Library/Application Support/oo/` | `%APPDATA%/oo/` (e.g. `C:\Users\<user>\AppData\Roaming\oo\`) |
| Data (SQLite index) | `~/.local/share/.oo/oo.db` | `~/Library/Application Support/.oo/oo.db` | `%LOCALAPPDATA%\.oo\oo.db` (e.g. `C:\Users\<user>\AppData\Local\.oo\oo.db`) |
| Patterns (user-defined) | `~/.config/oo/patterns/` | `~/Library/Application Support/oo/patterns/` | `%APPDATA%/oo/patterns/` |

Override the config directory with the `OO_CONFIG_DIR` environment variable:

```bash
export OO_CONFIG_DIR=/custom/path
```

Override the data directory with the `OO_DATA_DIR` environment variable (used primarily for test isolation):

```bash
export OO_DATA_DIR=/custom/data/path
```

The SQLite database is always created at `<data dir>/oo.db`.

## Config File

Location: `~/.config/oo/config.toml`

The config file is optional. If it doesn't exist, oo uses defaults.

### `[learn]` section

Configuration for the `oo learn` LLM integration. This is the single source of truth for the model name — the default model is `claude-haiku-4-5` (only Anthropic is supported). An annotated example lives in [config.example.toml](../config.example.toml).

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `provider` | string | `"anthropic"` | LLM provider (only `"anthropic"` is supported) |
| `model` | string | `"claude-haiku-4-5"` | Model identifier for the provider |
| `api_key_env` | string | `"ANTHROPIC_API_KEY"` | Environment variable containing the API key |

### Default behavior

If the `[learn]` section is absent or `config.toml` doesn't exist, oo defaults to `provider = "anthropic"`, `model = "claude-haiku-4-5"`, and `api_key_env = "ANTHROPIC_API_KEY"`.

## Environment Variables

### `OO_CONFIG_DIR`

Overrides the configuration directory.

- **Default**: `~/.config/oo/`
- **Purpose**: Specify a custom location for `config.toml` and `patterns/`

```bash
export OO_CONFIG_DIR="/some/custom/dir"
```

### `OO_DATA_DIR`

Overrides the data directory that holds the SQLite index.

- **Default**: platform base data dir joined with `.oo/` (see the [Directories](#directories) table)
- **Purpose**: Relocate `oo.db`; primarily used to isolate tests from the real store

```bash
export OO_DATA_DIR="/some/custom/data/dir"
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

The configuration directory (see the [Directories](#directories) table) contains:

- `config.toml` - Optional user configuration
- `patterns/` - User-defined pattern files (`.toml`) — see [Custom Patterns](patterns.md)
- `learn-status.log` - Transient status from background learn processes (auto-cleaned)

User patterns in the config directory's `patterns/` directory override the built-in patterns (see `oo patterns` for the full list).