# Security Model

This document describes oo's security properties, trust assumptions, and how it handles sensitive data.

## Core Principle: Local-First, Transparent

oo is designed to be safe and predictable:

1. **All execution is local** — Commands run on your machine exactly as you typed them
2. **No telemetry or data collection** — No data leaves your machine except what you explicitly send via the commands you run
3. **Transparent modification** — oo only compresses output; it doesn't modify command behavior or inject side effects
4. **Clear boundaries** — You control what gets indexed, stored, and transmitted

## Trust Assumptions

### Required Trust

You must trust:

1. **The oo binary** — It executes commands and can read/write local files
2. **The LLM provider (Anthropic)** — When using `oo learn`, your command output is sent to Anthropic (see below)
3. **Your shell environment** — Commands run with your existing environment variables, PATH, and shell settings

### What We Don't Trust (and Why)

- **Remote servers**: All command execution is local. No remote API calls except when you explicitly use `oo learn`.
- **Command inputs**: Commands are parsed but not modified. If a command is malicious (e.g., `rm -rf /`), oo executes it as-is — the security boundary is between you and the command, not oo.

## Data Handling

### Command Output

**Where it goes**:

- **Small outputs (<4 KiB)**: Displayed directly, never stored
- **Pattern-matched outputs**: Extracted summary displayed, original output discarded
- **Large (Data) and Bounded (Content/Unknown) unpatterned outputs**: Stored locally in the SQLite index (see Storage Backends) for recall

**Data retention**:
- Indexed outputs persist until you run `oo forget` or delete them manually
- No automatic syncing to remote services
- Project-scoped — `oo forget` clears indexed output for the current project. With the default SQLite store this is complete; with `--features vipune-store` it is bounded for projects over 10,000 entries and may leave entries behind (#182)

### Storage Backends

**SQLite (default)**:
- Database file: `oo.db`, in `<data dir>`. Default `<data dir>` is `~/.local/share/.oo/` on Linux (`$XDG_DATA_HOME/.oo/` when `XDG_DATA_HOME` is an absolute path) and `~/Library/Application Support/.oo/` on macOS. With `OO_DATA_DIR=/x` the database is `/x/oo.db` (no `.oo/` is added). A relative `OO_DATA_DIR` is resolved against the current working directory, and an empty `OO_DATA_DIR=` puts the database at `./oo.db`
- Permissions: the file is created with the default mode from the process umask (`0644` under the common umask `022`); oo does not set a restrictive mode. For sensitive environments, run `chmod 600` on the file or use a restrictive umask (e.g. `umask 077`) before running oo
- Contents: Command output, metadata (command, timestamp, session identifier — informational only, not used for scoping)
- Other local users can read it if the file and its parent directory permit it

**Vipune (optional, feature flag)**:
- Stores outputs in Vipune's database
- Same access controls as your Vipune installation
- Only used if you explicitly enable the `vipune-store` feature

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
- **Required**: `ANTHROPIC_API_KEY` environment variable
- **Model**: `claude-haiku-4-5` (default, configurable)
- **Endpoint**: `https://api.anthropic.com`
- **URL validation**: oo validates that the API URL uses HTTPS; plain HTTP is accepted only for `localhost` and `127.0.0.1`, other non-HTTPS URLs are rejected with an error

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
- Keys are sent only to the configured API endpoint (Anthropic)
- HTTPS is enforced; plain HTTP is accepted only for `localhost` and `127.0.0.1`
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

Patterns are TOML files with a `command_match` regex and optional `success` and `failure` strategies. Built-in patterns and user patterns (in the user patterns directory, see Read Access) are trusted by design: you installed them, wrote them, or accepted them, including patterns that `oo learn` writes into that directory from LLM output, which you should review.

**Project patterns are not trusted by design.** They are read from `<git-root>/.oo/patterns/` (or the current directory when there is no git root) from whatever repository you are working in, with no confirmation. A cloned repository you did not write can therefore supply patterns, and those patterns are applied automatically whenever `oo` runs or routes a command (`oo <cmd>`, `oo rewrite`, `oo hook claude`). The one exception is the foreground display of `oo learn`, which uses built-in patterns only (#151).

What a pattern can do (from `src/pattern/mod.rs` and `src/classify.rs`):

- **Failure (exit code ≠ 0)**: the failure strategy (`tail`, `head`, `grep`, or `between`) selects which lines of the output are printed. Any other lines are omitted from the display, at any output size. A pattern without a `failure` section falls back to the default truncation (`smart_truncate`: output of 80 lines or fewer is shown in full; output of 81 to 120 lines has all of its lines present and no marker, except that its last head line and first tail line are joined into one line with no newline between them; output over 120 lines keeps its first 72 and last 48 lines around a truncation marker). A failure strategy that selects no lines prints no output lines.
- **Success (exit code 0)**: only when the merged output is larger than 4 KB. The success strategy (`tail`, `head`, `grep`, or a regex with a summary template) replaces the displayed output with the extracted text, shown in parentheses on the `✓ label (...)` line. Template text is printed as written, so a pattern can display arbitrary text. If the strategy extracts nothing (no regex match, or grep with no matching lines), the category default applies; a regex that matches with an empty `summary` template gives an empty summary, which is not indexed and does not fall back to the category default; categories are fixed in code and a pattern cannot set them.
- **Command selection (`oo rewrite`, `oo hook claude`)**: a command (each shell segment of it) is routed through `oo` when any loaded pattern's `command_match` matches that text, so a project pattern can add commands to that set.
- **Precedence**: project, then user, then built-in; first match wins. `command_match` is an unanchored regex, matched against the full command for `oo <cmd>` and against each shell segment for `oo rewrite` and `oo hook claude`, so a broad project pattern can shadow user and built-in patterns for the commands it matches. Within one directory, pattern files are loaded in unsorted `read_dir` order, so when two patterns in the same directory match a command, which one wins is not defined; keep their `command_match` values disjoint.

What a pattern cannot do: run commands or any other process, read or write files, change the exit code `oo` returns, or change the command that `oo <cmd>` executes. The `✓`/`✗` status comes from the real exit code. Invalid pattern files are silently skipped when loaded; `oo patterns` may still list them.

**Consequence**: a project pattern can hide or replace the output you see, including failure lines, and a pattern-matched run is not stored for `oo recall` when its success strategy produces a summary. If the pattern has no `success` section, or its extraction yields nothing (no regex match, or grep with no matching lines), the output is handled exactly as for an unpatterned run of that command (so Data and Content/Unknown output is indexed, while Status output is not). A regex that matches with an empty summary template is not indexed. Failure runs are never indexed, with or without a pattern. For an untrusted repository, review `.oo/patterns/` (or run `oo patterns`, which lists the project patterns of the current repository under a `Project` header) before relying on `oo` output from it. Deleting the directory or the offending `.toml` file removes the effect. `oo <cmd>` still executes the command exactly as you gave it; patterns affect only what `oo` prints and which commands are routed through it.

## File System Access

### Read Access

oo reads from:
- Standard input (commands)
- Environment variables (runtime only)
- Configuration directory (`~/.config/oo/` on Linux, or `$XDG_CONFIG_HOME/oo/` when `XDG_CONFIG_HOME` is an absolute path; `~/Library/Application Support/oo/` on macOS; `OO_CONFIG_DIR` overrides; a relative value is resolved against the current working directory, and an empty value means the current working directory)
- Pattern directory (`patterns/` inside the configuration directory)
- Storage directory (`~/.local/share/.oo/` on Linux, or `$XDG_DATA_HOME/.oo/` when `XDG_DATA_HOME` is an absolute path; `~/Library/Application Support/.oo/` on macOS; `OO_DATA_DIR` overrides; a relative value is resolved against the current working directory, and an empty value means the current working directory)
- Project-local patterns (`.oo/patterns/` in the git root, or in the current directory when there is no git root), loaded each time oo classifies a command

By default the configuration and storage directories are under your user home directory; if no home directory can be determined they fall back under `/tmp` (see [configuration.md](configuration.md#directories)). On Linux, they move under `$XDG_CONFIG_HOME` and `$XDG_DATA_HOME` when those are absolute paths. The `OO_CONFIG_DIR` and `OO_DATA_DIR` overrides can point anywhere, so those paths are not confined to your home directory. Project-local patterns are read from the repository, not from home.

### Write Access

oo writes to:
- Storage directory (`~/.local/share/.oo/` on Linux, or `$XDG_DATA_HOME/.oo/` when `XDG_DATA_HOME` is an absolute path; `~/Library/Application Support/.oo/` on macOS; `OO_DATA_DIR` overrides): Indexed outputs
- Pattern directory (`patterns/` inside the configuration directory): Learned patterns (via `oo learn`)
- Status file (`learn-status.log` inside the configuration directory): Background learn results

Writes made by `oo init` (only when you run it):
- `oo init` or `oo init --format claude`: `.claude/hooks.json` in the git root (or the current directory when there is no git root)
- `oo init --agent claude-code`: `.claude/settings.json` in the git root (same fallback)
- `oo init --agent claude-code --global`: `$OO_CLAUDE_DIR/settings.json`, or `~/.claude/settings.json` when `OO_CLAUDE_DIR` is unset
- `oo init --agent pi`: `.pi/extensions/oo.ts` in the git root (same fallback)
- `oo init --agent pi --global`: `$OO_PI_EXTENSIONS_DIR/oo.ts`, or `~/.pi/agent/extensions/oo.ts` when `OO_PI_EXTENSIONS_DIR` is unset

By default these writes stay under your user home directory (global installs) or the repository root, falling back to the current directory when there is no git root (project installs). The `OO_CONFIG_DIR`, `OO_DATA_DIR`, `OO_CLAUDE_DIR`, and `OO_PI_EXTENSIONS_DIR` overrides can point anywhere. A set-but-empty `OO_CLAUDE_DIR` or `OO_PI_EXTENSIONS_DIR` is an error (exit 1, nothing written); it does not fall back to the home directory.

Temporary files (not listed above):
- `oo init --agent claude-code` writes `<settings.json>.tmp` next to the target settings file and renames it over the target, for both the project and `--global` paths.
- `oo learn` writes a JSON file to a named temporary file in the system temp directory (`std::env::temp_dir()`, the system temp directory `$TMPDIR`, typically `/tmp` on Linux and `/var/folders/...` on macOS; with an empty TMPDIR the temp file path is relative to the current directory, observed with the debug binary). The JSON holds the full command line, the complete captured command output (stdout and stderr merged, which may contain secrets or other sensitive data), the exit code, and the hint if one was given. The file is created by the `tempfile` crate with mode `0600` (owner read/write only, before the umask is applied), so other local users cannot read it. Its name is `.tmp` followed by random characters, for example `.tmpAbC123`. The background process deletes it after its whole learn attempt ends, whether that attempt succeeded or failed. The attempt can include up to 3 Anthropic API calls (retried only when the returned TOML is invalid; API and HTTP errors are not retried), each of which can take up to 30 seconds, so the output stays on disk for that whole period. The file is left behind if the background process cannot read it, the JSON does not parse, or the exit code is out of range, and if the background process is killed mid-attempt (for example by SIGKILL or a reboot). If spawning the process itself fails, the file is deleted immediately.
- Manual cleanup of leftover files: list the `.tmp*` files in `$TMPDIR` that you own (when `TMPDIR` is set but empty, they are in the current directory instead), check their contents, and delete them. The name is shared with other programs that use the `tempfile` crate, so do not delete files you have not checked.

User-run commands are not confined: `oo <cmd>` executes the command you give it with your permissions, so that command may read or write any path it can reach, wherever it is. Apart from the paths, temporary files, and directories it creates (`create_dir_all` for the configuration, storage, pattern, `.claude/`, and `.pi/extensions/` directories) listed here, oo does not write files.

### Execution Access

oo executes commands using your shell environment:
- Commands run with your user permissions
- Environment variables are inherited
- PATH is inherited

**Risk**: If a command in your PATH is malicious or compromised, oo executes it as-is. This is the same risk as running commands in a terminal.

## Network Access

### When oo makes network calls

**Only via `oo learn`**:
- To the Anthropic API (`https://api.anthropic.com/v1/messages` by default; `ANTHROPIC_API_URL` may override it and must use HTTPS, except `http://` to localhost/127.0.0.1)
- With your command output and API key
- In the background, after command completes

**No other network calls**:
- No telemetry
- No updates
- No cloud storage

### When commands make network calls

Commands you run via `oo` may make network calls (e.g., `git fetch`, `curl`, `cargo fetch`). These are executed as-is with your network access. oo does not intercept or modify them.

## Project-Scoped Storage

Indexed output is keyed by a project id. In the default build the id is the last path segment of the `origin` remote URL (`.git` stripped; an empty last segment, e.g. from a trailing `/`, falls through to the next step), falling back to the git root directory name, then the current directory name. With `--features vipune-store` the id comes from vipune's own project detection instead: `VIPUNE_PROJECT` is honoured first, then vipune uses the git remote in `owner/repo` form for common https/ssh URLs (vipune 0.1.2, the locked version).

- `oo recall` searches indexed output whose project id matches the current project, regardless of which session stored it
- `oo forget` deletes every indexed entry with that project id on the default SQLite store. With `--features vipune-store`, projects over 10,000 entries may keep entries after `oo forget` (#182)
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
2. **Validate URLs**: Enforce HTTPS for all network connections
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

- **Command output**: Local-only, stored in the storage directory (see [Read Access](#read-access))
- **Patterns**: Local-only, in the user patterns directory (under the configuration directory) and in project-local `.oo/patterns/` (see [Read Access](#read-access))
- **LLM data**: Sent to Anthropic (US-based), subject to Anthropic's privacy policy

### Data Access

- **Owner**: User account running oo
- **Permissions**: Storage files use the default mode from the process umask (not `rw-------`); use `chmod 600` or a restrictive umask for sensitive environments
- **Sharing**: No data sharing or syncing capabilities

### Data Retention

- **Indexed outputs**: Until manually deleted or `oo forget` runs
- **Patterns**: Permanent (until manually deleted)
- **LLM data**: Per Anthropic's retention policy (typically undefined, may be used for service improvement)

### Right to Delete

- **Local data**: Delete the storage directory and the user patterns directory (paths above)
- **LLM data**: No mechanism; contact Anthropic directly if needed

## Further Reading

- [Architecture](architecture.md) — System design
- [Learning Patterns](learn.md) — LLM integration details
- [Contributing Guide](../CONTRIBUTING.md) — Security requirements for contributors