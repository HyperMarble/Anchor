use std::collections::HashMap;

use super::anchor::SymbolEntry;

const K1: f32 = 1.5;
const B: f32 = 0.75;

/// Tokenize an identifier or free-text query into lowercase sub-tokens.
/// Splits on whitespace, underscores, and camelCase boundaries.
/// Mirrors the split_identifier logic in the parser so query tokens align with stored features.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for word in text.split_whitespace() {
        for part in word.split('_') {
            if part.is_empty() {
                continue;
            }
            let mut current = String::new();
            for ch in part.chars() {
                if ch.is_uppercase() && !current.is_empty() {
                    tokens.push(current.to_lowercase());
                    current = String::new();
                }
                current.push(ch);
            }
            if !current.is_empty() {
                tokens.push(current.to_lowercase());
            }
        }
    }
    tokens.retain(|t| t.len() > 2);
    tokens
}

/// Corpus-level statistics for BM25 scoring.
pub struct Bm25Index {
    /// Number of documents (symbols) containing each token.
    df: HashMap<String, usize>,
    doc_count: usize,
    avg_doc_len: f32,
}

impl Bm25Index {
    /// Build index from the full symbol list. O(n) over all feature tokens.
    pub fn build(symbols: &[SymbolEntry]) -> Self {
        let doc_count = symbols.len();
        let mut df: HashMap<String, usize> = HashMap::new();
        let mut total_len = 0usize;

        for sym in symbols {
            total_len += sym.features.len();
            for token in &sym.features {
                *df.entry(token.clone()).or_insert(0) += 1;
            }
        }

        let avg_doc_len = if doc_count > 0 {
            total_len as f32 / doc_count as f32
        } else {
            1.0
        };

        Self { df, doc_count, avg_doc_len }
    }

    /// BM25 score for one symbol against a pre-tokenized query.
    ///
    /// Name tokens get a 3x definition boost: a symbol whose own name contains the
    /// query term ranks higher than one that only mentions it via path or parent context.
    pub fn score(&self, symbol: &SymbolEntry, query_tokens: &[String]) -> f32 {
        if query_tokens.is_empty() || self.doc_count == 0 {
            return 0.0;
        }

        let name_tokens = tokenize(&symbol.name);
        let doc_len = symbol.features.len() as f32;
        let norm = 1.0 - B + B * doc_len / self.avg_doc_len.max(1.0);

        query_tokens
            .iter()
            .map(|qt| {
                let in_name = name_tokens.contains(qt);
                let in_features = symbol.features.iter().any(|f| f == qt);

                if !in_name && !in_features {
                    return 0.0;
                }

                let df = *self.df.get(qt).unwrap_or(&0);
                // Term matches symbol name but never appears in any feature list (e.g. blob
                // extractors that use hardcoded features). Treat as hapax: give it a high IDF
                // so name-only matches still rank rather than scoring zero.
                let idf = if df == 0 {
                    (self.doc_count as f32 + 1.0).ln()
                } else {
                    let n = self.doc_count as f32;
                    ((n - df as f32 + 0.5) / (df as f32 + 0.5) + 1.0).ln()
                };

                // features are deduped so tf is always 1 when present
                let tf = 1.0_f32;
                let boost = if in_name { 3.0_f32 } else { 1.0_f32 };
                let bm25 = idf * tf * (K1 + 1.0) / (tf + K1 * norm);

                bm25 * boost
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sym(name: &str, features: &[&str]) -> SymbolEntry {
        SymbolEntry {
            path: "src/test.rs".into(),
            source_hash: "abc".into(),
            name: name.into(),
            kind: "Function".into(),
            line_start: 1,
            line_end: 5,
            slice_hash: "def".into(),
            features: features.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn tokenize_camel_case() {
        // "by" and "id" are both 2 chars — filtered out (same as the parser's split_identifier)
        assert_eq!(tokenize("getUserById"), vec!["get", "user"]);
    }

    #[test]
    fn tokenize_snake_case() {
        assert_eq!(tokenize("lock_manager"), vec!["lock", "manager"]);
    }

    #[test]
    fn tokenize_multi_word() {
        assert_eq!(tokenize("get user"), vec!["get", "user"]);
    }

    #[test]
    fn definition_boost() {
        let defn = make_sym("LockManager", &["lock", "manager", "function", "storage"]);
        let reference = make_sym("AcquireLock", &["lock", "manager", "acquire", "function"]);

        let symbols = vec![defn.clone(), reference.clone()];
        let idx = Bm25Index::build(&symbols);
        let query = tokenize("lock manager");

        let score_defn = idx.score(&defn, &query);
        let score_ref = idx.score(&reference, &query);

        // LockManager has both "lock" and "manager" in its own name → 3x boost per token
        assert!(score_defn > score_ref, "definition should outscore reference");
    }

    #[test]
    fn no_match_returns_zero() {
        let sym = make_sym("Foo", &["foo", "bar"]);
        let idx = Bm25Index::build(&[sym.clone()]);
        let query = tokenize("unrelated");
        assert_eq!(idx.score(&sym, &query), 0.0);
    }
}
