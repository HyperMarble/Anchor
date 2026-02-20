//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

mod ast;
mod derivative;
mod parser;

pub use ast::Regex;
pub use derivative::{derivative, matches, nullable, Matcher};
pub use parser::{parse, ParseError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integration_simple() {
        let r = parse("hello").unwrap();
        assert!(matches(&r, "hello"));
        assert!(!matches(&r, "world"));
    }

    #[test]
    fn test_integration_star() {
        let r = parse("a*b").unwrap();
        assert!(matches(&r, "b"));
        assert!(matches(&r, "ab"));
        assert!(matches(&r, "aaab"));
        assert!(!matches(&r, "a"));
    }

    #[test]
    fn test_integration_intersection() {
        // Strings starting with 'a' AND ending with 'b'
        let r = parse("a.*&.*b").unwrap();
        assert!(matches(&r, "ab"));
        assert!(matches(&r, "axxb"));
        assert!(!matches(&r, "a"));
        assert!(!matches(&r, "b"));
        assert!(!matches(&r, "ba"));
    }

    #[test]
    fn test_integration_negation() {
        // Anything except "bad"
        let r = parse("~(bad)").unwrap();
        assert!(!matches(&r, "bad"));
        assert!(matches(&r, "good"));
        assert!(matches(&r, "ba"));
        assert!(matches(&r, ""));
    }

    #[test]
    fn test_camel_case_pattern() {
        // ConfigManager pattern - starts with Config, ends with Manager
        let r = parse("Config.*Manager").unwrap();
        assert!(matches(&r, "ConfigManager"));
        assert!(matches(&r, "ConfigFileManager"));
        assert!(matches(&r, "ConfigXYZManager"));
        assert!(!matches(&r, "Config"));
        assert!(!matches(&r, "Manager"));
        assert!(!matches(&r, "MyConfigManager"));
    }

    #[test]
    fn test_prefix_match() {
        let r = parse("Config.*").unwrap();
        assert!(matches(&r, "Config"));
        assert!(matches(&r, "ConfigFile"));
        assert!(matches(&r, "ConfigManager"));
        assert!(!matches(&r, "MyConfig"));
    }

    #[test]
    fn test_suffix_match() {
        let r = parse(".*Manager").unwrap();
        assert!(matches(&r, "Manager"));
        assert!(matches(&r, "FileManager"));
        assert!(matches(&r, "ConfigManager"));
        assert!(!matches(&r, "ManagerX"));
    }

    #[test]
    fn test_exact_match() {
        let r = parse("Config").unwrap();
        assert!(matches(&r, "Config"));
        assert!(!matches(&r, "ConfigFile"));
        assert!(!matches(&r, "MyConfig"));
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
    fn test_matcher_with_cache() {
        let pattern = parse("test.*").unwrap();
        let mut matcher = Matcher::new(pattern);

        assert!(matcher.is_match("test"));
        assert!(matcher.is_match("testing"));
        assert!(matcher.is_match("test123"));
        assert!(!matcher.is_match("Test"));
        assert!(!matcher.is_match("mytest"));
    }
}
