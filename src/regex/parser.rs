//
//  parser.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::sync::Arc;

use super::ast::Regex;

/// Parse a regex pattern string into AST.
pub fn parse(pattern: &str) -> Result<Arc<Regex>, ParseError> {
    let mut parser = Parser::new(pattern);
    parser.parse_expr()
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse error at {}: {}", self.position, self.message)
    }
}

impl std::error::Error for ParseError {}

struct Parser<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    #[allow(dead_code)]
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.char_indices().peekable(),
            input,
            pos: 0,
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    fn next(&mut self) -> Option<char> {
        self.chars.next().map(|(i, c)| {
            self.pos = i + c.len_utf8();
            c
        })
    }

    fn error(&self, msg: &str) -> ParseError {
        ParseError {
            message: msg.to_string(),
            position: self.pos,
        }
    }

    /// Parse full expression (handles |)
    fn parse_expr(&mut self) -> Result<Arc<Regex>, ParseError> {
        let mut left = self.parse_intersect()?;

        while self.peek() == Some('|') {
            self.next(); // consume '|'
            let right = self.parse_intersect()?;
            left = Regex::union(left, right);
        }

        Ok(left)
    }

    /// Parse intersection (handles &)
    fn parse_intersect(&mut self) -> Result<Arc<Regex>, ParseError> {
        let mut left = self.parse_concat()?;

        while self.peek() == Some('&') {
            self.next(); // consume '&'
            let right = self.parse_concat()?;
            left = Regex::intersect(left, right);
        }

        Ok(left)
    }

    /// Parse concatenation
    fn parse_concat(&mut self) -> Result<Arc<Regex>, ParseError> {
        let mut parts: Vec<Arc<Regex>> = Vec::new();

        while let Some(c) = self.peek() {
            if c == '|' || c == '&' || c == ')' {
                break;
            }
            parts.push(self.parse_quantified()?);
        }

        if parts.is_empty() {
            return Ok(Arc::new(Regex::Epsilon));
        }

        let mut result = parts.remove(0);
        for part in parts {
            result = Regex::concat(result, part);
        }

        Ok(result)
    }

    /// Parse quantified atom (*, +, ?)
    fn parse_quantified(&mut self) -> Result<Arc<Regex>, ParseError> {
        let atom = self.parse_atom()?;

        match self.peek() {
            Some('*') => {
                self.next();
                Ok(Regex::star(atom))
            }
            Some('+') => {
                self.next();
                Ok(Regex::plus(atom))
            }
            Some('?') => {
                self.next();
                Ok(Regex::optional(atom))
            }
            _ => Ok(atom),
        }
    }

    /// Parse atomic expression
    fn parse_atom(&mut self) -> Result<Arc<Regex>, ParseError> {
        match self.peek() {
            None => Ok(Arc::new(Regex::Epsilon)),
            Some('(') => {
                self.next(); // consume '('
                let inner = self.parse_expr()?;
                if self.peek() != Some(')') {
                    return Err(self.error("Expected ')'"));
                }
                self.next(); // consume ')'
                Ok(inner)
            }
            Some('~') => {
                self.next(); // consume '~'
                let inner = self.parse_atom()?;
                Ok(Regex::negate(inner))
            }
            Some('.') => {
                self.next();
                Ok(Arc::new(Regex::Any))
            }
            Some('^') => {
                // Start anchor - for simplicity, treat as epsilon at start
                self.next();
                Ok(Arc::new(Regex::Epsilon))
            }
            Some('$') => {
                // End anchor - for simplicity, treat as epsilon at end
                self.next();
                Ok(Arc::new(Regex::Epsilon))
            }
            Some('[') => self.parse_class(),
            Some('\\') => {
                self.next(); // consume '\'
                match self.next() {
                    Some(c) => Ok(Regex::lit(c)),
                    None => Err(self.error("Expected character after \\")),
                }
            }
            Some(c) if c == '|' || c == '&' || c == ')' || c == '*' || c == '+' || c == '?' => {
                Ok(Arc::new(Regex::Epsilon))
            }
            Some(c) => {
                self.next();
                Ok(Regex::lit(c))
            }
        }
    }

    /// Parse character class [...]
    fn parse_class(&mut self) -> Result<Arc<Regex>, ParseError> {
        self.next(); // consume '['

        let negated = if self.peek() == Some('^') {
            self.next();
            true
        } else {
            false
        };

        let mut chars = std::collections::HashSet::new();

        while let Some(c) = self.peek() {
            if c == ']' {
                break;
            }
            self.next();

            // Check for range a-z
            if self.peek() == Some('-') {
                self.next(); // consume '-'
                if let Some(end) = self.next() {
                    if end != ']' {
                        for ch in c..=end {
                            chars.insert(ch);
                        }
                        continue;
                    }
                }
            }

            chars.insert(c);
        }

        if self.peek() != Some(']') {
            return Err(self.error("Expected ']'"));
        }
        self.next(); // consume ']'

        let class = Arc::new(Regex::Class(chars));
        if negated {
            Ok(Regex::negate(class))
        } else {
            Ok(class)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regex::derivative::matches;

    #[test]
    fn test_parse_literal() {
        let r = parse("abc").unwrap();
        assert!(matches(&r, "abc"));
        assert!(!matches(&r, "ab"));
    }

    #[test]
    fn test_parse_star() {
        let r = parse("a*").unwrap();
        assert!(matches(&r, ""));
        assert!(matches(&r, "a"));
        assert!(matches(&r, "aaa"));
    }

    #[test]
    fn test_parse_union() {
        let r = parse("a|b").unwrap();
        assert!(matches(&r, "a"));
        assert!(matches(&r, "b"));
        assert!(!matches(&r, "c"));
    }

    #[test]
    fn test_parse_intersection() {
        // a.* & .*b = strings starting with a and ending with b
        let r = parse("a.*&.*b").unwrap();
        assert!(matches(&r, "ab"));
        assert!(matches(&r, "axxxb"));
        assert!(!matches(&r, "a"));
        assert!(!matches(&r, "b"));
    }

    #[test]
    fn test_parse_negation() {
        let r = parse("~a").unwrap();
        assert!(!matches(&r, "a"));
        assert!(matches(&r, "b"));
        assert!(matches(&r, ""));
    }

    #[test]
    fn test_parse_group() {
        let r = parse("(ab)+").unwrap();
        assert!(matches(&r, "ab"));
        assert!(matches(&r, "abab"));
        assert!(!matches(&r, "a"));
        assert!(!matches(&r, ""));
    }

    #[test]
    fn test_parse_any() {
        let r = parse("a.b").unwrap();
        assert!(matches(&r, "aab"));
        assert!(matches(&r, "axb"));
        assert!(!matches(&r, "ab"));
    }

    #[test]
    fn test_parse_class() {
        let r = parse("[a-c]+").unwrap();
        assert!(matches(&r, "a"));
        assert!(matches(&r, "abc"));
        assert!(matches(&r, "cba"));
        assert!(!matches(&r, "d"));
    }
}
