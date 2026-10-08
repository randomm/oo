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

A segment is rewritten when *any* pattern's `command_match` regex matches
*anywhere* in the segment text — patterns are unanchored, and quoted text is
not excluded from matching. This is the same matching semantics as
`oo <command>`, so `oo rewrite nope pytest -q` prints
`oo nope pytest -q`. Exit code 1 also covers the case where a pattern file
failed to load (patterns are loaded project, user, then builtin).

The rewritten output is only guaranteed to re-parse identically under POSIX
shell quoting rules: backslashes outside single quotes escape the next
character (an escaped `"` does not toggle quoting, `\&\&` is not a
separator), inside single quotes a backslash is literal, and the exact
original text — backslashes included — is preserved byte-for-byte.

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
- **Backslashes** outside single quotes escape the next character: an
  escaped quote does not toggle quoting, `\&\&` is not a separator, and an
  escaped `|`/`<`/`>`/`$()`/backtick is not refused. Inside single quotes a
  backslash is literal. A trailing lone backslash or an unterminated quote
  yields no rewrite (exit 1), since neither re-parses identically.
- **`VAR=value`** environment prefixes are preserved in front of `oo`.
- Already-`oo`-prefixed segments are left as-is (no double-wrap).
- Commands longer than 16 KiB are refused (exit 1, no output) — the bound
  caps worst-case regex work over the command string.
