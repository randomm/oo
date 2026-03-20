# Contributing to oo

Thank you for your interest in contributing! This guide covers the essentials.

## Development Setup

### Prerequisites

- **Rust**: 1.85 or later (check with `rustc --version`)
- **Cargo**: Included with Rust
- **Git**: For version control

### First-Time Setup

1. **Clone the repository**:

   ```bash
   git clone https://github.com/randomm/oo.git
   cd oo
   ```

2. **Verify Rust version**:

   ```bash
   rustc --version
   # Should output 1.85 or later
   ```

3. **Build the project**:

   ```bash
   cargo build --release
   ```

4. **Run tests** to ensure everything works:

   ```bash
   cargo test
   ```

5. **Create a symlink for development** (optional):

   ```bash
   ln -sf $(pwd)/target/release/oo /usr/local/bin/oo
   ```

   Or add `$(pwd)/target/release` to your PATH.

### First-Run Verification

After building, verify oo works with a simple command:

```bash
./target/release/oo echo "hello, world"
```

Expected output:
```
hello, world
```

For a more comprehensive test, try indexing a large output:

```bash
./target/release/oo git log --oneline -100
```

Expected output (if output > 4 KB):
```
● git log (indexed X.XX KiB → use `oo recall` to query)
```

## Workflow

1. **Fork and clone** your fork
2. **Create a branch**: `feature/issue-N-description` or `fix/issue-N-description`
3. **Make changes** with conventional commits: `feat(#N): description`, `fix(#N): description`, `chore: description`
4. **Run quality gates** (ALL must pass):
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   cargo tarpaulin --fail-under 70
   ```
5. **Open a PR** linking to a GitHub issue with `Fixes #N` in the body

## Code Style

- **Edition**: 2024
- **Rust version**: 1.85+
- **Error handling**: Use `thiserror` for error types; no `unwrap()` in library code (tests OK)
- **File size**: 500 lines hard cap, 300 lines ideal
- **No suppressions**: No `#[allow(...)]` without explanatory comment
- **Comments**: Explain WHY, not WHAT

## Testing

See [Testing Guide](docs/testing.md) for comprehensive details.

### Quick Start

- **TDD preferred** — write tests before implementation
- **80%+ coverage** for new code (enforced by `cargo tarpaulin`)
- **Unit tests** in-module (`#[cfg(test)]`)
- **Integration tests** in `tests/` using `assert_cmd`
- **Network-dependent tests**: mark `#[ignore]` with explanation

### Running Tests

```bash
# All tests
cargo test

# Unit tests only
cargo test --lib

# Integration tests only
cargo test --test integration

# Coverage
cargo tarpaulin --fail-under 70
```

## Documentation

### Public API

- All public API items require `///` doc comments
- See [Architecture](docs/architecture.md) for module responsibilities
- Use `cargo doc --open` to preview documentation

### For Users

- `README.md` is user-facing — keep it accurate
- Legitimate docs go in `docs/` with lowercase-hyphenated filenames
- No agent work artifacts (RESEARCH.md, PLAN.md, etc.)

## Release Notes

When contributing changes that will appear in releases:

1. **Write human-readable summaries**: Explain user impact, not just technical changes
   - ✅ "Fixes crash when git output contains non-UTF8 characters"
   - ❌ "Fix: handle decode error in exec.rs"
2. **Focus on what changed**: Users care about effects, not implementation details
3. **Keep it concise**: One sentence per entry is usually sufficient
4. **Prioritize visible changes**: Bug fixes, new features, breaking changes first
5. **Document breaking changes clearly**: If something requires user action, say so

> **Note**: The changelog is generated from conventional commits. Manual summaries in `CHANGELOG.md` should supplement (not replace) generated entries with minimal narrative context about user impact.

Examples of good release notes:
- "Fix: pattern matching now works with multi-byte Unicode characters"
- "Add: `oo init` generates Claude-compatible hooks automatically"
- "Change: SQLite database moved to `~/.local/share/.oo/` for XDG compliance"

See existing entries in [CHANGELOG.md](CHANGELOG.md) for style reference.

## Minimalist Engineering

Before creating code, ask:

1. Is this explicitly required by the GitHub issue?
2. Can existing code/tools solve this instead?
3. What's the SIMPLEST way to meet the requirement?
4. Will removing this break core functionality?
5. Am I building for hypothetical future needs?

If you cannot justify necessity, DO NOT create it.

**No TODO/FIXME comments** — open a GitHub issue instead.

## Additional Resources

- **[Architecture](docs/architecture.md)** — System design and module responsibilities
- **[Security Model](docs/security-model.md)** — Trust assumptions and data handling
- **[Testing Guide](docs/testing.md)** — How to write and run tests
- **[AGENTS.md](AGENTS.md)** — Complete project conventions for AI agents

## Quick Reference

### Common Commands

```bash
cargo build --release        # build optimised binary
cargo test                   # run all tests (unit + integration)
cargo clippy -- -D warnings  # lint (warnings = errors)
cargo fmt                    # format code
cargo fmt --check            # check formatting (used in CI)
```

### Branch Naming

- `feature/issue-N-description` for new features
- `fix/issue-N-description` for bug fixes

### Commit Messages

- `feat(#N): description` — new feature
- `fix(#N): description` — bug fix
- `chore: description` — maintenance task

## Questions?

- Check [Architecture](docs/architecture.md) and [Testing Guide](docs/testing.md) first
- See [AGENTS.md](AGENTS.md) for agent-specific conventions
- Open a GitHub issue if you're unsure about something