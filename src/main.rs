use clap::Parser;
use double_o::{
    Action, argv, cmd_forget, cmd_help, cmd_hook, cmd_init, cmd_learn, cmd_patterns, cmd_recall,
    cmd_rewrite, cmd_run, learn, learn_status, parse_action, warn_ignored_overrides,
};
use std::ffi::OsString;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "oo",
    version,
    about = "Context-efficient command runner for AI coding agents",
    long_about = "oo\n\nContext-efficient command runner for AI coding agents."
)]
struct Cli {
    /// A subcommand, or a command to run. Subcommands:
    /// recall (recall indexed output), forget (clear indexed output for this project),
    /// learn (auto-generate a compression pattern), help (cheat.sh sheet),
    /// init (set up agent hooks: --format claude|generic, --agent pi|claude-code [--global]; see oo init --help), patterns (list compression patterns),
    /// rewrite (print oo-prefixed command for agent hooks),
    /// hook (rewrite Bash commands for agent hook processors, e.g. `oo hook claude`),
    /// version (print oo version). Any other command runs as a shell command.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<OsString>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // Rust ignores SIGPIPE by default, so a write to a closed pipe (`oo patterns | head -1`)
    // returns EPIPE and `println!` panics. Restoring the default makes the process exit
    // quietly like other Unix tools. Must run before any output. This does not reach
    // std TcpStream writes do not raise SIGPIPE: MSG_NOSIGNAL on Linux (library/std/src/sys/net/connection/socket/mod.rs:435),
    // SO_NOSIGPIPE on Apple (socket/unix.rs:101-104), so a reset API connection in `_learn_bg` surfaces as an error.
    #[cfg(unix)]
    // SAFETY: signal(2) with SIG_DFL only changes the disposition for this process.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    // Before any dispatch, so the warning appears once on every code path,
    // including the background learn child.
    warn_ignored_overrides();

    // Intercept _learn_bg before clap parsing (it's a hidden internal command).
    // D5: read argv with args_os and treat payload path and directory as Path,
    // so a non-UTF-8 TMPDIR works fully.
    let raw_args: Vec<OsString> = std::env::args_os().collect();
    if raw_args.get(1).is_some_and(|a| a == "_learn_bg") {
        let status = learn_status::learn_status_path();
        match (raw_args.get(2), raw_args.get(3)) {
            (Some(path), Some(dir)) => {
                let _ = learn::run_background(
                    std::path::Path::new(path),
                    std::path::Path::new(dir),
                    &status,
                );
            }
            // Older parents pass only the path: report it, and leave the file alone
            // because argv is untrusted and the directory is what makes it a payload.
            (Some(_), None) => {
                let _ = learn_status::write_learn_status_failure(
                    &status,
                    "learn",
                    "_learn_bg needs a payload directory argument",
                );
            }
            _ => {}
        }
        std::process::exit(0);
    }

    let cli = Cli::parse();
    let args = cli.args;

    // D3: a non-UTF-8 first word that is not a reserved name is simply the
    // command to run. A non-UTF-8 argument given to an oo-owned option or to
    // a reserved subcommand's own text argument is REJECTED with exit code 2
    // and the stderr line `oo: argument is not valid UTF-8`.
    //
    // `oo learn` is special-cased: only the `--hint` value is oo-owned, while
    // all wrapped-command arguments pass through as raw bytes (D2).
    let first_is_reserved = args
        .first()
        .and_then(|a| argv::utf8_or(a).ok())
        .map(|name| argv::RESERVED_SUBCOMMANDS.contains(&name.as_str()))
        .unwrap_or(false);

    if first_is_reserved {
        let first_is_learn =
            args.first().and_then(|a| argv::utf8_or(a).ok()) == Some("learn".to_string());
        if first_is_learn {
            // Reject only non-UTF-8 values following `--hint` (D3: the hint
            // is consumed by oo and must be valid UTF-8).
            let mut i = 1;
            while i < args.len() {
                if args[i] == "--hint" && i + 1 < args.len() {
                    if argv::utf8_or(&args[i + 1]).is_err() {
                        eprintln!("{}", argv::NON_UTF8_ARG_ERR);
                        std::process::exit(2);
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else {
            // Every remaining argument is oo-owned and must be valid UTF-8.
            for arg in &args[1..] {
                if argv::utf8_or(arg).is_err() {
                    eprintln!("{}", argv::NON_UTF8_ARG_ERR);
                    std::process::exit(2);
                }
            }
        }
    }

    // Show any pending learn-status message from a previous background learn.
    // Runs before command dispatch so the user sees the result on the next invocation.
    {
        let status_path = learn_status::learn_status_path();
        learn_status::check_and_clear_learn_status(&status_path);
    }

    let exit_code = match parse_action(&args) {
        Action::Help(None) => {
            println!("oo — Context-efficient command runner for AI coding agents");
            println!();
            println!("Usage: oo <command> [args...]");
            println!();
            println!("Commands:");
            println!("  recall [--full] <query>   Search indexed output");
            println!("  forget                     Clear indexed output for this project");
            println!("  learn [--hint <text>] <cmd> [args...]   Learn output compression patterns");
            println!(
                "  help [cmd]                 Show help (or cheat-sheet for cmd via cheat.sh)"
            );
            println!(
                "  init [--format claude|generic | --agent pi|claude-code [--global]]   Set up hooks for agent frameworks (see: oo init --help)"
            );
            println!("  patterns                   List output compression patterns");
            println!("  rewrite <command...>       Print oo-prefixed form for agent hooks");
            println!(
                "  hook <agent>               Agent hook processor (e.g. `oo hook claude` reads PreToolUse JSON on stdin)"
            );
            println!("  version                    Show version");
            println!();
            println!("Any other command is run in the shell, e.g. oo git log");
            0
        }
        Action::Help(Some(cmd)) => cmd_help(&cmd),
        Action::Usage(text) => {
            print!("{text}");
            0
        }
        Action::Version => {
            println!("oo {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Action::Run { os, .. } => cmd_run(&os),
        Action::Recall { query, full } => cmd_recall(&query, full),
        Action::Forget => cmd_forget(),
        Action::Learn(args, hint) => cmd_learn(&args, hint.as_deref()),
        Action::Init(mode) => cmd_init(mode),
        Action::Patterns => cmd_patterns(),
        Action::Rewrite(command) => cmd_rewrite(&command),
        Action::Hook(agent) => match agent {
            Some(agent) => cmd_hook(&agent),
            None => {
                eprintln!("oo: hook requires an agent (e.g. `oo hook claude`)");
                1
            }
        },
    };

    std::process::exit(exit_code);
}
