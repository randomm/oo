# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.10.1] - 2026-10-14

### Documentation

- *(#225,#226,#227)* Accurate install, agent integration guide, and a README that explains oo in one screen ([#228](https://github.com/randomm/oo/pull/228))


## [0.10.0] - 2026-10-13

### ⚠ Breaking Changes

- *(#213)* `double_o::exec::run` takes `&[OsString]` instead of `&[String]`, and `double_o::learn::run_background` takes `data_path: &Path` instead of `&str`; its full signature is now `run_background(data_path: &Path, payload_dir: &Path, status_path: &Path)`. See [Library API Stability](docs/architecture.md#library-api-stability).
- *(#213)* The `#[doc(hidden)]` items `double_o::commands::{parse_action, run_command_args, cmd_run, cmd_learn}` and the crate-root re-exports `double_o::{parse_action, cmd_run, cmd_learn}` take `&[OsString]`. `Action::Run` carries `Vec<OsString>`; `Action::Learn` carries `Vec<OsString>` and `Option<String>`. New `#[doc(hidden)]` module `double_o::argv`.
- *(#213)* Non-UTF-8 arguments to oo's own inputs (`recall`, `forget`, `help`, `rewrite`, `hook`, `init`, `patterns`, `version`, and the `--hint` value of `learn`) exit 2 with `oo: argument is not valid UTF-8` instead of panicking. Wrapped-command arguments are not checked, except that a non-UTF-8 argument directly after a `--hint` token in `oo learn` is rejected even when it belongs to the wrapped command.
- *(#213)* On Unix, wrapped-command arguments reach the child as their original bytes; labels, history and learn payloads use the lossy UTF-8 form.
- *(#215)* With `vipune-store`, `oo forget` fails with `partial delete: deleted N, failed M; entries remain` unless a listing finds no entries. Previously a failed delete also failed, with `partial delete: removed X of Y entries, Z deletion(s) failed`, and a listing whose ids all returned not-found reported success. If the listing itself fails, the message is `partial delete: deleted N, failed M; entries may remain; listing failed: <detail>`.
- *(#215)* With `vipune-store`, retention cleanup inspects the newest 10,000 entries of the project, attempts to delete every stale `oo` entry among them, and returns an error if any delete fails; it previously stopped at the first failed delete. It does not drain the project.

### Bug Fixes

- *(#214)* Skip non-regular *.toml entries in the pattern loader
- *(#213)* Never panic on non-UTF-8 arguments; pass wrapped-command bytes through
- *(#215)* Drain Vipune projects completely or fail loudly; saturating cleanup cutoff
- *(#220)* Make env-dependent lib tests hermetic; NUL-terminate the mkfifo path

### Documentation

- Record the library API stability policy; exec::run and learn::run_background take OsString/Path


## [0.9.0] - 2026-10-12

### ⚠ Breaking Changes

- *(#200)* `double_o::learn::run_background` takes two more arguments: it is now `run_background(data_path: &str, payload_dir: &Path, status_path: &Path)` (was `run_background(data_path: &str)`). Callers must pass the payload directory the payload was created in and the learn status file path.
- *(#200)* The learn status helpers moved to the `#[doc(hidden)]` module `double_o::learn_status`: `write_learn_status`, `write_learn_status_failure` and `check_and_clear_learn_status` are no longer at `double_o::commands::*`, and `write_learn_status` and `check_and_clear_learn_status` are no longer re-exported at the crate root. `double_o::learn::learn_status_path` is unchanged.
- *(#199)* `double_o::commands::cmd_patterns_in` (`#[doc(hidden)]`) keeps its signature, but for an unreadable directory it now prints `oo: <path>: <error>` to stderr and returns 1. It previously printed the empty-state line and returned 0.

### Bug Fixes

- *(#190,#193)* Validate the learn API URL with a parser and refuse redirects
- *(#194)* Show failing output up to 120 lines in full instead of fusing lines
- *(#200)* Learn background child removes its payload and records failures
- *(#196)* Create the database private (0700 dir, 0600 files) and tighten existing ones
- *(#197)* Resolve OO_CONFIG_DIR and OO_DATA_DIR in one place; ignore relative, padded values with a warning
- *(#199)* Patterns empty state, SIGPIPE default and empty API key handling
- *(#198)* Report a failed indexing in the Large and Bounded tiers instead of advertising recall
- *(#208)* Compile on the declared MSRV 1.85 and add an MSRV CI job

### Documentation

- *(#181)* Correct the documentation against v0.8.0 behavior


## [0.8.0] - 2026-10-11

### ⚠ Breaking Changes

- *(#185)* `Classification::Failure` has a new public field `full_output: String`. Code that destructures or constructs this variant without `..` must be updated (add `..` to patterns, supply `full_output` in literals). The library API is pre-1.0, so this is a minor-version bump.
- *(#178)* `oo init` rejects unknown options, unsupported `--format` values, stray positional arguments, and repeated `--agent`, `--format` or `--global` (these were previously ignored or warned about and fell back to the default).

### Features

- *(#184)* Make recall retention configurable via [store] retention (#191). The default retention for indexed output changes from 24 hours to 7 days; `retention = 0` disables purging, and invalid or unknown `[store]` keys warn and fall back to the default.

### Bug Fixes

- *(#178)* Handle --help on reserved subcommands
- *(#185)* Index the full output of failed commands for oo recall (#192)

### Documentation

- *(#181)* Correct known inaccuracies and align docs with observed CLI behavior (#183)
- *(#181)* Correct savings indicator rules, learn temp file lifecycle and pattern source statements (#187)

## [0.7.0] - 2026-10-08

### Bug Fixes

- *(#168)* Make recall and forget project-scoped

### Features

- *(#170)* Add oo rewrite subcommand for agent hooks
- *(#171)* Add oo init --agent pi
- *(#172)* Add oo init --agent claude-code and oo hook claude


## [0.6.0] - 2026-09-10

### Bug Fixes

- *(#149)* Strip sudo/env prefixes and recognise cargo nextest run as Status (#157)
- *(help)* Detect HTML responses from cheat.sh and map to no-help error ([#164](https://github.com/randomm/oo/pull/164))

### Features

- *(#148)* Bound the Content/Unknown passthrough path (#155)
- *(#150)* Report compression savings in indicator lines (#156)
- *(#147)* Return FTS5 snippets from oo recall instead of full blobs (#158)
- List all 7 reserved subcommands in both help surfaces ([#163](https://github.com/randomm/oo/pull/163)) ([#165](https://github.com/randomm/oo/pull/165))

### Miscellaneous

- Fix documentation accuracy, consistency, and new-user experience ([#167](https://github.com/randomm/oo/pull/167))

### Refactoring

- *(#151)* Unify classification and display into a single path (#153)


## [0.5.0] - 2026-05-20

### Features

- *(#55,#57,#58)* Learn hints and expanded command patterns


### Features
- Added 14 new built-in patterns for common commands ([#58](https://github.com/randomm/oo/issues/58)):
  - `npm test`, `yarn test`, `pnpm test`, `bun test` - JavaScript/TypeScript test frameworks
  - `cargo tarpaulin` - Rust code coverage
  - `cargo fmt` - Rust formatting
  - `mypy` - Python type checking
  - `rubocop` - Ruby linting
  - `ruff format` - Python formatting
  - `prettier` - Code formatting
  - `npm run build`, `yarn build`, `pnpm build`, `bun build` - JavaScript/TypeScript build tools

### Documentation
- Updated README with expanded built-in patterns table (now 24 total patterns)

## [0.4.5] - 2026-04-16

### Features

- *(distribution)* Enable shell installer ([#143](https://github.com/randomm/oo/pull/143)) ([#144](https://github.com/randomm/oo/pull/144))


## [0.4.4] - 2026-03-20


## [0.4.3] - 2026-03-20

### Bug Fixes

- Correct pattern directory path in README
- Correct tarball extraction paths in installation instructions


## [0.4.2] - 2026-03-20


## [0.4.0] - 2026-03-19

### Bug Fixes

- Restore full README content with code-fence ASCII logo ([#84](https://github.com/randomm/oo/pull/84))

### Features

- New ascii logo ([#81](https://github.com/randomm/oo/pull/81))

### Miscellaneous

- *(#96,#97)* Add Cargo.toml metadata and README badges ([#103](https://github.com/randomm/oo/pull/103))
- *(#93,#101)* Add community files and SPDX license header ([#102](https://github.com/randomm/oo/pull/102))
- *(#89,#90,#91)* Eliminate unwrap, restrict API surface, split pattern.rs ([#105](https://github.com/randomm/oo/pull/105))


## [0.3.2] - 2026-03-05

### Bug Fixes

- *(#76,#77)* Add retry loop to oo learn, remove OpenAI/Cerebras ([#78](https://github.com/randomm/oo/pull/78))


## [0.3.1] - 2026-03-04

### Bug Fixes

- Improved `oo learn` pattern quality with better system prompt framing
- Corrected Anthropic model name and surfaced learn failures to users
- Fixed pattern filenames to include subcommands (e.g., `cargo-test.toml`)
- Replaced zalgo logo with plain "oo" text in CLI output

### Documentation

- Documented `oo learn` overwrite behavior


## [0.3.0] - 2026-03-03

### Features

- Initial implementation of `oo learn` for LLM-assisted pattern generation
- Added ASCII art logo

### Bug Fixes

- Formatting issues in README header
- README with styled div for context

### Documentation

- Add spacing below zalgo logo to avoid h1 border clipping
- Use div-wrapped h1 to suppress border clipping on zalgo logo
- Update README with description and clean HTML
- Change font style and size in README
- Use bold text for zalgo logo to avoid h1 border clipping
- Use processed PNG logo instead of SVG/text for zalgo header ([#23](https://github.com/randomm/oo/pull/23))