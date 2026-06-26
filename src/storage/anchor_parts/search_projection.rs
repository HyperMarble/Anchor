impl AnchorStore {
    pub fn search_symbols(&self, query: &str, limit: usize) -> Result<Vec<SymbolEntry>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let query_lower = query.to_lowercase();
        let mut matches: Vec<SymbolEntry> = self
            .load_symbol_index()?
            .symbols
            .into_iter()
            .filter(|symbol| {
                symbol.name.to_lowercase().contains(&query_lower)
                    || symbol.path.to_lowercase().contains(&query_lower)
            })
            .collect();

        matches.sort_by(|a, b| {
            score_symbol_match(a, &query_lower)
                .cmp(&score_symbol_match(b, &query_lower))
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.line_start.cmp(&b.line_start))
                .then_with(|| a.name.cmp(&b.name))
        });
        matches.truncate(limit);

        Ok(matches)
    }

    pub fn create_projection(&self, symbol: &SymbolEntry) -> Result<Projection> {
        let source_path = self.repo_root.join(&symbol.path);
        let source = fs::read_to_string(&source_path)?;
        let current_hash = content_hash(source.as_bytes());
        if current_hash != symbol.source_hash {
            return Err(AnchorError::InvalidStructure(format!(
                "stale symbol index for {}: expected {}, got {}",
                symbol.path, symbol.source_hash, current_hash
            )));
        }

        let lines: Vec<&str> = source.lines().collect();
        if symbol.line_start < 1
            || symbol.line_end < symbol.line_start
            || symbol.line_end > lines.len()
        {
            return Err(AnchorError::InvalidStructure(format!(
                "invalid projection range {}:{}-{}",
                symbol.path, symbol.line_start, symbol.line_end
            )));
        }

        let slice = lines[symbol.line_start - 1..symbol.line_end].join("\n");
        let prefix = lines[..symbol.line_start - 1].join("\n");
        let suffix = lines[symbol.line_end..].join("\n");

        Ok(Projection {
            path: symbol.path.clone(),
            source_hash: symbol.source_hash.clone(),
            symbol: symbol.name.clone(),
            line_start: symbol.line_start,
            line_end: symbol.line_end,
            slice_hash: content_hash(slice.as_bytes()),
            prefix_hash: content_hash(prefix.as_bytes()),
            suffix_hash: content_hash(suffix.as_bytes()),
            text: slice,
        })
    }

    /// Search symbols using BM25 ranking with camelCase/snake_case tokenization.
    /// Name-token matches get a 3x boost over path/parent/kind context matches.
    /// Falls back to substring search for queries that tokenize to nothing (e.g. "id").
    pub fn search_symbols_hybrid(&self, query: &str, limit: usize) -> Result<Vec<SymbolEntry>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let query_tokens = crate::storage::bm25::tokenize(query);
        if query_tokens.is_empty() {
            return self.search_symbols(query, limit);
        }

        let index = self.load_symbol_index()?;
        let bm25 = crate::storage::bm25::Bm25Index::build(&index.symbols);

        let mut scored: Vec<(SymbolEntry, f32)> = index
            .symbols
            .into_iter()
            .filter_map(|sym| {
                let score = bm25.score(&sym, &query_tokens);
                if score > 0.0 {
                    Some((sym, score))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            scored_symbol_rank(&b.0, query, b.1)
                .partial_cmp(&scored_symbol_rank(&a.0, query, a.1))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.path.cmp(&b.0.path))
                .then_with(|| a.0.line_start.cmp(&b.0.line_start))
                .then_with(|| a.0.name.cmp(&b.0.name))
        });
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(sym, _)| sym).collect())
    }

    fn repo_relative_path(&self, path: &Path) -> Result<String> {
        let relative = path.strip_prefix(&self.repo_root).map_err(|_| {
            AnchorError::InvalidStructure(format!(
                "path is outside Anchor repo root: {}",
                path.display()
            ))
        })?;

        Ok(relative.to_string_lossy().replace('\\', "/"))
    }
}
