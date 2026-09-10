# Documentation

This directory contains detailed documentation for `oo` beyond the user-facing README.

## Contents

- **[Testing Guide](testing.md)** — How to run, write, and understand oo's tests
- **[Architecture](architecture.md)** — System design and module responsibilities
- **[Security Model](security-model.md)** — Trust assumptions, data handling, and API key security
- **[Patterns](patterns.md)** — Creating custom patterns for command output compression
- **[Learning Patterns](learn.md)** — Using `oo learn` to automatically generate patterns
- **[CLI Reference](cli-reference.md)** — All subcommands, output tiers, and the savings indicator spec
- **[Configuration](configuration.md)** — Environment variables, config files, and platform paths

## Quick Links

- **For users**: See the [main README](../README.md) for installation, commands, and usage examples
- **For contributors**: See [CONTRIBUTING.md](../CONTRIBUTING.md) for development workflow and coding standards
- **For agents**: See [AGENTS.md](../AGENTS.md) for project-specific agent conventions

## Documentation Conventions

All documentation in this project follows these principles:

- **Audience-focused**: Written for the reader (you), not abstract "users"
- **Concrete**: Real commands, real examples, copy-pasteable
- **Scannable**: Headers tell the story; important info up front
- **Honest**: Acknowledges limitations and known issues
- **Maintainable**: One topic per document, cross-referenced rather than duplicated

See [tech-doc-writer](https://github.com/torokati44/tech-doc-writer) for the methodology behind these docs.