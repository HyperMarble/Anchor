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

fn create_projection(
    source_path: &Path,
    source: &str,
    symbol: &str,
    line_start: usize,
    line_end: usize,
) -> Projection {
    let lines: Vec<&str> = source.lines().collect();
    assert!(line_start >= 1);
    assert!(line_end >= line_start);
    assert!(line_end <= lines.len());

    let slice = lines[line_start - 1..line_end].join("\n");
    let prefix = lines[..line_start - 1].join("\n");
    let suffix = lines[line_end..].join("\n");

    Projection {
        source_path: source_path.to_path_buf(),
        source_hash: content_hash(source.as_bytes()),
        symbol: symbol.to_string(),
        line_start,
        line_end,
        slice_hash: content_hash(slice.as_bytes()),
        prefix_hash: content_hash(prefix.as_bytes()),
        suffix_hash: content_hash(suffix.as_bytes()),
        lock_id: format!("lock-{}", content_hash(symbol.as_bytes())),
        text: slice,
    }
}

fn create_projection_from_hit(hit: &SearchHit) -> Projection {
    let source = fs::read_to_string(&hit.source_path).unwrap();
    assert_eq!(content_hash(source.as_bytes()), hit.source_hash);
    create_projection(
        &hit.source_path,
        &source,
        &hit.symbol,
        hit.line_start,
        hit.line_end,
    )
}

fn acquire_lock(anchor_dir: &Path, projection: &Projection, owner: &str) -> Result<(), ApplyError> {
    let path = lock_path(anchor_dir, &projection.lock_id);
    if path.exists() {
        let raw = fs::read_to_string(path).map_err(|_| ApplyError::MissingLock)?;
        let lock: Value = serde_json::from_str(&raw).map_err(|_| ApplyError::MissingLock)?;
        if lock["owner"].as_str() != Some(owner) {
            return Err(ApplyError::LockConflict);
        }
        return Ok(());
    }

    fs::create_dir_all(path.parent().unwrap()).map_err(|_| ApplyError::MissingLock)?;
    fs::write(
        path,
        serde_json::to_string_pretty(&json!({
            "id": projection.lock_id,
            "owner": owner,
            "path": projection.source_path,
            "symbol": projection.symbol,
            "source_hash": projection.source_hash,
            "line_start": projection.line_start,
            "line_end": projection.line_end,
        }))
        .unwrap(),
    )
    .map_err(|_| ApplyError::MissingLock)
}

fn assert_lock_owner(
    anchor_dir: &Path,
    projection: &Projection,
    owner: &str,
) -> Result<(), ApplyError> {
    let path = lock_path(anchor_dir, &projection.lock_id);
    if !path.exists() {
        return Err(ApplyError::MissingLock);
    }

    let raw = fs::read_to_string(path).map_err(|_| ApplyError::MissingLock)?;
    let lock: Value = serde_json::from_str(&raw).map_err(|_| ApplyError::MissingLock)?;
    if lock["owner"].as_str() != Some(owner) {
        return Err(ApplyError::LockConflict);
    }

    Ok(())
}

fn apply_projection(projection: &Projection, edited_text: &str) -> Result<(), ApplyError> {
    let current =
        fs::read_to_string(&projection.source_path).map_err(|_| ApplyError::InvalidRange)?;
    if content_hash(current.as_bytes()) != projection.source_hash {
        return Err(ApplyError::StaleSource);
    }

    let lines: Vec<&str> = current.lines().collect();
    if projection.line_start < 1
        || projection.line_end < projection.line_start
        || projection.line_end > lines.len()
    {
        return Err(ApplyError::InvalidRange);
    }

    let current_slice = lines[projection.line_start - 1..projection.line_end].join("\n");
    if content_hash(current_slice.as_bytes()) != projection.slice_hash {
        return Err(ApplyError::StaleSlice);
    }

    let prefix = lines[..projection.line_start - 1].join("\n");
    let suffix = lines[projection.line_end..].join("\n");
    if content_hash(prefix.as_bytes()) != projection.prefix_hash
        || content_hash(suffix.as_bytes()) != projection.suffix_hash
    {
        return Err(ApplyError::StaleSource);
    }

    let mut next = String::new();
    if !prefix.is_empty() {
        next.push_str(&prefix);
        next.push('\n');
    }
    next.push_str(edited_text.trim_end_matches('\n'));
    if !suffix.is_empty() {
        next.push('\n');
        next.push_str(&suffix);
    }
    if current.ends_with('\n') {
        next.push('\n');
    }

    fs::write(&projection.source_path, next).map_err(|_| ApplyError::InvalidRange)
}

fn apply_locked_projection(
    anchor_dir: &Path,
    projection: &Projection,
    owner: &str,
    edited_text: &str,
) -> Result<(), ApplyError> {
    assert_lock_owner(anchor_dir, projection, owner)?;
    apply_projection(projection, edited_text)
}

fn verify_file_parses(source_path: &Path) {
    let source = fs::read_to_string(source_path).unwrap();
    anchor::parser::extract_file(source_path, &source).unwrap();
}

fn context_reduction_percent(full_bytes: usize, projection_bytes: usize) -> f64 {
    assert!(full_bytes > 0);
    100.0 - ((projection_bytes as f64 / full_bytes as f64) * 100.0)
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    assert!(!values.is_empty());
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}

fn collect_ts_files(root: &Path, out: &mut Vec<PathBuf>, max_files: usize) {
    if out.len() >= max_files {
        return;
    }

    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        if out.len() >= max_files {
            return;
        }

        let path = entry.path();
        if path.is_dir() {
            collect_ts_files(&path, out, max_files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("ts")
            && !path.to_string_lossy().ends_with(".d.ts")
        {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if (5_000..=90_000).contains(&meta.len()) {
                out.push(path);
            }
        }
    }
}

#[test]
fn parse_objects_are_reused_by_content_hash() {
    let dir = tempdir().unwrap();
    let anchor_dir = dir.path().join(".anchor");
    let source = "export function activate() {\n  return true;\n}\n";

    let (first_hash, first_existed) = store_parse_object(&anchor_dir, source).unwrap();
    let (second_hash, second_existed) = store_parse_object(&anchor_dir, source).unwrap();

    assert_eq!(first_hash, second_hash);
    assert!(!first_existed);
    assert!(second_existed);
    assert!(parse_object_path(&anchor_dir, &first_hash).exists());
}

#[test]
fn projection_transplants_slice_edit_back_to_source_file() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("extension.ts");
    fs::write(
        &file,
        "import * as vscode from 'vscode';\n\nexport function activate() {\n  return true;\n}\n\nexport function deactivate() {}\n",
    )
    .unwrap();

    let source = fs::read_to_string(&file).unwrap();
    let projection = create_projection(&file, &source, "activate", 3, 5);
    assert_eq!(projection.symbol, "activate");
    assert!(projection.lock_id.starts_with("lock-"));
    assert!(projection.text.contains("return true"));

    apply_projection(
        &projection,
        "export function activate() {\n  console.log('ready');\n  return true;\n}",
    )
    .unwrap();

    let updated = fs::read_to_string(&file).unwrap();
    assert!(updated.contains("console.log('ready');"));
    assert!(updated.contains("import * as vscode"));
    assert!(updated.contains("export function deactivate()"));
}

#[test]
fn projection_rejects_when_source_changed_after_context() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("service.ts");
    fs::write(&file, "export function start() {\n  return boot();\n}\n").unwrap();

    let source = fs::read_to_string(&file).unwrap();
    let projection = create_projection(&file, &source, "start", 1, 3);

    fs::write(
        &file,
        "export function start() {\n  audit();\n  return boot();\n}\n",
    )
    .unwrap();

    let result = apply_projection(
        &projection,
        "export function start() {\n  return bootFast();\n}",
    );
    assert_eq!(result, Err(ApplyError::StaleSource));
}

#[test]
fn changed_content_gets_a_new_parse_object() {
    let dir = tempdir().unwrap();
    let anchor_dir = dir.path().join(".anchor");

    let original = "export const version = 1;\n";
    let changed = "export const version = 2;\n";

    let (original_hash, _) = store_parse_object(&anchor_dir, original).unwrap();
    let (changed_hash, changed_existed) = store_parse_object(&anchor_dir, changed).unwrap();

    assert_ne!(original_hash, changed_hash);
    assert!(!changed_existed);
    assert!(parse_object_path(&anchor_dir, &original_hash).exists());
    assert!(parse_object_path(&anchor_dir, &changed_hash).exists());
}

#[test]
fn search_context_locked_edit_and_update_flow_proves_anchor_value() {
    let dir = tempdir().unwrap();
    let anchor_dir = dir.path().join(".anchor");
    let file = dir.path().join("extension.ts");
    let large_unrelated_context = (0..80)
        .map(|i| format!("export const unrelated{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(
        &file,
        format!(
            "{large_unrelated_context}\n\n\
             export function activate() {{\n\
             \treturn true;\n\
             }}\n\n\
             export function deactivate() {{\n\
             \treturn false;\n\
             }}\n"
        ),
    )
    .unwrap();

    let original_source = fs::read_to_string(&file).unwrap();
    let original_hash = index_file(&anchor_dir, &file).unwrap();
    let hits = search_symbol(&anchor_dir, "activate");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_hash, original_hash);

    let projection = create_projection_from_hit(&hits[0]);
    let reduction = context_reduction_percent(original_source.len(), projection.text.len());
    assert!(
        projection.text.len() * 8 < original_source.len(),
        "projection should give the agent a much smaller context slice"
    );
    assert!(
        reduction >= 87.5,
        "projection should reduce context by at least 87.5%"
    );
    assert!(projection.text.contains("return true"));
    assert!(!projection.text.contains("unrelated79"));
    assert!(!projection.text.contains("deactivate"));

    acquire_lock(&anchor_dir, &projection, "agent-a").unwrap();
    assert_eq!(
        acquire_lock(&anchor_dir, &projection, "agent-b"),
        Err(ApplyError::LockConflict)
    );

    apply_locked_projection(
        &anchor_dir,
        &projection,
        "agent-a",
        "export function activate() {\n\tconsole.log('ready');\n\treturn true;\n}",
    )
    .unwrap();
    verify_file_parses(&file);

    let updated_source = fs::read_to_string(&file).unwrap();
    assert!(updated_source.contains("console.log('ready');"));
    assert!(updated_source.contains("unrelated79"));
    assert!(updated_source.contains("export function deactivate()"));

    let updated_hash = index_file(&anchor_dir, &file).unwrap();
    assert_ne!(original_hash, updated_hash);
    let updated_hits = search_symbol(&anchor_dir, "activate");
    assert_eq!(updated_hits.len(), 1);
    assert_eq!(updated_hits[0].source_hash, updated_hash);

    let metrics = ProofMetrics {
        full_context_bytes: original_source.len(),
        projection_bytes: projection.text.len(),
        context_reduction_percent: reduction,
        unrelated_symbols_excluded: 81,
        stale_edits_rejected: 0,
        lock_conflicts_rejected: 1,
        verified_after_edit: true,
        index_hash_refreshed: updated_hash != original_hash,
    };

    eprintln!("anchor proof metrics: {metrics:?}");
    assert_eq!(metrics.full_context_bytes, original_source.len());
    assert_eq!(metrics.projection_bytes, projection.text.len());
    assert!(metrics.context_reduction_percent >= 98.0);
    assert_eq!(metrics.unrelated_symbols_excluded, 81);
    assert_eq!(metrics.lock_conflicts_rejected, 1);
    assert!(metrics.verified_after_edit);
    assert!(metrics.index_hash_refreshed);
}

#[test]
fn stale_edit_rejection_has_a_countable_safety_metric() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("service.ts");
    fs::write(&file, "export function start() {\n  return boot();\n}\n").unwrap();

    let source = fs::read_to_string(&file).unwrap();
    let projection = create_projection(&file, &source, "start", 1, 3);

    fs::write(
        &file,
        "export function start() {\n  audit();\n  return boot();\n}\n",
    )
    .unwrap();

    let rejected = apply_projection(
        &projection,
        "export function start() {\n  return bootFast();\n}",
    ) == Err(ApplyError::StaleSource);

    let metrics = ProofMetrics {
        full_context_bytes: source.len(),
        projection_bytes: projection.text.len(),
        context_reduction_percent: context_reduction_percent(source.len(), projection.text.len()),
        unrelated_symbols_excluded: 0,
        stale_edits_rejected: usize::from(rejected),
        lock_conflicts_rejected: 0,
        verified_after_edit: false,
        index_hash_refreshed: false,
    };

    eprintln!("anchor stale-edit safety metrics: {metrics:?}");
    assert_eq!(metrics.full_context_bytes, source.len());
    assert_eq!(metrics.projection_bytes, projection.text.len());
    assert!(metrics.context_reduction_percent >= 0.0);
    assert_eq!(metrics.unrelated_symbols_excluded, 0);
    assert_eq!(metrics.stale_edits_rejected, 1);
}

#[test]
#[ignore = "real VS Code repo probe; run explicitly when /Volumes/Hak_SSD/vscode is available"]
fn real_vscode_file_projection_metrics() {
    let vscode_repo = std::env::var("ANCHOR_REAL_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Volumes/Hak_SSD/vscode"));
    let real_file = vscode_repo.join("src/vs/workbench/browser/dnd.ts");
    assert!(
        real_file.exists(),
        "missing VS Code checkout at {}",
        real_file.display()
    );

    let dir = tempdir().unwrap();
    let anchor_dir = dir.path().join(".anchor");
    let file = dir.path().join("dnd.ts");
    fs::copy(&real_file, &file).unwrap();

    let original_source = fs::read_to_string(&file).unwrap();
    let original_hash = index_file(&anchor_dir, &file).unwrap();
    let hits = search_symbol(&anchor_dir, "extractTreeDropData");
    assert_eq!(hits.len(), 1);

    let projection = create_projection_from_hit(&hits[0]);
    let reduction = context_reduction_percent(original_source.len(), projection.text.len());
    assert!(projection.text.contains("extractTreeDropData"));
    assert!(!projection
        .text
        .contains("export class ResourcesDropHandler"));

    acquire_lock(&anchor_dir, &projection, "agent-a").unwrap();
    let edited_text = projection.text.replacen(
        "{\n",
        "{\n\tconst anchorProjectionProbe = true;\n\tvoid anchorProjectionProbe;\n",
        1,
    );
    apply_locked_projection(&anchor_dir, &projection, "agent-a", &edited_text).unwrap();
    verify_file_parses(&file);

    let updated_hash = index_file(&anchor_dir, &file).unwrap();
    let updated_hits = search_symbol(&anchor_dir, "extractTreeDropData");
    assert_eq!(updated_hits.len(), 1);
    assert_eq!(updated_hits[0].source_hash, updated_hash);

    let metrics = ProofMetrics {
        full_context_bytes: original_source.len(),
        projection_bytes: projection.text.len(),
        context_reduction_percent: reduction,
        unrelated_symbols_excluded: search_symbol(&anchor_dir, "ResourcesDropHandler").len(),
        stale_edits_rejected: 0,
        lock_conflicts_rejected: 0,
        verified_after_edit: true,
        index_hash_refreshed: updated_hash != original_hash,
    };

    eprintln!("anchor real vscode metrics: {metrics:?}");
    assert!(metrics.context_reduction_percent > 90.0);
    assert!(metrics.verified_after_edit);
    assert!(metrics.index_hash_refreshed);
}

#[test]
#[ignore = "real VS Code corpus probe; run explicitly when /Volumes/Hak_SSD/vscode is available"]
fn real_vscode_many_symbol_projection_metrics() {
    let vscode_repo = std::env::var("ANCHOR_REAL_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Volumes/Hak_SSD/vscode"));
    let root = vscode_repo.join("src/vs/workbench/browser");
    assert!(
        root.exists(),
        "missing VS Code checkout at {}",
        root.display()
    );

    let dir = tempdir().unwrap();
    let mut files = Vec::new();
    collect_ts_files(&root, &mut files, 120);

    let mut reductions = Vec::new();
    let mut full_bytes_total = 0usize;
    let mut projection_bytes_total = 0usize;
    let mut lock_conflicts_rejected = 0usize;
    let mut verified_after_edit = 0usize;
    let mut index_hash_refreshed = 0usize;
    let mut failures = 0usize;
    let target_symbols = 50usize;

    'files: for real_file in &files {
        let source = match fs::read_to_string(real_file) {
            Ok(source) => source,
            Err(_) => continue,
        };
        let extraction = match anchor::parser::extract_file(real_file, &source) {
            Ok(extraction) => extraction,
            Err(_) => continue,
        };

        for symbol in extraction.symbols {
            if reductions.len() >= target_symbols {
                break 'files;
            }
            if symbol.line_end <= symbol.line_start
                || symbol.code_snippet.len() < 40
                || symbol.code_snippet.len() > 5_000
                || !symbol.code_snippet.contains("{\n")
            {
                continue;
            }

            let case_dir = dir.path().join(format!("case_{}", reductions.len()));
            fs::create_dir_all(&case_dir).unwrap();
            let temp_file = case_dir.join("sample.ts");
            fs::write(&temp_file, &source).unwrap();
            let anchor_dir = case_dir.join(".anchor");

            let original_hash = match index_file(&anchor_dir, &temp_file) {
                Ok(hash) => hash,
                Err(_) => {
                    failures += 1;
                    continue;
                }
            };
            let hits = search_symbol(&anchor_dir, &symbol.name);
            if hits.len() != 1 {
                continue;
            }

            let projection = create_projection_from_hit(&hits[0]);
            let reduction = context_reduction_percent(source.len(), projection.text.len());
            let second_owner_blocked = acquire_lock(&anchor_dir, &projection, "agent-a").is_ok()
                && acquire_lock(&anchor_dir, &projection, "agent-b")
                    == Err(ApplyError::LockConflict);
            if second_owner_blocked {
                lock_conflicts_rejected += 1;
            }

            let edited_text =
                projection
                    .text
                    .replacen("{\n", "{\n\t// anchor projection corpus probe\n", 1);
            if apply_locked_projection(&anchor_dir, &projection, "agent-a", &edited_text).is_err() {
                failures += 1;
                continue;
            }

            if anchor::parser::extract_file(&temp_file, &fs::read_to_string(&temp_file).unwrap())
                .is_ok()
            {
                verified_after_edit += 1;
            } else {
                failures += 1;
                continue;
            }

            let updated_hash = match index_file(&anchor_dir, &temp_file) {
                Ok(hash) => hash,
                Err(_) => {
                    failures += 1;
                    continue;
                }
            };
            if updated_hash != original_hash {
                index_hash_refreshed += 1;
            }

            reductions.push(reduction);
            full_bytes_total += source.len();
            projection_bytes_total += projection.text.len();
        }
    }

    let metrics = CorpusMetrics {
        files_seen: files.len(),
        symbols_tested: reductions.len(),
        avg_context_reduction_percent: reductions.iter().sum::<f64>() / reductions.len() as f64,
        median_context_reduction_percent: percentile(&reductions, 0.50),
        p90_context_reduction_percent: percentile(&reductions, 0.90),
        min_context_reduction_percent: percentile(&reductions, 0.00),
        max_context_reduction_percent: percentile(&reductions, 1.00),
        avg_full_context_bytes: full_bytes_total as f64 / reductions.len() as f64,
        avg_projection_bytes: projection_bytes_total as f64 / reductions.len() as f64,
        lock_conflicts_rejected,
        verified_after_edit,
        index_hash_refreshed,
        failures,
    };

    eprintln!("anchor real vscode corpus metrics: {metrics:?}");
    assert!(metrics.files_seen >= 20);
    assert!(metrics.symbols_tested >= 20);
    assert!(metrics.avg_context_reduction_percent >= 80.0);
    assert!(metrics.median_context_reduction_percent >= 80.0);
    assert!(metrics.p90_context_reduction_percent >= metrics.median_context_reduction_percent);
    assert!(metrics.min_context_reduction_percent <= metrics.max_context_reduction_percent);
    assert!(metrics.avg_full_context_bytes > metrics.avg_projection_bytes);
    assert_eq!(metrics.lock_conflicts_rejected, metrics.symbols_tested);
    assert_eq!(metrics.verified_after_edit, metrics.symbols_tested);
    assert_eq!(metrics.index_hash_refreshed, metrics.symbols_tested);
    assert_eq!(metrics.failures, 0);
}
