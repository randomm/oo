use double_o::pattern::parse_pattern_str;

#[test]
fn test_normal_command_match() {
    // Normal regex patterns should parse and compile fine
    let toml = r#"
command_match = "^myapp"

[success]
pattern = "[0-9]+"
summary = "Result"
"#;
    let result = parse_pattern_str(toml);
    assert!(result.is_ok(), "Normal regex should parse and compile");
}

#[test]
fn test_simple_command_match() {
    // Simple word pattern should work
    let toml = r#"
command_match = "cargo test"

[success]
pattern = "[0-9]+ passed"
summary = "passed"
"#;
    let result = parse_pattern_str(toml);
    assert!(result.is_ok(), "Simple word pattern should parse");
}
