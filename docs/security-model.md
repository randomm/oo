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
- **Large unpatterned outputs**: Stored locally in the SQLite index (see Storage Backends) for recall

**Data retention**:
- Indexed outputs persist until you run `oo forget` or delete them manually
- No automatic syncing to remote services
- Project-scoped — `oo forget` clears all indexed output for the current project

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

User patterns in the user patterns directory (see Read Access) are trusted by design:

- Patterns are loaded from a user-owned directory
- No privilege escalation
- Invalid patterns are silently discarded (not crashed)

**Risk**: Malicious patterns could hide output. Trust pattern sources (don't download arbitrary TOML files into your patterns directory).

## File System Access

### Read Access

oo reads from:
- Standard input (commands)
- Environment variables (runtime only)
- Configuration directory (`~/.config/oo/` on Linux, `~/Library/Application Support/oo/` on macOS; `OO_CONFIG_DIR` overrides; a relative value is resolved against the current working directory, and an empty value means the current working directory)
- Pattern directory (`patterns/` inside the configuration directory)
- Storage directory (`~/.local/share/.oo/` on Linux, `~/Library/Application Support/.oo/` on macOS; `OO_DATA_DIR` overrides; a relative value is resolved against the current working directory, and an empty value means the current working directory)

All are within your user home directory.

### Write Access

oo writes to:
- Storage directory (`~/.local/share/.oo/` on Linux, `~/Library/Application Support/.oo/` on macOS): Indexed outputs
- Pattern directory (`patterns/` inside the configuration directory): Learned patterns (via `oo learn`)
- Status file (`learn-status.log` inside the configuration directory): Background learn results

No writes outside your user home directory.

### Execution Access

oo executes commands using your shell environment:
- Commands run with your user permissions
- Environment variables are inherited
- PATH is inherited

**Risk**: If a command in your PATH is malicious or compromised, oo executes it as-is. This is the same risk as running commands in a terminal.

## Network Access

### When oo makes network calls

**Only via `oo learn`**:
- To Anthropic API (HTTPS enforced)
- With your command output and API key
- In the background, after command completes

**No other network calls**:
- No telemetry
- No updates
- No cloud storage

### When commands make network calls

Commands you run via `oo` may make network calls (e.g., `git fetch`, `curl`, `cargo fetch`). These are executed as-is with your network access. oo does not intercept or modify them.

## Project-Scoped Storage

Indexed output is keyed by a project id. In the default build the id is the last path segment of the `origin` remote URL (`.git` stripped), falling back to the git root directory name, then the current directory name. With `--features vipune-store` the id comes from vipune's own project detection instead: `VIPUNE_PROJECT` is honoured first, then vipune uses the git remote in `owner/repo` form for common https/ssh URLs (vipune 0.1.2, the locked version).

- `oo recall` searches indexed output whose project id matches the current project, regardless of which session stored it
- `oo forget` deletes every indexed entry with that project id
- The project id is an organisational key, **not a security boundary**. Two repositories with the same remote name or directory name (for example two checkouts of `app`) share one partition: each can recall the other's indexed output, and `oo forget` in either one clears both.

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

**Mitigation**: Use `ANTHROPIC_API_URL` instead (if supported by the provider) or ensure your environment is not exposed.

### No encryption at rest

SQLite database stores output as plain text. If your machine is compromised, indexed outputs are readable.

**Mitigation**: Encrypt your home directory or use full-disk encryption.

## Security Best Practices

### For Users

1. **Review your patterns**: Check the user patterns directory (`oo patterns` prints its path) for unknown or suspicious files
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

- **Command output**: Local-only, stored in the storage directory (`~/.local/share/.oo/` on Linux, `~/Library/Application Support/.oo/` on macOS)
- **Patterns**: Local-only, stored in the user patterns directory (`~/.config/oo/patterns/` on Linux, `~/Library/Application Support/oo/patterns/` on macOS)
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