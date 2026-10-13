# Security Model

This document describes oo's security properties, trust assumptions, and how it handles sensitive data.

## Core Principle: Local-First, Transparent

oo is designed to be safe and predictable:

1. **All execution is local** — Commands run on your machine exactly as you typed them
2. **No telemetry or data collection** — oo sends no usage data. In the default build, its only network requests are the ones listed in [When oo makes network calls](#when-oo-makes-network-calls): the command output and API key sent by `oo learn`, and the command name requested from cheat.sh by `oo help <command>`. A `--features vipune-store` build also downloads an embedding model from Hugging Face (see [When oo makes network calls](#when-oo-makes-network-calls)). Commands you run may make their own requests
3. **Transparent modification** — oo only compresses output; it doesn't modify command behavior or inject side effects
4. **Clear boundaries** — You control what gets indexed, stored, and transmitted

## Trust Assumptions

### Required Trust

You must trust:

1. **The oo binary** — It executes commands and can read/write local files
2. **The LLM provider (Anthropic)** — When using `oo learn`, your command output is sent to Anthropic (see below)
3. **Your shell environment** — Commands run with your existing environment variables, PATH, and shell settings

### What We Don't Trust (and Why)

- **Remote servers**: All command execution is local. The default build makes network requests only for `oo learn` and `oo help <command>`; a `vipune-store` build also downloads an embedding model (see [When oo makes network calls](#when-oo-makes-network-calls)).
- **Command inputs**: Commands are parsed but not modified. If a command is malicious (e.g., `rm -rf /`), oo executes it as-is — the security boundary is between you and the command, not oo.

## Data Handling

### Command Output

**Where it goes**:

- **Small successful outputs (≤ 4 KiB, 4096 bytes)**: Displayed directly, never stored (failed-command output is handled by the failure bullet below, at any size)
- **Pattern-matched outputs**: Extracted summary displayed, original output discarded
- **Large (Data) and Bounded (Content/Unknown) unpatterned outputs**: Stored locally for recall, in the SQLite index by default or in Vipune's database with `vipune-store` (see Storage Backends)
- **Failed-command outputs (exit code ≠ 0)**: when the displayed slice does not match the full merged output line for line (compared line for line; trailing empty lines and CRLF line endings are ignored), the full merged output is stored in the same index for recall, with the same project scoping as other indexed output. With the default SQLite store it also has the same file permissions (`oo.db` created with mode `0600`, and an existing one tightened on open; see Storage Backends) and the same retention (the `[store] retention` purge). With `--features vipune-store`, oo sets no mode on Vipune's database, so its permissions are whatever Vipune creates, and the retention purge examines only the newest 10,000 entries Vipune lists for the project (from source inspection; see Data retention). It can contain secrets the failing command printed. The recall line printed after the slice is host-authored, but it follows command output that can contain a byte-identical copy of it, so its text does not prove that oo printed it (the same residual risk as the Bounded framing line; see Pattern Files)

**Data retention**:
- Indexed outputs are purged automatically after a configurable retention period (default 7 days), lazily when the same project next indexes output — see [configuration.md](configuration.md#store-section). With `--features vipune-store` the purge inspects only the newest 10,000 entries of a project, so older entries may persist beyond the retention period
- No automatic syncing to remote services
- Project-scoped — `oo forget` clears indexed output for the current project. With the default SQLite store it deletes every entry with that project id. With `--features vipune-store` it lists up to 10,000 of the project's entries per listing, deletes each one, and lists again while passes make progress, until a listing is empty. It returns an error in these cases: the store cannot be opened (before any listing); a listing fails, which returns at once with no count and may leave deletions from earlier passes committed; a pass deletes nothing and at least one delete fails; a pass deletes nothing and sees only already-gone entries twice in a row; or 1,000 passes run without an empty listing. Every drain stop returns `partial delete: deleted N, failed M; entries remain`, where M is the final number of distinct entries whose last delete attempt failed (0 when the stop came from already-gone entries). Otherwise it reports success with the number deleted. Success means a listing found no entries; entries written concurrently after that listing are not removed. On the vipune store the retention purge inspects only the newest 10,000 entries Vipune lists for the project (the list order is assumed, not verified against vipune 0.1.2), so older entries can outlive the retention period.

### Storage Backends

**SQLite (default)**:
- Database file: `oo.db`, in `<data dir>`. Default `<data dir>` is the `dirs::data_dir()` location plus `.oo/` (per-OS defaults in [configuration.md](configuration.md#directories)). `OO_DATA_DIR` replaces it when valid (see [Directories](configuration.md#directories)), never the current directory
- Permissions (Unix):
  - A data directory oo creates is made with mode `0700` (missing parent directories keep the umask mode), and `oo.db` is created with mode `0600`. The umask can only remove bits from these modes, so they are never wider.
  - SQLite gives its `-wal`, `-shm` and `-journal` sidecars the main file's mode, so they are `0600` too when `oo.db` is a `0600` file. If `oo.db` is a symlink, SQLite opens the target and the sidecars take the target's mode, which oo does not change.
  - Before the database is opened, oo tightens an existing `oo.db` and its sidecars that are regular files you own: group and other bits are removed (if set), and a file whose owner lacks read or write keeps its owner bits instead of `0600`. A file oo cannot open for reading is left unchanged. oo never changes a data directory it did not create, and never changes a file owned by another user.
  - oo's own create and chmod never follow a symlink at `oo.db` or a sidecar, but SQLite follows a symlink when it opens the database: if `oo.db` is a symlink, its target keeps its own mode, and if the link is dangling, SQLite creates the target with the umask mode.
  - Creating or tightening `oo.db` and its sidecars is best effort and silent: if it fails, oo still opens the store and prints nothing. Failing to create the data directory is still an error that stops oo from opening the store, as before.
- Contents: Command output, metadata (command, timestamp, session identifier — informational only, not used for scoping)
- When oo creates `oo.db`, other local users (not root) cannot read it, because the file is `0600`. When oo tightens an existing `oo.db` you own, its group and other bits are removed, so other users cannot read it either. A file owned by another user, or one oo cannot open for reading, is not changed, so its mode is whatever it was

**Vipune (optional, `--features vipune-store` build)**:
- Stores outputs in Vipune's database instead of `oo.db`. The default database is `~/.vipune/memories.db`; `VIPUNE_DATABASE_PATH`, or `database_path` in `vipune/config.toml` in the platform config directory, changes it (from source inspection of vipune 0.1.2)
- Same access controls as your Vipune installation
- Only used if you explicitly enable the `vipune-store` feature
- Opening the store may download the embedding model from Hugging Face when it is not cached (see [When oo makes network calls](#when-oo-makes-network-calls))

### LLM Learning (`oo learn`)

**What gets sent**:
- Command string (e.g., "cargo test --release")
- Complete command output (stdout + stderr)
- Exit code

**What does NOT get sent**:
- Environment variables (except API key for authentication)
- File system contents (except what the command prints)
- Shell history
- Previous session data
- Other projects' indexed output (storage is scoped per project)

**Provider**: Anthropic only (currently supported)
- **API key variable**: the environment variable named by `api_key_env` in the `[learn]` config (default `ANTHROPIC_API_KEY`); it must be set to a non-empty value for `oo learn`; an empty or whitespace-only value is treated as unset and no request is sent
- **Model**: `claude-haiku-4-5` (default, configurable)
- **Endpoint**: `https://api.anthropic.com`
- **URL validation**: the configured `ANTHROPIC_API_URL` is checked before any request is sent, and redirects are refused; the rules are in [When oo makes network calls](#when-oo-makes-network-calls).

**When it runs**:
- Only when you explicitly run `oo learn <command>`
- In the background after the command completes
- Results are written locally to the user patterns directory (`~/.config/oo/patterns/` on Linux, `~/Library/Application Support/oo/patterns/` on macOS)

**Anthropic's data policy**: See [Anthropic's data usage policy](https://www.anthropic.com/legal/privacy). Anthropic may store API inputs for service improvement.

### API Key Handling

**Storage**:
- API keys are never stored by oo
- They are read from environment variables at runtime
- No caching or persistence to disk

**Transmission**:
- The key is sent only to the endpoint configured in `ANTHROPIC_API_URL` (default `https://api.anthropic.com/v1/messages`), never to a redirect target (see [When oo makes network calls](#when-oo-makes-network-calls))
- Keys are never logged or printed in output

**Best practice**: Use shell environment or a secret manager:
```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

**Do NOT**:
- Commit API keys to git
- Hardcode keys in config files
- Share keys in issue reports

## Command Injection Prevention

### Input Parsing

oo uses `clap` for CLI argument parsing, which provides protection against:

- Shell injection via argument splitting
- Flag injection (malicious flags are passed as-is to the target command)

**However**: oo does NOT sanitize commands. If an agent generates `rm -rf /`, oo executes it. The security boundary is between you and the agent, not oo.

### Pattern Files

Patterns are TOML files with a `command_match` regex and optional `success` and `failure` strategies. Only regular files (or symlinks to regular files) named `*.toml` are read; directories, FIFOs, sockets, devices and dangling symlinks are skipped without being opened. Built-in patterns and user patterns (in the user patterns directory, see Read Access) are trusted by design:

- You installed, wrote, or accepted them. This includes patterns that `oo learn` writes into that directory from LLM output, which you should review.
- `OO_CONFIG_DIR` is used only when absolute (see [Directories](configuration.md#directories)), so repository files in the current directory are never read as user patterns or configuration because of it.
- A cloned repository that ships that directory (for example `./patterns/*.toml`) has its files loaded as trusted user patterns when `oo` runs in it.
- The same config directory holds the pending learn-status file and `config.toml` (see [configuration](configuration.md#files-in-configuration-directory)). These are always read from the absolute configuration directory described above.
- `[store] retention` in `config.toml` sets how long indexed output is kept (read on every indexing run; `0` disables purging). It is read only from the configuration directory, never from the current directory.
- A `[learn]` table in `config.toml`, when present, must set all three of `provider` (only `anthropic` is accepted), `model` and `api_key_env`. A repository that controls this file can choose the model and name the environment variable whose value is read and sent as the API key, but it cannot set the API URL. That URL comes only from the `ANTHROPIC_API_URL` environment variable (default `https://api.anthropic.com/v1/messages`); its validation rules are in [When oo makes network calls](#when-oo-makes-network-calls).
- A malformed `[learn]` table does not change retention, and an invalid `[store]` value does not stop `oo learn`. A TOML syntax error or an unreadable `config.toml` (a file with mode 000, or a directory named `config.toml`) affects both, except when the config directory itself is unsearchable (mode 000 on the directory): then `oo learn` silently uses its defaults while indexing still warns ([configuration](configuration.md#store-section)).

**Project patterns are not trusted by design.** They are read from `<git-root>/.oo/patterns/` (or the current directory when there is no git root) from whatever repository you are working in, with no confirmation. A cloned repository you did not write can therefore supply patterns, and those patterns are applied automatically whenever `oo` runs or routes a command (`oo <cmd>`, `oo rewrite`, `oo hook claude`). The one exception is the foreground display of `oo learn`, which uses built-in patterns only (#151).

What a pattern can do (from `src/pattern/mod.rs` and `src/classify.rs`):

- **Failure (exit code ≠ 0)**: the failure strategy (`tail`, `head`, `grep`, or `between`) selects which lines of the output are printed. Any other lines are omitted from the display, at any output size, but when the displayed slice differs from the full merged output (line-for-line comparison), the full output is indexed so `oo recall` can still find what the display left out. A command with no matching pattern, or whose pattern has no `failure` section, falls back to the default truncation (`smart_truncate`: output of 120 lines or fewer is shown in full, unchanged; output over 120 lines keeps its first 72 and last 48 lines around a truncation marker). A failure strategy that selects no lines prints no filtered lines: only the `✗` line and empty separator lines remain.
- **Success (exit code 0)**: only when the merged output is larger than 4 KiB. The success strategy (`tail`, `head`, `grep`, or a regex with a summary template) replaces the displayed output with the extracted text, shown in parentheses on the `✓ label (...)` line. Template text is printed as written, so a pattern can display arbitrary text. If the strategy extracts nothing (no regex match, or grep with no matching lines), the category default applies; a regex that matches with an empty `summary` template gives an empty summary, which is not indexed and does not fall back to the category default; categories are fixed in code and a pattern cannot set them.
- **Command selection (`oo rewrite`, `oo hook claude`)**: a command (each shell segment of it) is routed through `oo` when any loaded pattern's `command_match` matches that text, so a project pattern can add commands to that set.
- **Precedence**: project, then user, then built-in; first match wins. `command_match` is an unanchored regex, matched against the full command for `oo <cmd>` and against each shell segment for `oo rewrite` and `oo hook claude`, so a broad project pattern can shadow user and built-in patterns for the commands it matches. Within one directory, pattern files are loaded in unsorted `read_dir` order, so when two patterns in the same directory match a command, which one wins is not defined; keep their `command_match` values disjoint.

What a pattern cannot do: run commands or any other process, read or write files, change the exit code `oo` returns, or change the command that `oo <cmd>` executes. The `✓`/`✗` status comes from the real exit code. Invalid pattern files are silently skipped when loaded; `oo patterns` may still list them.

**Consequence**: a project pattern can hide or replace the output you see, including failure lines, and a pattern-matched run is not stored for `oo recall` when its success strategy produces a summary. If the pattern has no `success` section, or its extraction yields nothing (no regex match, or grep with no matching lines), the output is handled exactly as for an unpatterned run of that command (so Data and Content/Unknown output is indexed, while Status output is not). A regex that matches with an empty summary template is not indexed. Failure runs are indexed for recall when the displayed slice does not match the full output line for line (ignoring trailing empty lines), with or without a pattern (a pattern that drops lines from a failure display therefore does not drop them from `oo recall`). For an untrusted repository, review `.oo/patterns/` (or run `oo patterns`, which lists the project patterns of the current repository under a `Project` header) before relying on `oo` output from it. Deleting the directory or the offending `.toml` file removes the effect. `oo <cmd>` still executes the command exactly as you gave it; patterns affect only what `oo` prints and which commands are routed through it.

## File System Access

### Read Access

oo reads from:
- Standard input (commands)
- Environment variables (runtime only)
- Configuration directory (`~/.config/oo/` on Linux, or `$XDG_CONFIG_HOME/oo/` when `XDG_CONFIG_HOME` is an absolute path; `~/Library/Application Support/oo/` on macOS; `OO_CONFIG_DIR` overrides, see [Directories](configuration.md#directories))
- Pattern directory (`patterns/` inside the configuration directory)
- Storage directory (SQLite store, default build only; `~/.local/share/.oo/` on Linux, or `$XDG_DATA_HOME/.oo/` when `XDG_DATA_HOME` is an absolute path; `~/Library/Application Support/.oo/` on macOS; `OO_DATA_DIR` overrides, see [Directories](configuration.md#directories))
- With `vipune-store`, Vipune's database (`~/.vipune/memories.db` by default) and its config file (`vipune/config.toml` in the platform config directory) instead of `oo.db`
- Project-local patterns (`.oo/patterns/` in the git root, or in the current directory when there is no git root), loaded each time oo classifies a command

By default the configuration and storage directories are under your user home directory; if no home directory can be determined they fall back under `/tmp` (see [configuration.md](configuration.md#directories)). On Linux, they move under `$XDG_CONFIG_HOME` and `$XDG_DATA_HOME` when those are absolute paths. The absolute `OO_CONFIG_DIR` and `OO_DATA_DIR` overrides can point anywhere, so those paths are not confined to your home directory. Residual risk: an invalid (relative or padded) `OO_CONFIG_DIR` or `OO_DATA_DIR` is not isolation. oo writes a stderr warning and falls back to the real default directories above, so a harness that sets a relative value expecting isolation reads and writes your real configuration and store. Project-local patterns are read from the repository, not from home.

### Write Access

oo writes to:
- Storage directory (`~/.local/share/.oo/` on Linux, or `$XDG_DATA_HOME/.oo/` when `XDG_DATA_HOME` is an absolute path; `~/Library/Application Support/.oo/` on macOS; `OO_DATA_DIR` overrides when absolute; an ignored value falls back to the default with a stderr warning, see Read Access): Indexed outputs (SQLite store; with `vipune-store`, Vipune's database instead). On Unix, a data directory oo creates is made with mode `0700` (missing parents keep the umask mode), and `oo.db` is created with mode `0600` or, if it already exists, tightened on open (see Storage Backends)
- Hugging Face cache (`~/.cache/huggingface/hub`; vipune 0.1.2 ignores `HF_HOME`), `vipune-store` builds only: embedding model files, downloaded when the store is opened and not yet cached (vipune's `model_cache` setting is not used for this in 0.1.2, from source inspection)
- Pattern directory (`patterns/` inside the configuration directory): Learned patterns (via `oo learn`)
- Status file (`learn-status.log` inside the configuration directory): Background learn results. It is created with mode 0600 on Unix, a pre-existing file keeps its mode, and a missing config directory is created with default permissions. If it cannot be removed after being shown, the same lines are shown again on every run until it is removed.

Writes made by `oo init` (only when you run it):
- `oo init` or `oo init --format claude`: `.claude/hooks.json` in the git root (or the current directory when there is no git root)
- `oo init --agent claude-code`: `.claude/settings.json` in the git root (same fallback)
- `oo init --agent claude-code --global`: `$OO_CLAUDE_DIR/settings.json`, or `~/.claude/settings.json` when `OO_CLAUDE_DIR` is unset
- `oo init --agent pi`: `.pi/extensions/oo.ts` in the git root (same fallback)
- `oo init --agent pi --global`: `$OO_PI_EXTENSIONS_DIR/oo.ts`, or `~/.pi/agent/extensions/oo.ts` when `OO_PI_EXTENSIONS_DIR` is unset

By default these writes stay under your user home directory (global installs) or the repository root, falling back to the current directory when there is no git root (project installs). The absolute `OO_CONFIG_DIR` and `OO_DATA_DIR` overrides, and `OO_CLAUDE_DIR` and `OO_PI_EXTENSIONS_DIR`, can point anywhere. A set-but-empty `OO_CLAUDE_DIR` or `OO_PI_EXTENSIONS_DIR` is an error (exit 1, nothing written); it does not fall back to the home directory.

Temporary files (not listed above):
- `oo init --agent claude-code` writes `<settings.json>.tmp` next to the target settings file and renames it over the target, for both the project and `--global` paths.
- `oo learn` writes a JSON payload to a temporary file in the system temp directory (`$TMPDIR`, typically `/tmp` on Linux and `/var/folders/...` on macOS). An empty or relative `$TMPDIR` falls back to `/tmp`, so the file never lands in the current directory.
  - Content: the command line, the complete captured output (stdout and stderr merged, which may contain secrets), the exit code, and the hint if given.
  - Created by the `tempfile` crate with mode `0600`; the name is `.tmp` plus random characters (for example `.tmpAbC123`).
  - The background process removes it as soon as it has read it, before any parse or learn step. A failed removal is recorded in `learn-status.log`. If that file cannot be written either (for example, the config directory path is blocked by a regular file), nothing is printed, the failure is lost, and the file stays on disk.
  - Only a file with the exact tempfile name shape (`.tmp` plus six alphanumeric characters) directly inside the directory given on the hidden `_learn_bg` argv is read or removed; any other path is neither read nor removed and the background run records an error. This checks the name and directory only: argv is untrusted, so it does not prove this process's parent created the file.
  - A background process killed before it reads the file (SIGKILL, reboot) leaves it behind. If spawning the process fails, the file is removed immediately.
  - The background child reads its argv as OS strings and treats the payload path and directory as `Path` values, so a non-UTF-8 `$TMPDIR` works fully: the payload is read and removed as usual, and a failure is recorded in `learn-status.log`.
- Manual cleanup of leftover files: list the `.tmp*` files in `$TMPDIR` that you own (or in `/tmp` when `TMPDIR` is empty or relative), check their contents, and delete them. The name is shared with other programs that use the `tempfile` crate, so do not delete files you have not checked.

User-run commands are not confined: `oo <cmd>` executes the command you give it with your permissions, so that command may read or write any path it can reach, wherever it is. Apart from the paths, temporary files, and directories it creates (`create_dir_all` for the configuration, pattern, `.claude/`, and `.pi/extensions/` directories, and for the missing parents of the storage directory; the storage directory itself is created with mode `0700` on Unix) listed here, oo does not write files.

### Execution Access

oo executes commands using your shell environment:
- Commands run with your user permissions
- Environment variables are inherited
- PATH is inherited

**Risk**: If a command in your PATH is malicious or compromised, oo executes it as-is. This is the same risk as running commands in a terminal.

## Network Access

### When oo makes network calls

In the default build, oo itself makes network requests in two cases:

**Via `oo learn`**:
- To the Anthropic API (`https://api.anthropic.com/v1/messages` by default). `ANTHROPIC_API_URL` may override it: it must use HTTPS, except `http://` to a host that is exactly `localhost`, `127.0.0.1` or `[::1]` (compared case-insensitively), and it must not contain userinfo (`user@`). Other schemes are rejected. The URL is validated once, and the request is sent to that validated URL.
- Redirects are not followed. The HTTP client is configured with `max_redirects(0)`, so any 3xx status (300 to 399, with or without a `Location` header, including 304) fails the request with `unexpected redirect (status N): redirects are not followed; ANTHROPIC_API_URL must point directly at the API endpoint`. The `x-api-key` header is never sent to a `Location` target, and the error does not include the `Location` value.
- With your command output and API key
- In the background, after command completes

**Via `oo help <command>`** (not for reserved subcommands):
- An HTTPS GET to `https://cheat.sh/<command>?T`, with the command name URL-encoded. The HTTP client follows redirects (up to 10 by default), so a redirect causes further requests. Redirect targets are not restricted to cheat.sh and no API key is sent; the request to a target is built from its `Location` header, so the command name reaches the target only if that header contains it (from source)
- Reserved subcommands (for example `oo help recall`) print the local usage text and make no request

**Via the embedding store (only with `--features vipune-store`)**:
- When the store is opened (for example by `oo recall`, `oo forget`, or indexing output) and the embedding model is not yet in the Hugging Face cache, vipune downloads the model (only if the database's parent directory exists; vipune canonicalizes that path before loading the model) over HTTPS from `https://huggingface.co` (vipune 0.1.2 ignores `HF_ENDPOINT`): `onnx/model.onnx` (or `model.onnx`) and `tokenizer.json`. The model is set by vipune's `embedding_model` / `VIPUNE_EMBEDDING_MODEL` (default `BAAI/bge-small-en-v1.5`). Cached files are not re-downloaded
- The request carries no command output. If a Hugging Face token file exists at `~/.cache/huggingface/token`, it is sent as a `Bearer` header
- At build time, the `ort` dependency's default `download-binaries` feature fetches ONNX Runtime binaries during `cargo build`
- All of the above is from source inspection of vipune 0.1.2, hf-hub 0.4.3 and ort 2.0.0-rc.10. It was not observed at runtime, and the `vipune-store` build was not compiled

**No other network calls** (beyond the cases above):
- No telemetry
- No updates
- No cloud storage

### When commands make network calls

Commands you run via `oo` may make network calls (e.g., `git fetch`, `curl`, `cargo fetch`). These are executed as-is with your network access. oo does not intercept or modify them.

## Project-Scoped Storage

Indexed output is keyed by a project id. In the default build the id is the last path segment of the `origin` remote URL (`.git` stripped; an empty last segment, e.g. from a trailing `/`, falls through to the next step), falling back to the git root directory name, then the current directory name. With `--features vipune-store` the id comes from vipune's own project detection instead: `VIPUNE_PROJECT` is honoured first, then vipune uses the git remote in `owner/repo` form for common https/ssh URLs (vipune 0.1.2, the locked version).

- `oo recall` searches indexed output whose project id matches the current project, regardless of which session stored it
- `oo forget` deletes every indexed entry with that project id on the default SQLite store. With `--features vipune-store` it can fail with a partial-delete error; see Data retention above
- The project id is an organisational key, **not a security boundary**. Two repositories with the same remote name or directory name (for example two checkouts of `app`) share one partition: each can recall the other's indexed output, and `oo forget` in either one clears both (SQLite store).

Use `oo forget` to clear a project's index when done.

## Auditing

### What is logged

oo does not log command execution or output. No audit trail is kept locally.

### What is not logged

- Commands executed
- Outputs generated
- Patterns applied
- Queries run via `oo recall`

If you need auditing, use your shell history or tools like `atop`.

## Known Limitations

### No sandboxing

oo does not sandbox commands. If a command deletes files, accesses network, or reads sensitive data, oo executes it as-is.

### No command validation

oo does not check if a command is safe. `rm -rf /` is executed without warning.

### API key exposure in process list

When running `oo learn`, the API key is in the environment and may be visible in your process list (`ps`, `top`). This is a known limitation of environment variables.

**Mitigation**: Ensure your environment is not exposed to other users or processes.

### No encryption at rest

SQLite database stores output as plain text. If your machine is compromised, indexed outputs are readable.

**Mitigation**: Encrypt your home directory or use full-disk encryption.

## Security Best Practices

### For Users

1. **Review your patterns**: Check the user patterns directory (`oo patterns` prints its path) and, for any repository you did not write, its `.oo/patterns/` directory (`oo patterns` lists project patterns) for unknown or suspicious files
2. **Use `oo forget`**: Clear indexed output for a project when done
3. **Protect API keys**: Use environment variables, not config files
4. **Audit your PATH**: Ensure commands in PATH are from trusted sources
5. **Read before running**: Don't blindly trust commands generated by AI agents

### For Contributors

1. **No secret storage**: Never implement API key caching or persistence
2. **Validate URLs**: Enforce HTTPS for all network connections (plain HTTP only to `localhost`/`127.0.0.1`/`[::1]`); the `oo learn` URL and redirect rules are in [When oo makes network calls](#when-oo-makes-network-calls)
3. **Sanitize inputs**: Use `clap` for argument parsing
4. **Test for injection**: Ensure malformed input doesn't cause crashes or security issues
5. **Document assumptions**: Be clear about what is trusted and what is not

## Reporting Security Issues

Report security vulnerabilities privately through [GitHub Security Advisories](https://github.com/randomm/oo/security/advisories) — this ensures responsible disclosure.

**Do not open a public issue for security vulnerabilities.** Include:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if known)

We aim to acknowledge reports within 7 days and will communicate a remediation timeline. This section mirrors the [SECURITY.md](../SECURITY.md) policy.

## Compliance

This section is for compliance assessments.

### Data Location

- **Command output**: In the default build, stored in `oo.db` in the storage directory (see [Read Access](#read-access)). With `--features vipune-store`, stored in Vipune's database (`~/.vipune/memories.db` by default). Sent to Anthropic only when you run `oo learn` (see LLM data)
- **Patterns**: Local-only, in the user patterns directory (under the configuration directory) and in project-local `.oo/patterns/` (see [Read Access](#read-access))
- **LLM data**: Sent to Anthropic (US-based), subject to Anthropic's privacy policy

### Data Access

- **Owner**: User account running oo
- **Permissions**: The SQLite database file (default build) is created with mode `0600` (`rw-------`) and a data directory oo creates with mode `0700` (see Storage Backends). An existing `oo.db` is tightened on open (see Storage Backends), and a data directory that already existed keeps its mode. With `vipune-store`, oo does not set the mode of Vipune's database (the vipune-side mode was not checked)
- **Sharing**: No data sharing or syncing capabilities

### Data Retention

- **Indexed outputs**: Purged automatically after a configurable retention period (default 7 days), lazily on the next indexing run of the same project — see [configuration.md](configuration.md#store-section). With `--features vipune-store` the purge inspects only the newest 10,000 entries of a project, so older entries may persist beyond the retention period
- **Patterns**: Permanent (until manually deleted)
- **LLM data**: Per Anthropic's retention policy (typically undefined, may be used for service improvement)

### Right to Delete

- **Local data**: Delete the storage directory and the user patterns directory (paths above). With `vipune-store`, also delete Vipune's database (`~/.vipune/memories.db` by default) and the Hugging Face model cache (`~/.cache/huggingface/hub`)
- **LLM data**: No mechanism; contact Anthropic directly if needed

## Further Reading

- [Architecture](architecture.md) — System design
- [Learning Patterns](learn.md) — LLM integration details
- [Contributing Guide](../CONTRIBUTING.md) — Security requirements for contributors