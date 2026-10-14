# Documentation

Detailed documentation for `oo`, grouped by audience. The [main README](../README.md) covers installation and a quick tour.

## Using oo

- **[Agent Integration](agents.md)** — Install, verify, disable and uninstall the pi and Claude Code hooks; what to commit; other agents
- **[Custom Patterns](patterns.md)** — Creating patterns for command output compression
- **[Learning Patterns](learn.md)** — Using `oo learn` to generate patterns automatically
- **[Security Model](security-model.md)** — Trust assumptions, data handling, and API key security

## Agents

- **[Agent Integration](agents.md)** — Per-agent setup and the recall instruction to add to `CLAUDE.md` or `AGENTS.md`
- **[`oo rewrite`](oo-rewrite.md)** — Which commands the hooks rewrite, and the rules that decide it (experimental)

## Reference

- **[CLI Reference](cli-reference.md)** — All subcommands, flags, exit codes, file paths, output tiers, and the savings indicator spec
- **[Configuration](configuration.md)** — Environment variables, config files, and platform paths

## Maintainers

- **[Architecture](architecture.md)** — System design and module responsibilities
- **[Testing Guide](testing.md)** — How to run, write, and understand oo's tests

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
