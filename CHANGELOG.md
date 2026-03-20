# Changelog

All notable changes to this project will be documented in this file.

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