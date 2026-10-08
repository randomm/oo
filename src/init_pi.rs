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
/// - at load, probes `oo --version` via `pi.exec` with a short timeout and
///   silently disables itself (status note only) if `oo` is missing or errors;
/// - the `tool_call` handler acts only on `toolName === "bash"`, ignores empty
///   commands and commands already starting with `oo `, and returns
///   immediately when `event.parentToolCallId` is set (nested codemode calls
///   are left raw — the guards short-circuit BEFORE any `pi.exec` call);
/// - honours the `OO_DISABLE=1` environment opt-out;
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

// Local reimplementation of the package's `isToolCallEventType("bash", event)`
// type guard: that helper is a value export, so importing it would pull in the
// whole pi barrel at extension load. The type-only imports below are erased at
// compile time and carry none of that cost.
function isBashToolCallEvent(
  event: ToolCallEvent
): event is BashToolCallEvent {
  return event.toolName === "bash";
}

// Calls `oo rewrite`; returns the rewritten command or null (pass through).
async function rewriteCommand(
  pi: ExtensionAPI,
  cmd: string,
  signal?: AbortSignal
): Promise<string | null> {
  const result = await pi.exec("oo", ["rewrite", cmd], {
    timeout: REWRITE_TIMEOUT_MS,
    signal,
  });
  if (result.killed) return null;
  if (result.code !== 0) return null;
  const rewritten = result.stdout.trim();
  return rewritten === "" ? null : rewritten;
}

type StatusContext = {
  ui?: {
    setStatus?: (key: string, text: string) => void;
  };
};

// Retain the session_start context so a status note can be applied as soon as
// the load-time probe resolves, without blocking extension load itself.
function registerOoUnavailableNotice(pi: ExtensionAPI): (reason: string) => void {
  let reason: string | undefined;
  let sessionContext: StatusContext | undefined;

  const applyStatus = () => {
    if (!reason || !sessionContext) return;
    try {
      sessionContext.ui?.setStatus?.("oo", `oo disabled: ${reason}`);
    } catch {
      // Status reporting must never affect fail-open behavior.
    }
  };

  try {
    pi.on("session_start", (_event: unknown, ctx: unknown) => {
      sessionContext = ctx as StatusContext;
      applyStatus();
    });
  } catch {
    // Runtimes without a session_start event: nothing to report.
    return (_reason: string) => {};
  }

  return (nextReason: string) => {
    reason = nextReason;
    applyStatus();
  };
}

export default async function (pi: ExtensionAPI) {
  const reportOoUnavailable = registerOoUnavailableNotice(pi);

  // Register the handler first (same ordering as rtk's extension): a slow
  // version probe must never leave a gap where bash tool calls are missed.
  pi.on("tool_call", async (event, ctx) => {
    try {
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
  // success — `oo --version` prints "oo <version>". When the probe fails the
  // registered handler stays a pass-through no-op (oo rewrite would fail
  // anyway), so nothing is lost.
  try {
    const ver = await pi.exec("oo", ["--version"], { timeout: REWRITE_TIMEOUT_MS });
    if (ver.code !== 0) {
      reportOoUnavailable("oo binary not found in PATH");
      console.warn("[oo] oo binary not found in PATH — extension disabled");
    }
  } catch {
    reportOoUnavailable("oo version probe failed");
    console.warn("[oo] could not probe oo — extension disabled");
  }
}
"#;

/// Resolve the directory the extension file is written into.
///
/// - global: `<home>/.pi/agent/extensions` — resolved from the home/override
///   dir and NEVER consults `find_root`/git root;
/// - project: `<git-root>/.pi/extensions` — `find_root` walks up to `.git`
///   and falls back to cwd outside a repo.
fn target_dir(cwd: &Path, global: bool) -> PathBuf {
    if global {
        let home = std::env::var_os("OO_PI_EXTENSIONS_DIR")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_default();
        home.join(".pi").join("agent").join("extensions")
    } else {
        find_root(cwd).join(".pi").join("extensions")
    }
}

/// Install the pi extension for `oo init --agent pi`.
///
/// Idempotency is two-way: an existing file with identical content is a no-op
/// ("already installed", exit 0); an existing file with different content is
/// NOT overwritten — the caller is told how to proceed (still exit 0, mirroring
/// the Claude path's warn-and-skip semantics).
pub fn run(cwd: &Path, global: bool) -> Result<(), Error> {
    let dir = target_dir(cwd, global);
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

    // -----------------------------------------------------------------------
    // target_dir resolution
    // -----------------------------------------------------------------------

    #[test]
    fn target_dir_project_under_git_root() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let sub = dir.path().join("a").join("b");
        fs::create_dir_all(&sub).unwrap();
        assert_eq!(
            target_dir(&sub, false),
            dir.path().join(".pi").join("extensions")
        );
    }

    #[test]
    fn target_dir_project_falls_back_to_cwd_outside_repo() {
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(
            target_dir(dir.path(), false),
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
                target_dir(repo.path(), true),
                home.path().join(".pi").join("agent").join("extensions")
            );
        });
    }

    #[test]
    fn target_dir_global_honors_override_env() {
        let override_dir = tempfile::TempDir::new().unwrap();
        let other_home = tempfile::TempDir::new().unwrap();
        let _guard = env_guard();
        unsafe {
            std::env::set_var("OO_PI_EXTENSIONS_DIR", override_dir.path());
            std::env::set_var("HOME", other_home.path());
        }
        assert_eq!(
            target_dir(Path::new("/tmp"), true),
            override_dir
                .path()
                .join(".pi")
                .join("agent")
                .join("extensions")
        );
        unsafe { std::env::remove_var("OO_PI_EXTENSIONS_DIR") };
    }

    // -----------------------------------------------------------------------
    // run — install, idempotency, no-overwrite
    // -----------------------------------------------------------------------

    #[test]
    fn run_creates_extension_under_project_dir() {
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
}
