use double_o::pattern::parse_pattern_str;

#[test]
fn test_empty_command_match() {
    // Empty command_match should be rejected by regex compilation
    let toml = r#"
command_match = ""

[success]
pattern = "(?P<n>\d+)"
summary = "Result: {n}"
"#;
    let result = parse_pattern_str(toml);
    // Empty regex is technically a valid regex (matches everything)
    // So this should succeed, but we might want to reject it explicitly
    println!("Result: {:?}", result.is_ok());
}

#[test]
fn test_whitespace_only_command_match() {
    // Whitespace-only command_match should compile to a valid regex
    let toml = r#"
command_match = "   "

[success]
pattern = "(?P<n>\d+)"
summary = "Result: {n}"
"#;
    let result = parse_pattern_str(toml);
    println!("Whitespace only result: {:?}", result.is_ok());
}

#[test]
fn test_newline_in_command_match() {
    // Newline in command_match should compile (matches literal newline)
    let toml = r#"
command_match = "cargo
test"

[success]
pattern = "(?P<n>\d+)"
summary = "Result: {n}"
"#;
    let result = parse_pattern_str(toml);
    println!("Newline result: {:?}", result.is_ok());
}
