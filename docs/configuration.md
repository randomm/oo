# Configuration Reference

## Directories

oo uses the per-OS base directories from the `dirs` crate: `dirs::config_dir()` for configuration and `dirs::data_dir()` for indexed data. The table lists the default values on macOS and Linux. On Linux the values follow the XDG base directory rules as implemented by the `dirs` crate:

| Purpose | Linux | macOS |
|---------|-------|-------|
| Configuration | `$XDG_CONFIG_HOME/oo/` (default `~/.config/oo/`) | `~/Library/Application Support/oo/` |
| Data (SQLite index) | `$XDG_DATA_HOME/.oo/oo.db` (default `~/.local/share/.oo/oo.db`) | `~/Library/Application Support/.oo/oo.db` |
| Patterns (user-defined) | `<config dir>/patterns/` (default `~/.config/oo/patterns/`) | `~/Library/Application Support/oo/patterns/` |

On Linux, `XDG_CONFIG_HOME` and `XDG_DATA_HOME` are honoured only when they are absolute paths; otherwise the defaults above apply. On macOS the XDG variables are ignored. Windows is untested.

If the `dirs` crate cannot determine a base directory, the config directory falls back to `/tmp/oo` and the data directory falls back to `$HOME/.oo` (then `/tmp/.oo`).

Override the config directory with the `OO_CONFIG_DIR` environment variable. The value is used as-is (no `oo/` is appended), so `config.toml` and `patterns/` live directly inside it. A relative value is resolved against the current working directory, and an empty value (`OO_CONFIG_DIR=`) means the current working directory itself:

```bash
export OO_CONFIG_DIR=/custom/path
```

Override the data directory with the `OO_DATA_DIR` environment variable (used primarily for test isolation):

```bash
export OO_DATA_DIR=/custom/data/path
```

The SQLite database is always named `oo.db`. Its directory (`<data dir>`) is `OO_DATA_DIR` when set, used as-is (`OO_DATA_DIR=/x` gives `/x/oo.db`). A relative value is resolved against the current working directory (`OO_DATA_DIR=rel` gives `./rel/oo.db`), and an empty value gives `./oo.db`. Otherwise it is the platform data directory plus `.oo/` (the Data row above).

## Config File

Location: `config.toml` in the configuration directory (`~/.config/oo/config.toml` on Linux, `~/Library/Application Support/oo/config.toml` on macOS; see [Directories](#directories)).

The config file is optional. If it doesn't exist, oo uses defaults.

### `[learn]` section

Configuration for the `oo learn` LLM integration. This is the single source of truth for the model name — the default model is `claude-haiku-4-5` (only Anthropic is supported). An annotated example lives in [config.example.toml](../config.example.toml).

| Field | Type | Default (only when the whole `[learn]` table is absent) | Description |
|-------|------|---------|-------------|
| `provider` | string | `"anthropic"` | LLM provider (only `"anthropic"` is supported); required when `[learn]` is present |
| `model` | string | `"claude-haiku-4-5"` | Model identifier for the provider; required when `[learn]` is present |
| `api_key_env` | string | `"ANTHROPIC_API_KEY"` | Environment variable containing the API key; required when `[learn]` is present |

### Default behavior

If the `[learn]` section is absent or `config.toml` doesn't exist, oo defaults to `provider = "anthropic"`, `model = "claude-haiku-4-5"`, and `api_key_env = "ANTHROPIC_API_KEY"`. These defaults are fixed in code and do not depend on which environment variables are set.

A `[learn]` table that is present must set all three keys. The per-field defaults in the table above do not fill in a missing key: a table with only some of them (for example only `model`) fails to load with `missing field` (for example ``missing field `provider` ``). `oo learn` then prints an `oo: config error` line, and its background step fails on the same error, so no pattern is written.

### `[store]` section

Retention for indexed command output (the entries `oo` stores for `oo recall`). An annotated example lives in [config.example.toml](../config.example.toml).

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `retention` | integer (seconds) | `604800` (7 days) | Indexed entries older than this are purged. `0` disables purging. Must be `>= 0`. |

- **When purging happens:** lazily and per project. A purge runs when a project next indexes output, and removes only that project's entries older than `retention`. Other projects, and a project that never indexes again, keep their entries until then.
- **Vipune store backend:** with `--features vipune-store`, each purge examines only the first 10,000 entries that Vipune lists for the project, so stale entries beyond that page are not purged by that run.
- **Invalid config:** an absent `[store]` section or `retention` key uses the default. A negative, non-integer, or out-of-range (above 2^63-1) value, an unknown key in `[store]` (for example a typo such as `retension`), or a `config.toml` that cannot be parsed prints a warning on stderr and uses the default. The warning appears when indexing runs (not on `oo recall` or `oo forget`). Indexing is never blocked by config. The `[learn]` table is parsed separately: a partial or otherwise invalid `[learn]` table (observed: only `model`) leaves `[store] retention` in effect, and an invalid `[store]` value (observed: `retention = "x"`) leaves `oo learn` working. Only a TOML syntax error or an unreadable `config.toml` affects both: indexing then warns and uses the default retention, and `oo learn` prints a config error. Observed for a `config.toml` with mode 000 and for a directory named `config.toml`. One case differs: if the config directory itself cannot be searched (observed with mode 000 on the directory), `oo learn` treats `config.toml` as absent, prints no error and silently uses its defaults (exit 0), while indexing still warns. The cause in source is `load_learn_config` (`learn.rs`) using `path.exists()`, which returns false on any metadata error, whereas `retention.rs` reads the file and reports the error.
- `oo forget` clears a project's indexed entries immediately, regardless of retention. With `--features vipune-store`, its completeness is subject to the stop rule in [Security Model](security-model.md#command-output).

## Environment Variables

### `OO_CONFIG_DIR`

Overrides the configuration directory.

- **Default**: the platform configuration directory plus `oo/` (see the [Directories](#directories) table)
- **Purpose**: Specify a custom location for `config.toml` and `patterns/`. The same `config.toml` supplies `[learn]` and `[store] retention`. With an empty or relative value it is read from the working directory, so a repository's own `config.toml` can then set the retention (see [Security Model](security-model.md#pattern-files))

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

The environment variable named by `api_key_env` in `[learn]` must be set for `oo learn`.

- **Required**: Only when using `oo learn`
- **Default**: no value is set; the variable name defaults to `ANTHROPIC_API_KEY`

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

> **Security warning:** the URL rules (HTTPS except for `localhost`, `127.0.0.1` and `[::1]`, no userinfo) and the rule that redirects are never followed are in [When oo makes network calls](security-model.md#when-oo-makes-network-calls).

## Runtime Thresholds

These constants are built into the binary and cannot be configured.

| Constant | Value | Description |
|----------|-------|-------------|
| `SMALL_THRESHOLD` | 4096 bytes | Successful (exit 0) output of at most this many bytes (4 KiB) passes through unchanged; failed output is always filtered |
| `MAX_LINES` | 120 lines | Cap of the `smart_truncate` view (output of at most this many lines is shown in full; longer output keeps its first 72 and last 48 lines around a marker). Used for failed output with no matching pattern or whose pattern has no `failure` section, and as the display when indexing fails for successful Data output over 4 KiB |

## Feature Flags

| Flag | Description |
|------|-------------|
| `vipune-store` | Enables Vipune backend for indexed output with semantic search. When enabled, outputs are stored in Vipune's database (`~/.vipune/memories.db` by default) instead of SQLite. Build with `cargo build --features vipune-store`. |

To build with Vipune support:

```bash
cargo build --release --features vipune-store
```

The `vipune-store` feature changes the storage backend for indexed output (`oo recall`, `oo forget` and indexing). It also adds network access that the default build does not have: opening the store downloads the embedding model from Hugging Face when it is not cached (only once the database's parent directory exists, because vipune canonicalizes that path before loading the model), and the `ort` dependency downloads ONNX Runtime at build time. These behaviours come from source inspection of vipune 0.1.2 and were not observed at runtime. See [Security Model](security-model.md#when-oo-makes-network-calls).

## Files in Configuration Directory

The configuration directory (see the [Directories](#directories) table) contains:

- `config.toml` - Optional user configuration
- `patterns/` - User-defined pattern files (`.toml`) — see [Custom Patterns](patterns.md)
- `learn-status.log` - Transient status from background learn processes (auto-cleaned)

Project patterns (`<git-root>/.oo/patterns/`) and user patterns in the config directory's `patterns/` directory are checked before the built-in patterns; the order is project, then user, then built-in, and the first match wins. The foreground display in `oo learn` uses built-ins only (#151). See `oo patterns` for the full list.