//
//  cache.rs
//  Anchor
//
//  Persistent cross-session symbol cache.
//  Stores symbol+path → slice_hash so unchanged symbols return "CACHED"
//  instead of full code on subsequent sessions. 98% token savings for
//  symbols that haven't changed between agent sessions.
//

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const CACHE_FILE: &str = "persistent_cache.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PersistentCache {
    // key: "symbol_name\x00file_path", value: slice_hash
    entries: HashMap<String, String>,
    #[serde(skip)]
    dirty: bool,
}

impl PersistentCache {
    pub fn load(anchor_root: &Path) -> Self {
        let path = anchor_root.join(CACHE_FILE);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&mut self, anchor_root: &Path) {
        if !self.dirty {
            return;
        }
        let path = anchor_root.join(CACHE_FILE);
        if let Ok(json) = serde_json::to_string(&self.entries) {
            let _ = std::fs::write(path, json);
            self.dirty = false;
        }
    }

    fn key(name: &str, file_path: &str) -> String {
        format!("{}\x00{}", name, file_path)
    }

    /// Returns true if the symbol's hash matches the cached hash (unchanged).
    pub fn is_hit(&self, name: &str, file_path: &str, slice_hash: &str) -> bool {
        self.entries
            .get(&Self::key(name, file_path))
            .map(|h| h == slice_hash)
            .unwrap_or(false)
    }

    /// Record or update the hash for a symbol. Marks dirty if changed.
    pub fn update(&mut self, name: &str, file_path: &str, slice_hash: &str) {
        let key = Self::key(name, file_path);
        let current = self.entries.get(&key);
        if current.map(|h| h.as_str()) != Some(slice_hash) {
            self.entries.insert(key, slice_hash.to_string());
            self.dirty = true;
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
