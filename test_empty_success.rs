use double_o::pattern::parse_pattern_str;

fn main() {
    let toml = r#"
command_match = "^myapp"

[success]
"#;
    match parse_pattern_str(toml) {
        Ok(pat) => println!("Success: pattern parsed, success = {:?}",
            pat.success.map(|s| format!("{:?}", s.strategy))),
        Err(e) => println!("Error: {:?}", e),
    }
}
