fn record_context_read(
    store: &AnchorStore,
    sym: &SymbolEntry,
    status: &str,
    message: Option<String>,
) {
    let mut meta = std::collections::BTreeMap::new();
    meta.insert("source_hash".to_string(), sym.source_hash.clone());
    meta.insert("slice_hash".to_string(), sym.slice_hash.clone());
    events::record_with_meta(
        store.anchor_root(),
        "context.read",
        Some(sym.path.clone()),
        Some(sym.name.clone()),
        status,
        message,
        meta,
    );
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskPacket {
    schema: String,
    intent: String,
    likely_files: Vec<TaskPath>,
    owner_chunks: Vec<TaskSlice>,
    related_files: Vec<TaskRelatedFile>,
    likely_tests: Vec<TaskTest>,
    verification_plan: TaskVerificationPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskPath {
    path: String,
    source_hash: String,
    score: i32,
    role: String,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskSlice {
    path: String,
    source_hash: String,
    owner: String,
    symbol: String,
    kind: String,
    line_start: usize,
    line_end: usize,
    score: i32,
    reasons: Vec<String>,
    meaning: String,
    responsibility_tags: Vec<String>,
    code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskTest {
    path: String,
    score: usize,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskRelatedFile {
    path: String,
    score: usize,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskVerificationPlan {
    steps: Vec<String>,
    preferred_check: Option<String>,
    check_hints: Vec<TaskCheckHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskCheckHint {
    kind: String,
    command: String,
}

fn task_packet_path(store: &AnchorStore) -> PathBuf {
    store
        .anchor_root()
        .join("tasks")
        .join(TASK_WORKSPACE_CURRENT)
}

fn save_task_packet(store: &AnchorStore, packet: &TaskPacket) -> Result<()> {
    let path = task_packet_path(store);
    std::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("task packet path has no parent: {}", path.display()))?,
    )?;
    std::fs::write(path, serde_json::to_vec_pretty(packet)?)?;
    Ok(())
}

