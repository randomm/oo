use clap::Parser;
use double_o::{
    Action, cmd_forget, cmd_help, cmd_init, cmd_learn, cmd_recall, cmd_run, learn, parse_action,
};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "oo",
    version,
    about = "Context-efficient command runner for AI coding agents",
    long_about = "o\u{335}\u{55f}\u{33f}\u{353}\u{317}\u{33f}\u{51b}\u{31c}o\u{335}\u{359}\u{358}\u{35d}\u{31a}\n\nContext-efficient command runner for AI coding agents."
)]
struct Cli {
    /// Arguments: a subcommand (recall/forget/learn/version) or a command to run
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

    let exit_code = match parse_action(&cli.args) {
        Action::Help(None) => {
            println!(
                "o\u{335}\u{55f}\u{33f}\u{353}\u{317}\u{33f}\u{51b}\u{31c}o\u{335}\u{359}\u{358}\u{35d}\u{31a}"
            );
            println!();
            println!("Usage: oo <command> [args...]");
            println!("       oo recall <query>");
            println!("       oo forget");
            println!("       oo learn <command> [args...]");
            println!("       oo help <cmd>");
            println!("       oo version");
            0
        }
        Action::Help(Some(cmd)) => cmd_help(&cmd),
        Action::Version => {
            println!(
                "o\u{335}\u{55f}\u{33f}\u{353}\u{317}\u{33f}\u{51b}\u{31c}o\u{335}\u{359}\u{358}\u{35d}\u{31a} {}",
                env!("CARGO_PKG_VERSION")
            );
            0
        }
        Action::Run(args) => cmd_run(&args),
        Action::Recall(query) => cmd_recall(&query),
        Action::Forget => cmd_forget(),
        Action::Learn(args) => cmd_learn(&args),
        Action::Init => cmd_init(),
    };

    std::process::exit(exit_code);
}
