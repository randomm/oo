//! `oo init --agent pi` — install a pi (pi-coding-agent) extension.
//!
//! The generated TypeScript extension hooks pi's `tool_call` event for bash
//! and rewrites commands via `oo rewrite` (issue #170), so pi agents get
//! token-efficient output without manual `oo` prefixing.

use crate::error::Error;
use crate::init::find_root;
use std::fs;
use std::path::{Path, PathBuf};

/// Target extension filename inside the pi extensions directory.
pub const EXTENSION_FILENAME: &str = "oo.ts";

/// The generated pi extension, embedded as a constant so the installer writes
/// byte-identical files (idempotency is byte-equality, not substring match).
///
/// Behaviour contract (see issue #171 acceptance criteria):
/// - carries an `ooAvailable` flag (true initially) that the load-time
///   `oo --version` probe updates: false on any probe failure (missing
///   binary, non-zero exit, timeout, exception), true on success; while the
///   asynchronous probe is still pending the flag is treated as available —
///   the handler never blocks on it;
/// - the `tool_call` handler acts only on `toolName === "bash"`, and every
///   guard returns immediately (undefined) BEFORE any `pi.exec` call: the
///   `ooAvailable` flag, empty commands, commands already starting with
///   `oo `, `event.parentToolCallId` set (nested codemode calls are left
///   raw), and the `OO_DISABLE=1` environment opt-out;
/// - calls `oo rewrite <cmd>` via `pi.exec` with a 2000ms timeout and the
///   context abort signal, mutating `event.input.command` only when the exit
///   code is 0 and stdout is non-empty and differs from the original;
/// - the whole handler body is wrapped in try/catch and returns undefined on
///   any error (a tool_call handler failure would BLOCK the tool, so this is
///   a fail-open requirement, not a preference).
pub const OO_TS_EXTENSION: &str = r#"// oo pi extension — installs via: oo init --agent pi (v0.1)
// Rewrites bare shell commands to their `oo`-prefixed form for token savings.
//
// Thin delegating extension: all rewrite logic lives in `oo rewrite`
// (the single source of truth — oo's pattern registry). To change rewrite
// behaviour, run `oo learn <cmd>` or edit .oo/patterns — not this file.
//
// Exit code contract for `oo rewrite`:
//   0 + stdout  Rewrite found -> mutate command
//   1           No rewrite    -> pass through unchanged
//
// Disable for a session: OO_DISABLE=1
// Uninstall: delete this file.

import type {
  BashToolCallEvent,
  ExtensionAPI,
  ToolCallEvent,
} from "@earendil-works/pi-coding-agent";

const REWRITE_TIMEOUT_MS = 2000;

// Availability of the `oo` binary. True initially so a slow load-time probe
// never makes the first tool call wait; flipped by the probe — false on any
// probe failure (missing binary, non-zero exit, timeout, exception), true on
// success. While the probe is still pending the handler treats the binary as
// available (a failed rewrite is a harmless pass-through anyway).
let ooAvailable = true;

// Local reimplementation of the package's `isToolCallEventType("bash", event)`
// type guard: that helper is a value export, so importing it would pull in the
// whole pi barrel at extension load. The type-only imports below are erased at
// compile time and carry none of that cost.
function isBashToolCallEvent(event) {
  return event.toolName === "bash";
}

// Calls `oo rewrite`; returns the rewritten command or null (pass through).
async function rewriteCommand(pi, cmd, signal) {
  const result = await pi.exec("oo", ["rewrite", cmd], {
    timeout: REWRITE_TIMEOUT_MS,
    signal,
  });
  if (result.killed) return null;
  if (result.code !== 0) return null;
  const rewritten = result.stdout.trim();
  return rewritten === "" ? null : rewritten;
}

// Retain the session_start context so a status note can be applied as soon as
// the load-time probe resolves, without blocking extension load itself.
function registerOoUnavailableNotice(pi) {
  let reason = undefined;
  let sessionContext = undefined;

  const applyStatus = () => {
    if (!reason || !sessionContext) return;
    try {
      sessionContext.ui?.setStatus?.("oo", `oo disabled: ${reason}`);
    } catch {
      // Status reporting must never affect fail-open behavior.
    }
  };

  try {
    pi.on("session_start", (_event, ctx) => {
      sessionContext = ctx;
      applyStatus();
    });
  } catch {
    // Runtimes without a session_start event: nothing to report.
    return (_reason) => {};
  }

  return (nextReason) => {
    reason = nextReason;
    applyStatus();
  };
}

export default async function (pi) {
  const reportOoUnavailable = registerOoUnavailableNotice(pi);

  // Register the handler first (same ordering as rtk's extension): a slow
  // version probe must never leave a gap where bash tool calls are missed.
  pi.on("tool_call", async (event, ctx) => {
    try {
      if (!ooAvailable) return;

      if (!isBashToolCallEvent(event)) return;

      const cmd = event.input.command;
      if (typeof cmd !== "string" || cmd.trim() === "") return;

      // Nested calls (codemode scripts, other tools) must see raw,
      // unwrapped output — leave them untouched.
      if (event.parentToolCallId) return;

      // Already an oo command (including `oo rewrite` itself) — no-op.
      // This guard also short-circuits before the rewrite pi.exec call, so
      // a slow `oo` is never invoked on an oo-prefixed command.
      if (cmd.startsWith("oo ")) return;

      if (process.env.OO_DISABLE === "1") return;

      const rewritten = await rewriteCommand(pi, cmd, ctx.signal);
      if (rewritten && rewritten !== cmd) {
        event.input.command = rewritten;
      }
    } catch (err) {
      // Fail open: a tool_call handler failure BLOCKS the tool as a
      // fail-safe, so any unexpected error here must pass the command
      // through instead.
      console.warn("[oo] unexpected error in tool_call handler; passing through command", err);
      return;
    }
  });

  // Probe `oo` at load time; disable silently (status note only) if the
  // binary is missing or errors. Any exit-0 output counts as a version
  // success — `oo --version` prints "oo <version>". On any probe failure
  // (missing binary, non-zero exit, timeout, exception) the handler is
  // switched to a hard pass-through no-op via `ooAvailable = false` (oo
  // rewrite would fail anyway), so nothing is lost.
  try {
    const ver = await pi.exec("oo", ["--version"], { timeout: REWRITE_TIMEOUT_MS });
    if (ver.code !== 0) {
      ooAvailable = false;
      reportOoUnavailable("oo binary not found in PATH");
      console.warn("[oo] oo binary not found in PATH — extension disabled");
    } else {
      ooAvailable = true;
    }
  } catch (err) {
    ooAvailable = false;
    reportOoUnavailable("oo version probe failed");
    console.warn("[oo] could not probe oo — extension disabled", err);
  }
}
"#;

/// Resolve the directory the extension file is written into.
///
/// Environment dependencies (global mode only):
///
/// - `OO_PI_EXTENSIONS_DIR`: when set (and non-empty) it IS the extensions
///   directory itself — the file lands at `$OO_PI_EXTENSIONS_DIR/oo.ts` and
///   the value is a trusted path used as-is (no sanitisation, no suffix
///   appended). A set-but-empty value is an error.
/// - `HOME`: consulted only when `OO_PI_EXTENSIONS_DIR` is not set; the
///   directory is `<home>/.pi/agent/extensions`. Unset or empty `HOME` is an
///   error.
///
/// - project: `<git-root>/.pi/extensions` — `find_root` walks up to `.git`
///   and falls back to cwd outside a repo. No environment variables are
///   consulted.
///
/// The global path never consults `find_root`/git root.
fn target_dir(cwd: &Path, global: bool) -> Result<PathBuf, Error> {
    if global {
        if let Some(override_dir) = std::env::var_os("OO_PI_EXTENSIONS_DIR") {
            if override_dir.is_empty() {
                return Err(Error::Init(format!(
                    "OO_PI_EXTENSIONS_DIR is set but empty; expected the pi extensions directory (got {:?})",
                    override_dir
                )));
            }
            return Ok(PathBuf::from(override_dir));
        }
        match std::env::var_os("HOME") {
            Some(home) if !home.as_os_str().is_empty() => {
                Ok(PathBuf::from(home).join(".pi").join("agent").join("extensions"))
            }
            _ => Err(Error::Init(
                "cannot determine the global pi extensions directory: $OO_PI_EXTENSIONS_DIR is not set and $HOME is unset or empty"
                    .to_string(),
            )),
        }
    } else {
        Ok(find_root(cwd).join(".pi").join("extensions"))
    }
}

/// Install the pi extension for `oo init --agent pi`.
///
/// Idempotency is two-way: an existing file with identical content is a no-op
/// ("already installed", exit 0); an existing file with different content is
/// NOT overwritten — the caller is told how to proceed (still exit 0, mirroring
/// the Claude path's warn-and-skip semantics).
pub fn run(cwd: &Path, global: bool) -> Result<(), Error> {
    let dir = target_dir(cwd, global)?;
    let path = dir.join(EXTENSION_FILENAME);

    fs::create_dir_all(&dir)
        .map_err(|e| Error::Init(format!("cannot create {}: {e}", dir.display())))?;

    if path.exists() {
        let existing = fs::read_to_string(&path)
            .map_err(|e| Error::Init(format!("cannot read {}: {e}", path.display())))?;
        if existing == OO_TS_EXTENSION {
            println!("{} already installed — nothing to do", path.display());
            return Ok(());
        }
        eprintln!(
            "oo init: {} already exists with different content — not overwritten.",
            path.display()
        );
        eprintln!(
            "To install the bundled version: back up or delete the file, then re-run `oo init --agent pi`."
        );
        return Ok(());
    }

    fs::write(&path, OO_TS_EXTENSION)
        .map_err(|e| Error::Init(format!("cannot write {}: {e}", path.display())))?;
    println!("Created {}", path.display());

    println!();
    println!("The extension rewrites bare shell commands via `oo rewrite` when pi runs them.");
    println!("Disable for a session with OO_DISABLE=1, or uninstall by deleting the file.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // set_var is process-global and tests run on parallel threads — every
    // env-mutating test below takes this lock first.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn with_home<F: FnOnce(PathBuf)>(home: &Path, f: F) {
        let _guard = env_guard();
        unsafe {
            std::env::set_var("HOME", home);
            std::env::remove_var("OO_PI_EXTENSIONS_DIR");
        }
        f(home.to_path_buf());
        unsafe { std::env::remove_var("OO_PI_EXTENSIONS_DIR") };
    }

    // -----------------------------------------------------------------------
    // OO_TS_EXTENSION structural contract (issue #171 acceptance criteria)
    // -----------------------------------------------------------------------

    #[test]
    fn extension_source_contains_required_tokens() {
        for token in [
            "ooAvailable",      // hard-disable flag consulted first by the handler
            "parentToolCallId", // nested codemode calls left raw
            "try {",            // handler fail-open wrapper
            "catch",            // ...with catch, returning undefined
            "2000",             // rewrite timeout
            "oo rewrite",       // the delegated rewriter
            "OO_DISABLE",       // env opt-out
            "tool_call",        // the hooked event
            "bash",             // the hooked tool
        ] {
            assert!(
                OO_TS_EXTENSION.contains(token),
                "extension source must contain {token:?}"
            );
        }
    }

    #[test]
    fn extension_source_has_version_marker() {
        assert!(
            OO_TS_EXTENSION.starts_with("// oo pi extension") && OO_TS_EXTENSION.contains("(v0.1)"),
            "extension must carry a version marker comment"
        );
    }

    #[test]
    fn extension_source_oo_available_guard_first_in_handler() {
        // The `ooAvailable` hard-disable guard must short-circuit BEFORE every
        // other handler guard (bash-only, empty, parentToolCallId, `oo ` prefix)
        // and before any `pi.exec` call.
        let handler = OO_TS_EXTENSION.find("pi.on(").expect("handler");
        let scope = &OO_TS_EXTENSION[handler..];
        let available = scope
            .find("if (!ooAvailable) return;")
            .expect("ooAvailable guard");
        for token in [
            "isBashToolCallEvent(event)",
            "cmd.trim() === \"\"",
            "parentToolCallId",
            "cmd.startsWith(\"oo \")",
            "rewriteCommand(pi, cmd, ctx.signal)",
        ] {
            let at = scope[available..].find(token).expect(token) + available;
            assert!(
                available < at,
                "ooAvailable guard must precede {token:?} in the handler"
            );
        }
    }

    #[test]
    fn extension_source_oo_prefix_guard_before_exec() {
        // The `oo ` prefix guard must short-circuit BEFORE the rewrite
        // `pi.exec` call. The rewrite call lives inside `rewriteCommand`
        // (a helper outside the handler), so the contract is asserted on
        // handler-local source order: the guard appears before the
        // `rewriteCommand(pi, cmd, ctx.signal)` invocation in the handler.
        let handler = OO_TS_EXTENSION.find("pi.on(").expect("handler");
        let guard = OO_TS_EXTENSION
            .find("cmd.startsWith(\"oo \")")
            .expect("guard");
        let exec_call = OO_TS_EXTENSION[handler..]
            .find("rewriteCommand(pi, cmd, ctx.signal)")
            .expect("rewrite call")
            + handler;
        assert!(
            guard < exec_call,
            "`oo ` prefix guard must appear before the rewrite pi.exec invocation"
        );
    }

    /// Gated syntax check: when a `node` or `bun` binary is available, the
    /// embedded TypeScript must parse; otherwise the test skips (it must never
    /// fail the suite for lack of a JS runtime).
    #[test]
    fn extension_source_parses_with_available_js_runtime() {
        let runtime = ["node", "bun"].iter().find(|r| which(r));
        let Some(runtime) = runtime else {
            eprintln!("skipping JS syntax check: no node/bun binary found");
            return;
        };
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("oo_ext.ts");
        fs::write(&src, OO_TS_EXTENSION).unwrap();
        let out = std::process::Command::new(runtime)
            .args(["--check", &src.to_string_lossy()])
            .env("HOME", dir.path())
            .output()
            .expect("failed to spawn JS runtime");
        assert!(
            out.status.success(),
            "embedded extension must be valid syntax for {runtime}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn which(binary: &str) -> bool {
        std::process::Command::new("which")
            .arg(binary)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Node-gated runtime test for the probe-failure path (issue #171
    /// lens-review fix 5). Skips cleanly when no `node` binary is available.
    ///
    /// The generated extension is loaded under plain node with a stub `pi`
    /// whose `exec` always rejects (simulating a missing `oo` binary). After
    /// the default export settles, a bash `tool_call` event is fired; the
    /// expected behavior is that the handler returns without calling
    /// `pi.exec("oo", ["rewrite", ...])` because the failed probe set
    /// `ooAvailable = false`.
    ///
    /// The harness is a tiny `.mjs` file that stubs `pi.exec` and `pi.on`,
    /// imports the extension (after stripping type annotations so plain
    /// node can load it), and prints a JSON verdict. The test asserts the
    /// verdict shows no rewrite call was made.
    #[test]
    fn extension_probe_failure_disables_handler() {
        // Skip cleanly when no node binary is available.
        if !which("node") {
            eprintln!("skipping probe-failure runtime test: no node binary found");
            return;
        }

        let dir = tempfile::TempDir::new().unwrap();
        let ext_path = dir.path().join("oo_ext.mjs");
        let runner_path = dir.path().join("runner.mjs");

        // The extension source is written to be node-compatible (no type
        // annotations that plain node cannot parse), so the stripping only
        // needs to remove the `import type { … } from "…";` statement.
        let stripped = OO_TS_EXTENSION.replace(
            "import type {\n  BashToolCallEvent,\n  ExtensionAPI,\n  ToolCallEvent,\n} from \"@earendil-works/pi-coding-agent\";\n\n",
            "",
        );

        std::fs::write(&ext_path, &stripped).unwrap();

        // The runner stubs `pi` (on + exec always rejecting, simulating a
        // missing `oo` binary), imports the extension, fires a bash
        // tool_call event, and prints a JSON verdict.
        let runner = r#"
import { readFileSync } from "node:fs";

const calls = [];
const handlerRegistry = {};
const pi = {
  on(name, cb) { handlerRegistry[name] = cb; },
  exec: async (bin, args) => {
    calls.push({ bin, args });
    throw new Error("command not found: " + bin);
  },
};

const mod = await import("./oo_ext.mjs");
await mod.default(pi);

// Wait a tick for the probe's rejection to settle.
await new Promise((r) => setTimeout(r, 50));

const event = { toolName: "bash", input: { command: "ls" } };
const result = await handlerRegistry["tool_call"](event, { signal: undefined });

console.log(JSON.stringify({
  handlerReturned: result === undefined,
  rewriteCalled: calls.some((c) => c.bin === "oo" && c.args[0] === "rewrite"),
  totalExecCalls: calls.length,
}));
"#;
        std::fs::write(&runner_path, runner).unwrap();

        let out = std::process::Command::new("node")
            .arg(&runner_path)
            .current_dir(dir.path())
            .output()
            .expect("failed to spawn node");

        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "harness failed: stdout={stdout} stderr={stderr}"
        );

        let verdict: serde_json::Value = serde_json::from_str(stdout.trim())
            .expect("harness must print a JSON verdict: stdout={stdout} stderr={stderr}");

        // The handler must return undefined (fail-open) and must NOT have
        // called pi.exec for a rewrite — the failed probe disabled the
        // handler via `ooAvailable = false`.
        assert_eq!(
            verdict["handlerReturned"], true,
            "handler must return undefined when the probe failed"
        );
        assert_eq!(
            verdict["rewriteCalled"], false,
            "no rewrite pi.exec call should be made when ooAvailable is false"
        );
    }

    // -----------------------------------------------------------------------
    // target_dir resolution
    // -----------------------------------------------------------------------

    #[test]
    fn target_dir_project_under_git_root() {
        let _guard = env_guard();
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let sub = dir.path().join("a").join("b");
        fs::create_dir_all(&sub).unwrap();
        assert_eq!(
            target_dir(&sub, false).unwrap(),
            dir.path().join(".pi").join("extensions")
        );
    }

    #[test]
    fn target_dir_project_falls_back_to_cwd_outside_repo() {
        let _guard = env_guard();
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(
            target_dir(dir.path(), false).unwrap(),
            dir.path().join(".pi").join("extensions")
        );
    }

    #[test]
    fn target_dir_global_uses_home_never_git_root() {
        let home = tempfile::TempDir::new().unwrap();
        let repo = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(repo.path().join(".git")).unwrap();
        with_home(home.path(), |_| {
            assert_eq!(
                target_dir(repo.path(), true).unwrap(),
                home.path().join(".pi").join("agent").join("extensions")
            );
        });
    }

    #[test]
    fn target_dir_global_honors_override_env() {
        // OO_PI_EXTENSIONS_DIR IS the extensions directory itself — the file
        // lands at `$OO_PI_EXTENSIONS_DIR/oo.ts`, no `.pi/agent/extensions`
        // suffix appended.
        let override_dir = tempfile::TempDir::new().unwrap();
        let other_home = tempfile::TempDir::new().unwrap();
        let _guard = env_guard();
        unsafe {
            std::env::set_var("OO_PI_EXTENSIONS_DIR", override_dir.path());
            std::env::set_var("HOME", other_home.path());
        }
        assert_eq!(
            target_dir(Path::new("/tmp"), true).unwrap(),
            override_dir.path()
        );
        unsafe {
            std::env::remove_var("OO_PI_EXTENSIONS_DIR");
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn target_dir_global_empty_override_is_error() {
        let _guard = env_guard();
        unsafe { std::env::set_var("OO_PI_EXTENSIONS_DIR", "") };
        let err = target_dir(Path::new("/tmp"), true).unwrap_err();
        assert!(
            format!("{err:?}").contains("OO_PI_EXTENSIONS_DIR"),
            "error must name the offending variable: {err:?}"
        );
        unsafe { std::env::remove_var("OO_PI_EXTENSIONS_DIR") };
    }
    #[test]
    fn target_dir_global_without_home_or_override_is_error() {
        let _guard = env_guard();
        unsafe {
            std::env::remove_var("OO_PI_EXTENSIONS_DIR");
            std::env::remove_var("HOME");
        }
        let err = target_dir(Path::new("/tmp"), true).unwrap_err();
        assert!(
            format!("{err:?}").contains("HOME"),
            "error must name HOME: {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // run — install, idempotency, no-overwrite
    // -----------------------------------------------------------------------

    #[test]
    fn run_creates_extension_under_project_dir() {
        let _guard = env_guard();
        let dir = tempfile::TempDir::new().unwrap();
        run(dir.path(), false).expect("install must succeed");
        let ext = dir.path().join(".pi").join("extensions").join("oo.ts");
        let content = fs::read_to_string(&ext).expect("oo.ts must be created");
        assert_eq!(
            content, OO_TS_EXTENSION,
            "written file must be byte-identical"
        );
    }

    #[test]
    fn run_is_noop_for_identical_file() {
        let _guard = env_guard();
        let dir = tempfile::TempDir::new().unwrap();
        run(dir.path(), false).unwrap();
        let before =
            fs::read_to_string(dir.path().join(".pi").join("extensions").join("oo.ts")).unwrap();
        run(dir.path(), false).expect("second run with identical file must succeed");
        let after =
            fs::read_to_string(dir.path().join(".pi").join("extensions").join("oo.ts")).unwrap();
        assert_eq!(after, before, "identical file must be a no-op");
    }

    #[test]
    fn run_does_not_overwrite_different_file() {
        let _guard = env_guard();
        let dir = tempfile::TempDir::new().unwrap();
        let ext_dir = dir.path().join(".pi").join("extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let ext = ext_dir.join("oo.ts");
        let custom = "// user-edited extension\nexport default function () {}\n";
        fs::write(&ext, custom).unwrap();

        run(dir.path(), false).expect("existing different file must still exit Ok");

        assert_eq!(
            fs::read_to_string(&ext).unwrap(),
            custom,
            "existing different file must not be overwritten"
        );
    }

    #[test]
    fn run_global_writes_under_home_override() {
        let home = tempfile::TempDir::new().unwrap();
        let cwd = tempfile::TempDir::new().unwrap();
        with_home(home.path(), |_| {
            run(cwd.path(), true).expect("global install must succeed");
        });
        let ext = home
            .path()
            .join(".pi")
            .join("agent")
            .join("extensions")
            .join("oo.ts");
        let content = fs::read_to_string(&ext).expect("global oo.ts must be created");
        assert_eq!(content, OO_TS_EXTENSION);
    }

    #[test]
    fn run_global_writes_under_extensions_dir_override() {
        // OO_PI_EXTENSIONS_DIR is the final extensions directory: the file
        // lands at `$OO_PI_EXTENSIONS_DIR/oo.ts` directly.
        let override_dir = tempfile::TempDir::new().unwrap();
        let cwd = tempfile::TempDir::new().unwrap();
        let _guard = env_guard();
        unsafe { std::env::set_var("OO_PI_EXTENSIONS_DIR", override_dir.path()) };
        run(cwd.path(), true).expect("global install must succeed");
        unsafe { std::env::remove_var("OO_PI_EXTENSIONS_DIR") };
        let ext = override_dir.path().join("oo.ts");
        let content = fs::read_to_string(&ext).expect("global oo.ts must be created");
        assert_eq!(content, OO_TS_EXTENSION);
    }

    #[test]
    fn run_global_without_home_or_override_fails() {
        let _guard = env_guard();
        unsafe {
            std::env::remove_var("OO_PI_EXTENSIONS_DIR");
            std::env::remove_var("HOME");
        }
        let err = run(Path::new("/tmp"), true).unwrap_err();
        assert!(
            format!("{err:?}").contains("HOME"),
            "error must name HOME: {err:?}"
        );
    }
}
