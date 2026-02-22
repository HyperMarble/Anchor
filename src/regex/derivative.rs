//
//  derivative.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::collections::HashMap;
use std::sync::Arc;

use super::ast::Regex;

/// Check if regex accepts the empty string.
pub fn nullable(r: &Regex) -> bool {
    match r {
        Regex::Empty => false,
        Regex::Epsilon => true,
        Regex::Literal(_) => false,
        Regex::Any => false,
        Regex::Class(_) => false,
        Regex::Concat(r1, r2) => nullable(r1) && nullable(r2),
        Regex::Union(r1, r2) => nullable(r1) || nullable(r2),
        Regex::Intersect(r1, r2) => nullable(r1) && nullable(r2),
        Regex::Negate(r) => !nullable(r),
        Regex::Star(_) => true,
    }
}

/// Compute the derivative of regex R with respect to character c.
///
/// D_c(R) is the regex that matches string s iff R matches cs.
pub fn derivative(r: &Regex, c: char) -> Arc<Regex> {
    match r {
        Regex::Empty => Arc::new(Regex::Empty),
        Regex::Epsilon => Arc::new(Regex::Empty),
        Regex::Literal(l) => {
            if *l == c {
                Arc::new(Regex::Epsilon)
            } else {
                Arc::new(Regex::Empty)
            }
        }
        Regex::Any => Arc::new(Regex::Epsilon),
        Regex::Class(set) => {
            if set.contains(&c) {
                Arc::new(Regex::Epsilon)
            } else {
                Arc::new(Regex::Empty)
            }
        }
        Regex::Concat(r1, r2) => {
            // D_c(R1 R2) = D_c(R1) R2 | ν(R1) D_c(R2)
            let d1 = Regex::concat(derivative(r1, c), r2.clone());
            if nullable(r1) {
                Regex::union(d1, derivative(r2, c))
            } else {
                d1
            }
        }
        Regex::Union(r1, r2) => {
            // D_c(R1 | R2) = D_c(R1) | D_c(R2)
            Regex::union(derivative(r1, c), derivative(r2, c))
        }
        Regex::Intersect(r1, r2) => {
            // D_c(R1 & R2) = D_c(R1) & D_c(R2)
            Regex::intersect(derivative(r1, c), derivative(r2, c))
        }
        Regex::Negate(inner) => {
            // D_c(~R) = ~D_c(R)
            Regex::negate(derivative(inner, c))
        }
        Regex::Star(inner) => {
            // D_c(R*) = D_c(R) R*
            Regex::concat(derivative(inner, c), Arc::new(Regex::Star(inner.clone())))
        }
    }
}

/// Match a string against a regex using derivatives.
///
/// Time complexity: O(n * |R|) where n is string length.
/// No backtracking. No exponential blowup.
pub fn matches(r: &Regex, s: &str) -> bool {
    let mut current = Arc::new(r.clone());

    for c in s.chars() {
        current = derivative(&current, c);
    }

    nullable(&current)
}

/// Matcher with memoization for repeated queries.
pub struct Matcher {
    regex: Arc<Regex>,
    cache: HashMap<(Arc<Regex>, char), Arc<Regex>>,
}

impl Matcher {
    pub fn new(regex: Arc<Regex>) -> Self {
        Self {
            regex,
            cache: HashMap::new(),
        }
    }

    /// Check if string matches the regex.
    pub fn is_match(&mut self, s: &str) -> bool {
        let mut current = self.regex.clone();

        for c in s.chars() {
            let key = (current.clone(), c);
            current = self
                .cache
                .entry(key)
                .or_insert_with_key(|(r, ch)| derivative(r, *ch))
                .clone();
        }

        nullable(&current)
    }

    /// Check if string starts with a match (prefix match).
    pub fn is_prefix_match(&mut self, s: &str) -> bool {
        let mut current = self.regex.clone();

        // Check if current state is nullable at any point
        if nullable(&current) {
            return true;
        }

        for c in s.chars() {
            let key = (current.clone(), c);
            current = self
                .cache
                .entry(key)
                .or_insert_with_key(|(r, ch)| derivative(r, *ch))
                .clone();

            if nullable(&current) {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_match() {
        let r = Regex::lit('a');
        assert!(matches(&r, "a"));
        assert!(!matches(&r, "b"));
        assert!(!matches(&r, ""));
        assert!(!matches(&r, "aa"));
    }

    #[test]
    fn test_concat() {
        let r = Regex::concat(Regex::lit('a'), Regex::lit('b'));
        assert!(matches(&r, "ab"));
        assert!(!matches(&r, "a"));
        assert!(!matches(&r, "b"));
        assert!(!matches(&r, "ba"));
    }

    #[test]
    fn test_union() {
        let r = Regex::union(Regex::lit('a'), Regex::lit('b'));
        assert!(matches(&r, "a"));
        assert!(matches(&r, "b"));
        assert!(!matches(&r, "c"));
        assert!(!matches(&r, "ab"));
    }

    #[test]
    fn test_star() {
        let r = Regex::star(Regex::lit('a'));
        assert!(matches(&r, ""));
        assert!(matches(&r, "a"));
        assert!(matches(&r, "aa"));
        assert!(matches(&r, "aaa"));
        assert!(!matches(&r, "b"));
        assert!(!matches(&r, "ab"));
    }

    #[test]
    fn test_string() {
        let r = Regex::string("hello");
        assert!(matches(&r, "hello"));
        assert!(!matches(&r, "hell"));
        assert!(!matches(&r, "helloo"));
    }

    #[test]
    fn test_intersection() {
        // a* & a+ = a+ (one or more a's)
        let a_star = Regex::star(Regex::lit('a'));
        let a_plus = Regex::plus(Regex::lit('a'));
        let r = Regex::intersect(a_star, a_plus);

        assert!(!matches(&r, "")); // a+ doesn't match empty
        assert!(matches(&r, "a"));
        assert!(matches(&r, "aa"));
    }

    #[test]
    fn test_negation() {
        // ~a = anything except "a"
        let r = Regex::negate(Regex::lit('a'));
        assert!(!matches(&r, "a"));
        assert!(matches(&r, "b"));
        assert!(matches(&r, ""));
        assert!(matches(&r, "aa"));
    }

    #[test]
    fn test_complex() {
        // ^Config.*Manager$
        // Config followed by any chars, ending with Manager
        let config = Regex::string("Config");
        let any_star = Regex::star(Arc::new(Regex::Any));
        let manager = Regex::string("Manager");
        let r = Regex::concat(Regex::concat(config, any_star), manager);

        assert!(matches(&r, "ConfigManager"));
        assert!(matches(&r, "ConfigFileManager"));
        assert!(matches(&r, "ConfigXYZManager"));
        assert!(!matches(&r, "Config"));
        assert!(!matches(&r, "Manager"));
        assert!(!matches(&r, "MyConfigManager")); // doesn't start with Config
    }
}
