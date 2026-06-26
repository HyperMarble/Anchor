impl AnchorStore {
    /// Symbols in a file that overlap the given line range. Used for write impact analysis.
    pub fn symbols_in_range<'a>(
        &self,
        index: &'a SymbolIndex,
        path: &str,
        start: usize,
        end: usize,
    ) -> Vec<&'a SymbolEntry> {
        index
            .symbols
            .iter()
            .filter(|s| s.path == path && s.line_start <= end && s.line_end >= start)
            .collect()
    }

    /// Absolute line numbers (1-indexed) of every call site inside a symbol's body.
    /// Re-parses the file to recover line info (the saved CallIndex drops it).
    /// Used by `slice_code` to keep only call-relevant lines in projections.
    pub fn call_lines_for_symbol(&self, sym: &SymbolEntry) -> Vec<usize> {
        let source_path = self.repo_root.join(&sym.path);
        let source = match fs::read_to_string(&source_path) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let extraction = match crate::parser::extract_file(&source_path, &source) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        extraction
            .calls
            .into_iter()
            .filter(|c| c.line >= sym.line_start && c.line <= sym.line_end)
            .map(|c| c.line)
            .collect()
    }

    pub fn upsert_symbols_for_path(
        &self,
        source_path: &Path,
    ) -> Result<(PathEntry, Vec<SymbolEntry>, bool)> {
        let source = fs::read_to_string(source_path)?;
        let extraction = crate::parser::extract_file(source_path, &source)?;
        let (path_entry, path_changed) = self.upsert_path(source_path)?;

        let mut symbols: Vec<SymbolEntry> = extraction
            .symbols
            .iter()
            .map(|symbol| SymbolEntry {
                path: path_entry.path.clone(),
                source_hash: path_entry.source_hash.clone(),
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                line_start: symbol.line_start,
                line_end: symbol.line_end,
                slice_hash: content_hash(symbol.code_snippet.as_bytes()),
                features: symbol.features.clone(),
            })
            .collect();
        symbols.sort_by(|a, b| {
            a.line_start
                .cmp(&b.line_start)
                .then_with(|| a.name.cmp(&b.name))
        });

        let mut index = self.load_symbol_index()?;
        let existing: Vec<SymbolEntry> = index
            .symbols
            .iter()
            .filter(|symbol| symbol.path == path_entry.path)
            .cloned()
            .collect();
        let changed = path_changed || existing != symbols;

        if changed {
            index
                .symbols
                .retain(|symbol| symbol.path != path_entry.path);
            index.symbols.extend(symbols.clone());
            index.symbols.sort_by(|a, b| {
                a.path
                    .cmp(&b.path)
                    .then_with(|| a.line_start.cmp(&b.line_start))
                    .then_with(|| a.name.cmp(&b.name))
            });
            self.save_symbol_index(&index)?;
        }

        Ok((path_entry, symbols, changed))
    }
}
