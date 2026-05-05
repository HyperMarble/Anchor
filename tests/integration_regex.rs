use anchor::regex::{matches, parse, Matcher};

#[test]
fn test_literal_match() {
    let r = parse("hello").unwrap();
    assert!(matches(&r, "hello"));
    assert!(!matches(&r, "world"));
    assert!(!matches(&r, "hell"));
    assert!(!matches(&r, "helloo"));
}

#[test]
fn test_dot_star_prefix() {
    let r = parse(".*Manager").unwrap();
    assert!(matches(&r, "Manager"));
    assert!(matches(&r, "FileManager"));
    assert!(matches(&r, "ConfigManager"));
    assert!(!matches(&r, "ManagerX"));
}

#[test]
fn test_dot_star_suffix() {
    let r = parse("Config.*").unwrap();
    assert!(matches(&r, "Config"));
    assert!(matches(&r, "ConfigFile"));
    assert!(matches(&r, "ConfigManager"));
    assert!(!matches(&r, "MyConfig"));
}

#[test]
fn test_star_quantifier() {
    let r = parse("a*b").unwrap();
    assert!(matches(&r, "b"));
    assert!(matches(&r, "ab"));
    assert!(matches(&r, "aaab"));
    assert!(!matches(&r, "a"));
    assert!(!matches(&r, "ba"));
}

#[test]
fn test_plus_quantifier() {
    let r = parse("a+b").unwrap();
    assert!(matches(&r, "ab"));
    assert!(matches(&r, "aab"));
    assert!(!matches(&r, "b"));
}

#[test]
fn test_question_mark_optional() {
    let r = parse("colou?r").unwrap();
    assert!(matches(&r, "color"));
    assert!(matches(&r, "colour"));
    assert!(!matches(&r, "colouur"));
}

#[test]
fn test_alternation() {
    let r = parse("cat|dog").unwrap();
    assert!(matches(&r, "cat"));
    assert!(matches(&r, "dog"));
    assert!(!matches(&r, "fish"));
}

#[test]
fn test_negation() {
    let r = parse("~(bad)").unwrap();
    assert!(!matches(&r, "bad"));
    assert!(matches(&r, "good"));
    assert!(matches(&r, "ba"));
    assert!(matches(&r, ""));
}

#[test]
fn test_intersection() {
    // Strings starting with 'a' AND ending with 'b'
    let r = parse("a.*&.*b").unwrap();
    assert!(matches(&r, "ab"));
    assert!(matches(&r, "axxb"));
    assert!(!matches(&r, "a"));
    assert!(!matches(&r, "b"));
}

#[test]
fn test_character_class() {
    let r = parse("[A-Z][a-z]+").unwrap();
    assert!(matches(&r, "Config"));
    assert!(matches(&r, "Manager"));
    assert!(!matches(&r, "config"));
    assert!(!matches(&r, "CONFIG"));
}

#[test]
fn test_empty_pattern_matches_empty_string() {
    let r = parse("").unwrap();
    assert!(matches(&r, ""));
    assert!(!matches(&r, "a"));
}

#[test]
fn test_dot_matches_any_char() {
    let r = parse("a.c").unwrap();
    assert!(matches(&r, "abc"));
    assert!(matches(&r, "axc"));
    assert!(!matches(&r, "ac"));
    assert!(!matches(&r, "abbc"));
}

#[test]
fn test_grouping_with_parens() {
    let r = parse("(ab)+").unwrap();
    assert!(matches(&r, "ab"));
    assert!(matches(&r, "abab"));
    assert!(!matches(&r, "a"));
    assert!(!matches(&r, "b"));
}

#[test]
fn test_matcher_caches_results() {
    let pattern = parse("test.*").unwrap();
    let mut m = Matcher::new(pattern);
    assert!(m.is_match("test"));
    assert!(m.is_match("testing"));
    assert!(m.is_match("test123"));
    assert!(!m.is_match("Test"));
    assert!(!m.is_match("mytest"));
    // Call again to exercise cache path
    assert!(m.is_match("test"));
    assert!(!m.is_match("Test"));
}

#[test]
fn test_parse_invalid_pattern_errors() {
    // Unmatched bracket — should be a parse error
    let result = parse("[unclosed");
    assert!(result.is_err());
}

#[test]
fn test_camel_case_pattern() {
    let r = parse("Config.*Manager").unwrap();
    assert!(matches(&r, "ConfigManager"));
    assert!(matches(&r, "ConfigFileManager"));
    assert!(!matches(&r, "MyConfigManager"));
}

#[test]
fn test_digit_class() {
    let r = parse("[0-9]+").unwrap();
    assert!(matches(&r, "0"));
    assert!(matches(&r, "123"));
    assert!(!matches(&r, "abc"));
    assert!(!matches(&r, "12a"));
}
