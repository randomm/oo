```
  ██████   ██████ 
 ███▒▒███ ███▒▒███
▒███ ▒███▒███ ▒███
▒███ ▒███▒███ ▒███
▒▒██████ ▒▒██████ 
 ▒▒▒▒▒▒   ▒▒▒▒▒▒ 
```

<br /><br />

*"Double-o, agent's best friend."* 

*Or: how I learned to stop worrying and love my context-efficient command runner for AI coding agents.*

---

## The Problem

AI agents see everything you print. A `cargo test` run producing 8 KB of output
costs the agent hundreds of tokens just to learn "tests passed." Multiply that
across a session and the context window fills with noise, not signal. `oo` runs
commands for you and collapses their output to what the agent actually needs.

---

## Output Tiers

```
$ oo ls -la
total 48
drwxr-xr-x  8 user user 4096 Mar  2 09:00 .
...
```
Small output (≤ 4 KB) passes through verbatim — no wrapping, no prefix.

```
$ oo pytest tests/
✓ pytest (47 passed, 3.2s)
```
Large output with a known success pattern collapses to a single summary line.

```
$ oo pytest tests/
✗ pytest

FAILED tests/test_api.py::test_login - AssertionError: expected 200, got 401
