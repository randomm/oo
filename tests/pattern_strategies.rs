// Tests for pattern strategy extraction

use double_o::pattern::{extract_failure, extract_summary};

#[test]
fn test_success_strategy_tail() {
    let pat = double_o::pattern::SuccessPattern {
        strategy: double_o::pattern::SuccessStrategy::Tail { lines: 10 },
    };
    let lines: String = (0..50).map(|i| format!("line {i}\n")).collect();
    let result = extract_summary(&pat, &lines).unwrap();
    // tail 10 lines from 50 → lines 40..49
    assert!(result.contains("line 40"));
    assert!(result.contains("line 49"));
    assert!(!result.contains("line 0\n"));
}

#[test]
fn test_success_strategy_head() {
    let pat = double_o::pattern::SuccessPattern {
        strategy: double_o::pattern::SuccessStrategy::Head { lines: 5 },
    };
    let lines: String = (0..20).map(|i| format!("line {i}\n")).collect();
    let result = extract_summary(&pat, &lines).unwrap();
    assert_eq!(result, "line 0\nline 1\nline 2\nline 3\nline 4");
}

#[test]
fn test_success_strategy_grep() {
    let pat = double_o::pattern::SuccessPattern {
        strategy: double_o::pattern::SuccessStrategy::Grep {
            pattern: regex::Regex::new(r"passed").unwrap(),
        },
    };
    let output = "test 1 passed\ntest 2 failed\ntest 3 passed\n";
    let result = extract_summary(&pat, output).unwrap();
    assert_eq!(result, "test 1 passed\ntest 3 passed");
}

#[test]
fn test_success_strategy_regex_template() {
    let pat = double_o::pattern::SuccessPattern {
        strategy: double_o::pattern::SuccessStrategy::Regex {
            pattern: regex::Regex::new(r"(?P<a>\d+) things, (?P<b>\d+) items").unwrap(),
            summary: "{a} things and {b} items".into(),
        },
    };
    let result = extract_summary(&pat, "found 5 things, 3 items here").unwrap();
    assert_eq!(result, "5 things and 3 items");
}

#[test]
fn test_success_strategy_tail_empty_when_lines_exceeds_output() {
    let pat = double_o::pattern::SuccessPattern {
        strategy: double_o::pattern::SuccessStrategy::Tail { lines: 0 },
    };
    let output = "line1\nline2\n";
    assert!(extract_summary(&pat, output).is_none());
}

#[test]
fn test_success_strategy_head_empty_on_zero_lines() {
    let pat = double_o::pattern::SuccessPattern {
        strategy: double_o::pattern::SuccessStrategy::Head { lines: 0 },
    };
    let output = "line1\nline2\n";
    assert!(extract_summary(&pat, output).is_none());
}

#[test]
fn test_success_strategy_grep_empty_when_no_matches() {
    let pat = double_o::pattern::SuccessPattern {
        strategy: double_o::pattern::SuccessStrategy::Grep {
            pattern: regex::Regex::new(r"NOMATCH").unwrap(),
        },
    };
    let output = "line1\nline2\n";
    assert!(extract_summary(&pat, output).is_none());
}

#[test]
fn test_success_strategy_regex_none_when_no_match() {
    let pat = double_o::pattern::SuccessPattern {
        strategy: double_o::pattern::SuccessStrategy::Regex {
            pattern: regex::Regex::new(r"(?P<n>\d+) tests").unwrap(),
            summary: "{n} tests".into(),
        },
    };
    let output = "no tests here";
    assert!(extract_summary(&pat, output).is_none());
}

#[test]
fn test_failure_strategy_head() {
    let strat = double_o::pattern::FailurePattern {
        strategy: double_o::pattern::FailureStrategy::Head { lines: 3 },
    };
    let output = "line1\nline2\nline3\nline4\nline5\n";
    let result = extract_failure(&strat, output);
    assert_eq!(result, "line1\nline2\nline3");
}

#[test]
fn test_failure_strategy_grep() {
    let strat = double_o::pattern::FailurePattern {
        strategy: double_o::pattern::FailureStrategy::Grep {
            pattern: regex::Regex::new(r"ERROR").unwrap(),
        },
    };
    let output = "INFO ok\nERROR bad\nINFO fine\nERROR worse\n";
    let result = extract_failure(&strat, output);
    assert_eq!(result, "ERROR bad\nERROR worse");
}

#[test]
fn test_failure_strategy_between() {
    let strat = double_o::pattern::FailurePattern {
        strategy: double_o::pattern::FailureStrategy::Between {
            start: "FAILURES".into(),
            end: "summary".into(),
        },
    };
    let output = "stuff\nFAILURES\nerror 1\nerror 2\nshort test summary\nmore\n";
    let result = extract_failure(&strat, output);
    assert_eq!(result, "FAILURES\nerror 1\nerror 2\nshort test summary");
}

#[test]
fn test_summary_template_formatting() {
    let pat = double_o::pattern::SuccessPattern {
        strategy: double_o::pattern::SuccessStrategy::Regex {
            pattern: regex::Regex::new(r"(?P<a>\d+) things, (?P<b>\d+) items").unwrap(),
            summary: "{a} things and {b} items".into(),
        },
    };
    let result = extract_summary(&pat, "found 5 things, 3 items here").unwrap();
    assert_eq!(result, "5 things and 3 items");
}
