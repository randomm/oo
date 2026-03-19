//! Basic example: classify command output
//!
//! Run with: `cargo run --example basic`

use double_o::{CommandOutput, classify};

fn main() {
    // Create a mock command output (as if we ran "pytest tests/")
    let output = CommandOutput {
        stdout: b"collected 47 items\n\
                   .................\n\
                   47 passed in 3.2s\n"
            .to_vec(),
        stderr: Vec::new(),
        exit_code: 0,
    };

    // Load built-in patterns
    let patterns = double_o::builtins();

    // Classify the output
    let classification = classify(&output, "pytest tests/", patterns);

    println!("Classification result:");
    match &classification {
        double_o::Classification::Success { label, summary } => {
            println!("  Type: Success");
            println!("  Label: {}", label);
            println!("  Summary: {}", summary);
        }
        double_o::Classification::Failure { label, output } => {
            println!("  Type: Failure");
            println!("  Label: {}", label);
            println!("  Output: {}", output);
        }
        double_o::Classification::Passthrough { output } => {
            println!("  Type: Passthrough");
            println!("  Output: {}", output);
        }
        double_o::Classification::Large { label, size, .. } => {
            println!("  Type: Large (indexed)");
            println!("  Label: {}", label);
            println!("  Size: {} bytes", size);
        }
    }

    // Example with failing command
    let fail_output = CommandOutput {
        stdout: Vec::new(),
        stderr: b"error: tests failed\n\
                   test result: FAILED. 10 passed; 5 failed; 0 ignored\n"
            .to_vec(),
        exit_code: 1,
    };

    let fail_classification = classify(&fail_output, "cargo test", patterns);
    println!("\nFailed command classification:");
    match &fail_classification {
        double_o::Classification::Failure { label, output } => {
            println!("  Type: Failure");
            println!("  Label: {}", label);
            println!("  Filtered output:\n{}", output);
        }
        _ => println!("  Unexpected classification type"),
    }
}
