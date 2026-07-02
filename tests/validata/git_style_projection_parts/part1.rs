use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tempfile::tempdir;

#[derive(Debug, Clone)]
struct SearchHit {
    source_path: PathBuf,
    source_hash: String,
    symbol: String,
    line_start: usize,
    line_end: usize,
}

#[derive(Debug, Clone)]
struct Projection {
    source_path: PathBuf,
    source_hash: String,
    symbol: String,
    line_start: usize,
    line_end: usize,
    slice_hash: String,
    prefix_hash: String,
    suffix_hash: String,
    lock_id: String,
    text: String,
}

#[derive(Debug, PartialEq, Eq)]
enum ApplyError {
    StaleSource,
    StaleSlice,
    InvalidRange,
    MissingLock,
    LockConflict,
}

#[derive(Debug)]
struct ProofMetrics {
    full_context_bytes: usize,
    projection_bytes: usize,
    context_reduction_percent: f64,
    unrelated_symbols_excluded: usize,
    stale_edits_rejected: usize,
    lock_conflicts_rejected: usize,
    verified_after_edit: bool,
    index_hash_refreshed: bool,
}

#[derive(Debug)]
struct CorpusMetrics {
    files_seen: usize,
    symbols_tested: usize,
    avg_context_reduction_percent: f64,
    median_context_reduction_percent: f64,
    p90_context_reduction_percent: f64,
    min_context_reduction_percent: f64,
    max_context_reduction_percent: f64,
    avg_full_context_bytes: f64,
    avg_projection_bytes: f64,
    lock_conflicts_rejected: usize,
    verified_after_edit: usize,
    index_hash_refreshed: usize,
    failures: usize,
}

fn content_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn parse_object_path(anchor_dir: &Path, hash: &str) -> PathBuf {
    anchor_dir
        .join("objects")
        .join("parses")
        .join(&hash[..2])
        .join(format!("{hash}.json"))
}

fn symbols_index_path(anchor_dir: &Path) -> PathBuf {
    anchor_dir.join("index").join("symbols.json")
}

fn paths_index_path(anchor_dir: &Path) -> PathBuf {
    anchor_dir.join("index").join("paths.json")
}

fn lock_path(anchor_dir: &Path, lock_id: &str) -> PathBuf {
    anchor_dir
        .join("locks")
        .join("ranges")
        .join(format!("{lock_id}.json"))
}

fn store_parse_object(anchor_dir: &Path, source: &str) -> std::io::Result<(String, bool)> {
    let hash = content_hash(source.as_bytes());
    let path = parse_object_path(anchor_dir, &hash);
    let existed = path.exists();

    if !existed {
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(
            &path,
            format!(
                "{{\"content_hash\":\"{hash}\",\"bytes\":{}}}\n",
                source.len()
            ),
        )?;
    }

    Ok((hash, existed))
}

fn index_file(anchor_dir: &Path, source_path: &Path) -> std::io::Result<String> {
    let source = fs::read_to_string(source_path)?;
    let (source_hash, _) = store_parse_object(anchor_dir, &source)?;
    let extraction = anchor::parser::extract_file(source_path, &source).unwrap();

    fs::create_dir_all(anchor_dir.join("index"))?;
    fs::write(
        paths_index_path(anchor_dir),
        serde_json::to_string_pretty(&json!([{
            "path": source_path,
            "source_hash": source_hash,
            "bytes": source.len(),
            "symbols": extraction.symbols.len(),
        }]))
        .unwrap(),
    )?;

    let symbols: Vec<Value> = extraction
        .symbols
        .iter()
        .map(|symbol| {
            json!({
                "path": source_path,
                "source_hash": source_hash,
                "name": symbol.name,
                "kind": format!("{:?}", symbol.kind),
                "line_start": symbol.line_start,
                "line_end": symbol.line_end,
                "slice_hash": content_hash(symbol.code_snippet.as_bytes()),
            })
        })
        .collect();

    fs::write(
        symbols_index_path(anchor_dir),
        serde_json::to_string_pretty(&symbols).unwrap(),
    )?;

    Ok(source_hash)
}

fn search_symbol(anchor_dir: &Path, name: &str) -> Vec<SearchHit> {
    let raw = fs::read_to_string(symbols_index_path(anchor_dir)).unwrap();
    let symbols: Vec<Value> = serde_json::from_str(&raw).unwrap();

    symbols
        .iter()
        .filter(|symbol| symbol["name"].as_str() == Some(name))
        .map(|symbol| SearchHit {
            source_path: PathBuf::from(symbol["path"].as_str().unwrap()),
            source_hash: symbol["source_hash"].as_str().unwrap().to_string(),
            symbol: symbol["name"].as_str().unwrap().to_string(),
            line_start: symbol["line_start"].as_u64().unwrap() as usize,
            line_end: symbol["line_end"].as_u64().unwrap() as usize,
        })
        .collect()
}

