# AGENTS.md — `oo` Project

Agent guidelines for the `oo` codebase (Rust, Edition 2024).

---

## 1. Context7 Protocol

Before writing any Rust code, resolve current documentation via Context7:

```
context7_resolve-library-id → context7_query-docs
```

Use this for crate APIs (`tokio`, `clap`, `rusqlite`, `thiserror`, `assert_cmd`, etc.).
Training data may be outdated — Context7 has authoritative current docs.

---

## 2. Minimalist Engineering Philosophy

**Every line of code is a liability.** Before creating anything:

- **LESS IS MORE**: Question necessity before creation
- **Challenge Everything**: Ask "Is this truly needed?" before implementing
- **Minimal Viable Solution**: Build the simplest thing that fully solves the problem
- **No Speculative Features**: Don't build for "future needs" - solve today's problem
- **Prefer Existing**: Reuse existing code/tools before creating new ones
- **One Purpose Per Component**: Each function/module should do one thing well

### Pre-Creation Challenge (MANDATORY)

Before creating ANY code, ask:
1. Is this explicitly required by the GitHub issue?
2. Can existing code/tools solve this instead?
3. What's the SIMPLEST way to meet the requirement?
4. Will removing this break core functionality?
5. Am I building for hypothetical future needs?

**If you cannot justify the necessity, DO NOT CREATE IT.**

---

## 3. Pre-Push Quality Gates

All checks must pass locally before any push. CI is for **verification**, not discovery.

```bash
cargo fmt --check          # formatting
cargo clippy -- -D warnings  # lint (warnings are errors)
cargo test                 # all tests pass
cargo tarpaulin --fail-under 70  # coverage gate (70% interim, 80% target after VCR cassettes)
cargo build --release      # release build succeeds
```

Fix failures locally. Never push to "see if CI catches anything."

---

## 4. Testing Standards

- **TDD preferred** — write tests before implementation
- **80% line coverage minimum** for new code — enforced via `cargo tarpaulin`
- **Unit tests** in-module (`#[cfg(test)]`) for pure logic
- **Integration tests** in `tests/` for CLI behaviour — use `assert_cmd`
- Integration tests are the primary coverage driver — they test real CLI invocations
- Every new pattern in `src/pattern/` must have a corresponding test
- Tests must provide real value — no trivial assertions like `assert!(true)` or `assert_eq!(1, 1)`
- Every test must exercise a real code path and assert meaningful behaviour
- Network-dependent tests must be marked `#[ignore]` with a comment explaining why
- ~197 tests currently; do not reduce this count

---

## 5. Code Style & Conventions

- Edition 2024, `rust-version = "1.85"` — use modern Rust idioms
- `thiserror` for all error types — no `unwrap()` in library code, only in tests
- No `#[allow(...)]` suppressions without a comment explaining why
- **File size**: 500 lines hard cap, 300 lines ideal — refactor if exceeded
- Module responsibilities must stay single-purpose (see Architecture section)
- No `TODO` / `FIXME` / `HACK` comments — open a GitHub issue instead
- Code comments explain **WHY**, not WHAT

---

## 6. Git Workflow

- **Branch naming**: `feature/issue-N-short-description` or `fix/issue-N-short-description`
- **Conventional commits**: `feat(#N): description`, `fix(#N): description`, `chore: description`
- Every PR must link to a GitHub issue — include `Fixes #N` in the PR body
- No force push to `main`
- All quality gates must pass before opening a PR
- **Merge policy**: Agents may squash merge PRs that have been approved by the code review agent. No manual approval required in that case.

---

## 7. Documentation Policy

- `README.md` is user-facing — keep it accurate (the 200-PR test: only document what stays true)
- Public API items get doc comments (`///`)
- Do **not** commit agent work artifacts: no `RESEARCH.md`, `PLAN.md`, `DESIGN.md`, `ANALYSIS.md`
- Legitimate docs go in `docs/` with lowercase-hyphenated filenames

---

## 8. Architecture Notes

Orientation for agents new to the codebase:

| Module | Responsibility |
|--------|---------------|
| `src/main.rs` | CLI entry point and subcommand dispatch |
| `src/exec.rs` | Shell command execution and output capture |
| `src/classify.rs` | Classification engine — touch carefully, well-tested |
| `src/pattern/` | Built-in patterns (10), user TOML loading, pattern matching (split into `mod.rs`, `builtins.rs`, `toml.rs`) |
| `src/store.rs` | `SqliteStore` (default) + optional `VipuneStore` feature flag |
| `src/session.rs` | Session tracking and context management |
| `src/learn.rs` | LLM integration, background re-exec, Anthropic only |
| `src/error.rs` | Unified error types via `thiserror` |

**Key rules:**
- New commands go in `src/main.rs` dispatch, with logic in their own module
- Reserved subcommands: `recall`, `forget`, `learn`, `help`, `init`, `version` — everything else is a shell command to execute
- `src/classify.rs` is the critical path — changes require corresponding tests

**Open issues to be aware of:**
- `#1` — rust version alignment
- `#2` — `oo help` tldr
- `#3` — `oo init` hooks

---

## 9. Commands Reference

```bash
cargo build --release        # build optimised binary
cargo test                   # run all tests (unit + integration)
cargo clippy -- -D warnings  # lint (warnings = errors)
cargo fmt                    # format code
cargo fmt --check            # check formatting (used in CI)
```

Binary name: `oo` — GitHub repo: `randomm/oo`
