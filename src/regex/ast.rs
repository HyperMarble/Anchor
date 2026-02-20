//
//  ast.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Regex AST - immutable, hashable, algebraic.
#[derive(Clone, Debug)]
pub enum Regex {
    /// Matches nothing (empty set)
    Empty,
    /// Matches empty string only (epsilon)
    Epsilon,
    /// Matches a single character
    Literal(char),
    /// Matches any single character (.)
    Any,
    /// Matches a character class [a-z]
    Class(HashSet<char>),
    /// Concatenation: R1 followed by R2
    Concat(Arc<Regex>, Arc<Regex>),
    /// Union: R1 | R2
    Union(Arc<Regex>, Arc<Regex>),
    /// Intersection: R1 & R2 (boolean algebra)
    Intersect(Arc<Regex>, Arc<Regex>),
    /// Negation: ~R (boolean algebra)
    Negate(Arc<Regex>),
    /// Kleene star: R*
    Star(Arc<Regex>),
}

impl Regex {
    /// Create a literal regex
    pub fn lit(c: char) -> Arc<Regex> {
        Arc::new(Regex::Literal(c))
    }

    /// Create a string literal (concatenation of chars)
    pub fn string(s: &str) -> Arc<Regex> {
        if s.is_empty() {
            return Arc::new(Regex::Epsilon);
        }
        let mut chars = s.chars();
        let first = Arc::new(Regex::Literal(chars.next().unwrap()));
        chars.fold(first, |acc, c| {
            Arc::new(Regex::Concat(acc, Arc::new(Regex::Literal(c))))
        })
    }

    /// Concatenation
    pub fn concat(r1: Arc<Regex>, r2: Arc<Regex>) -> Arc<Regex> {
        // Normalize: Empty annihilates
        match (r1.as_ref(), r2.as_ref()) {
            (Regex::Empty, _) | (_, Regex::Empty) => Arc::new(Regex::Empty),
            (Regex::Epsilon, _) => r2,
            (_, Regex::Epsilon) => r1,
            _ => Arc::new(Regex::Concat(r1, r2)),
        }
    }

    /// Union (alternation)
    pub fn union(r1: Arc<Regex>, r2: Arc<Regex>) -> Arc<Regex> {
        // Normalize: Empty is identity
        match (r1.as_ref(), r2.as_ref()) {
            (Regex::Empty, _) => r2,
            (_, Regex::Empty) => r1,
            _ => Arc::new(Regex::Union(r1, r2)),
        }
    }

    /// Intersection (boolean and)
    pub fn intersect(r1: Arc<Regex>, r2: Arc<Regex>) -> Arc<Regex> {
        // Normalize: Empty annihilates
        match (r1.as_ref(), r2.as_ref()) {
            (Regex::Empty, _) | (_, Regex::Empty) => Arc::new(Regex::Empty),
            _ => Arc::new(Regex::Intersect(r1, r2)),
        }
    }

    /// Negation (complement)
    pub fn negate(r: Arc<Regex>) -> Arc<Regex> {
        // Normalize: double negation
        match r.as_ref() {
            Regex::Negate(inner) => inner.clone(),
            _ => Arc::new(Regex::Negate(r)),
        }
    }

    /// Kleene star
    pub fn star(r: Arc<Regex>) -> Arc<Regex> {
        // Normalize
        match r.as_ref() {
            Regex::Empty | Regex::Epsilon => Arc::new(Regex::Epsilon),
            Regex::Star(_) => r, // R** = R*
            _ => Arc::new(Regex::Star(r)),
        }
    }

    /// Optional: R?
    pub fn optional(r: Arc<Regex>) -> Arc<Regex> {
        Regex::union(Arc::new(Regex::Epsilon), r)
    }

    /// One or more: R+
    pub fn plus(r: Arc<Regex>) -> Arc<Regex> {
        Regex::concat(r.clone(), Regex::star(r))
    }
}

impl PartialEq for Regex {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Regex::Empty, Regex::Empty) => true,
            (Regex::Epsilon, Regex::Epsilon) => true,
            (Regex::Literal(a), Regex::Literal(b)) => a == b,
            (Regex::Any, Regex::Any) => true,
            (Regex::Class(a), Regex::Class(b)) => a == b,
            (Regex::Concat(a1, a2), Regex::Concat(b1, b2)) => a1 == b1 && a2 == b2,
            (Regex::Union(a1, a2), Regex::Union(b1, b2)) => a1 == b1 && a2 == b2,
            (Regex::Intersect(a1, a2), Regex::Intersect(b1, b2)) => a1 == b1 && a2 == b2,
            (Regex::Negate(a), Regex::Negate(b)) => a == b,
            (Regex::Star(a), Regex::Star(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Regex {}

impl Hash for Regex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Regex::Literal(c) => c.hash(state),
            Regex::Class(set) => {
                for c in set.iter() {
                    c.hash(state);
                }
            }
            Regex::Concat(r1, r2) | Regex::Union(r1, r2) | Regex::Intersect(r1, r2) => {
                r1.hash(state);
                r2.hash(state);
            }
            Regex::Negate(r) | Regex::Star(r) => r.hash(state),
            _ => {}
        }
    }
}
