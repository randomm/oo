//! Custom pattern example: load and use a user-defined pattern
//!
//! Run with: `cargo run --example custom_pattern`

use double_o::pattern::parse_pattern_str;
use double_o::{CommandOutput, classify};

fn main() {
    // Define a custom pattern in TOML format
    let toml_pattern = r#"
# Pattern for a hypothetical "myapp test" command
command_match = "^myapp test"

[success]
pattern = '(?P<passed>\d+) tests passed, (?P<failed>\d+) failed'
summary = "{passed} passed, {failed} failed"

[failure]
strategy = "tail"
lines = 20
"#;

    // Parse the pattern from TOML string
    let custom_pattern = match parse_pattern_str(toml_pattern) {
        Ok(pat) => {
            println!("✓ Successfully parsed custom pattern");
            pat
        }
        Err(e) => {
            eprintln!("✗ Failed to parse pattern: {}", e);
            return;
        }
    };

    // Create mock output for successful test run
    let success_output = CommandOutput {
        stdout: br"Running test suite...
Test 1... OK
Test 2... OK
Test 3... OK
Result: 42 tests passed, 0 failed
Total time: 2.5s"
            .to_vec(),
        stderr: Vec::new(),
        exit_code: 0,
    };

    // Classify with custom pattern
    let patterns = vec![custom_pattern];
    let classification = classify(&success_output, "myapp test --verbose", &patterns);

    println!("\n✓ Success classification:");
    match &classification {
        double_o::Classification::Success { label, summary } => {
            println!("  Label: {}", label);
            println!("  Summary: {}", summary);
        }
        _ => println!("  Unexpected classification type"),
    }

    // Test with failure output
    let failure_output = CommandOutput {
        stdout: br"Running test suite...".to_vec(),
        stderr: br"Test 1... OK
Test 2... FAILED
  Error: assertion failed at line 42
Test 3... FAILED
  Error: timeout after 5s
Test 4... FAILED
  Error: connection refused
Test 5... OK
...
[100 more lines of test output]
Test 105... FAILED
  Error: stack overflow
Result: 50 tests passed, 55 failed"
            .to_vec(),
        exit_code: 1,
    };

    let fail_classification = classify(&failure_output, "myapp test", &patterns);
    println!("\n✓ Failure classification:");
    match &fail_classification {
        double_o::Classification::Failure { label, output } => {
            println!("  Label: {}", label);
            println!("  Filtered output (last 20 lines):\n{}", output);
        }
        _ => println!("  Unexpected classification type"),
    }

    // Example with a different failure strategy
    let grep_pattern = r#"
command_match = "^myapp build"

[success]
pattern = 'Build complete'
summary = "build succeeded"

[failure]
strategy = "grep"
grep = "Error:"
"#;

    if let Ok(pat) = parse_pattern_str(grep_pattern) {
        let build_output = CommandOutput {
            stdout: Vec::new(),
            stderr: br"Compiling module A...
Compiling module B...
Warning: deprecated feature used
Compiling module C...
Error: undefined symbol 'foo'
Error: type mismatch at line 42
Build failed"
                .to_vec(),
            exit_code: 1,
        };

        let build_classification = classify(&build_output, "myapp build", &[pat]);
        println!("\n✓ Failure classification with grep strategy:");
        match &build_classification {
            double_o::Classification::Failure { label, output } => {
                println!("  Label: {}", label);
                println!("  Filtered output (only Error: lines):\n{}", output);
            }
            _ => println!("  Unexpected classification type"),
        }
    }
}
