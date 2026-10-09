use clap::Parser;
use double_o::{
    Action, check_and_clear_learn_status, cmd_forget, cmd_help, cmd_hook, cmd_init, cmd_learn,
    cmd_patterns, cmd_recall, cmd_rewrite, cmd_run, learn, parse_action,
};

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
    /// init (set up project hooks; see `oo init --help`), patterns (list compression patterns),
    /// rewrite (print oo-prefixed command for agent hooks),
    /// hook (rewrite Bash commands for agent hook processors, e.g. `oo hook claude`),
    /// version (print oo version). Any other command runs as a shell command.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // Intercept _learn_bg before clap parsing (it's a hidden internal command)
    let raw_args: Vec<String> = std::env::args().collect();
    if raw_args.get(1).is_some_and(|a| a == "_learn_bg") {
        if let Some(path) = raw_args.get(2) {
            let _ = learn::run_background(path);
        }
        std::process::exit(0);
    }

    let cli = Cli::parse();

    // Show any pending learn-status message from a previous background learn.
    // Runs before command dispatch so the user sees the result on the next invocation.
    {
        let status_path = learn::learn_status_path();
        check_and_clear_learn_status(&status_path);
    }

    let exit_code = match parse_action(&cli.args) {
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
                "  init [--format claude|generic | --agent pi|claude-code [--global]]  Set up hooks for agent frameworks (see: oo init --help)"
            );
            println!("  patterns                   List output compression patterns");
            println!("  rewrite <command>          Print oo-prefixed form for agent hooks");
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
        Action::Run(args) => cmd_run(&args),
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
