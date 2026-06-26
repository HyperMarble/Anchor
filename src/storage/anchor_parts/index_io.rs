impl AnchorStore {
    pub fn path_index_path(&self) -> PathBuf {
        self.anchor_root.join("index").join("paths.json")
    }

    pub fn symbol_index_path(&self) -> PathBuf {
        self.anchor_root.join("index").join("symbols.json")
    }

    pub fn load_path_index(&self) -> Result<PathIndex> {
        let path = self.path_index_path();
        if !path.exists() {
            return Ok(PathIndex::default());
        }

        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save_path_index(&self, index: &PathIndex) -> Result<()> {
        let path = self.path_index_path();
        fs::create_dir_all(path.parent().ok_or_else(|| {
            AnchorError::InvalidStructure(format!("path index has no parent: {}", path.display()))
        })?)?;
        fs::write(path, serde_json::to_vec_pretty(index)?)?;
        Ok(())
    }

    pub fn upsert_path(&self, source_path: &Path) -> Result<(PathEntry, bool)> {
        let bytes = fs::read(source_path)?;
        let entry = PathEntry {
            path: self.repo_relative_path(source_path)?,
            source_hash: content_hash(&bytes),
            bytes: bytes.len() as u64,
        };

        let mut index = self.load_path_index()?;
        let mut changed = true;

        if let Some(existing) = index.files.iter_mut().find(|item| item.path == entry.path) {
            if existing == &entry {
                changed = false;
            } else {
                *existing = entry.clone();
            }
        } else {
            index.files.push(entry.clone());
        }

        if changed {
            index.files.sort_by(|a, b| a.path.cmp(&b.path));
            self.save_path_index(&index)?;
        }

        Ok((entry, changed))
    }

    pub fn load_symbol_index(&self) -> Result<SymbolIndex> {
        let path = self.symbol_index_path();
        if !path.exists() {
            return Ok(SymbolIndex::default());
        }

        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save_symbol_index(&self, index: &SymbolIndex) -> Result<()> {
        let path = self.symbol_index_path();
        fs::create_dir_all(path.parent().ok_or_else(|| {
            AnchorError::InvalidStructure(format!("symbol index has no parent: {}", path.display()))
        })?)?;
        fs::write(path, serde_json::to_vec_pretty(index)?)?;
        Ok(())
    }

    pub fn call_index_path(&self) -> PathBuf {
        self.anchor_root.join("index").join("calls.json")
    }

    pub fn save_call_index(&self, index: &CallIndex) -> Result<()> {
        let path = self.call_index_path();
        fs::create_dir_all(path.parent().ok_or_else(|| {
            AnchorError::InvalidStructure(format!("call index has no parent: {}", path.display()))
        })?)?;
        fs::write(path, serde_json::to_vec(index)?)?;
        Ok(())
    }

    pub fn load_call_index(&self) -> CallIndex {
        let Ok(bytes) = fs::read(self.call_index_path()) else {
            return CallIndex::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn history_index_path(&self) -> PathBuf {
        self.anchor_root.join("index").join("history.json")
    }

    pub fn save_history_index(&self, index: &HistoryIndex) -> Result<()> {
        let path = self.history_index_path();
        fs::create_dir_all(path.parent().ok_or_else(|| {
            AnchorError::InvalidStructure(format!(
                "history index has no parent: {}",
                path.display()
            ))
        })?)?;
        fs::write(path, serde_json::to_vec_pretty(index)?)?;
        Ok(())
    }

    pub fn load_history_index(&self) -> HistoryIndex {
        let Ok(bytes) = fs::read(self.history_index_path()) else {
            return HistoryIndex::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }
}
