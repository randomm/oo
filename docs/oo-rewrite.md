# `oo` Rewrite

> **Status: experimental** — the command set and output format may change
> without notice.

`oo rewrite` prints an `oo`-prefixed version of a shell command, for agent
hooks that need the rewritten form before execution. It never executes the
command and never reads from or writes to the output index store.

## Syntax

```sh
oo rewrite <command...>
```

If at least one shell segment (split on `&&`, `||`, `;`) matches an `oo`
pattern, the matching segments are prefixed with `oo` and the result is
printed on **stdout** with exit code **0**.
If no segment matches, nothing is printed and exit code is **1**.

## Examples

```sh
$ oo rewrite pytest -q
oo pytest -q

$ oo rewrite git status && pytest -q
git status && oo pytest -q

$ oo rewrite cargo test > log.txt
# (no output — redirections are refused)
exit 1
```

## Edge cases

- **Pipes** (`|`), **redirections** (`<`/`>`), **heredocs** (`<<`),
  **command substitution** (`$()`/backticks), and **background** (`&`)
  are not rewritten — `oo` can't safely reason about them, so exit 1.
- **`&&`, `||`, `;`** inside quotes are not segment separators.
- **`VAR=value`** environment prefixes are preserved in front of `oo`.
- Already-`oo`-prefixed segments are left as-is (no double-wrap).
