# Contributing to oo

Thank you for your interest in contributing! This guide covers the essentials.

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

- Edition 2024, Rust 1.85+
- Use `thiserror` for error types
- No `unwrap()` in library code (tests OK)
- File size: 500 lines hard cap, 300 lines ideal

## Testing

- TDD preferred — write tests before implementation
- 80%+ coverage for new code (enforced by `cargo tarpaulin`)
- Unit tests in-module (`#[cfg(test)]`)
- Integration tests in `tests/` using `assert_cmd`
- Network-dependent tests: mark `#[ignore]` with explanation

## Additional Guidelines

- Public API items get `///` doc comments
- Comments explain WHY, not WHAT
- No TODO/FIXME comments — open a GitHub issue instead
- Minimalist engineering: question necessity before creating

See [AGENTS.md](AGENTS.md) for complete project conventions.