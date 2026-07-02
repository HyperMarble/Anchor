pub fn record(
    anchor_root: &Path,
    event_type: impl Into<String>,
    path: Option<String>,
    symbol: Option<String>,
    status: impl Into<String>,
    message: Option<String>,
) {
    let event = ExecutionEvent::new(event_type, path, symbol, status, message);
    if let Err(err) = append(anchor_root, &event) {
        eprintln!("anchor: failed to record event: {err}");
    }
}

/// Record an event that the caller treats as load-bearing: mutating
/// operations call this *before* touching the file so that "no receipt, no
/// write" holds — a flight recorder that can silently fail is not evidence.
pub fn record_required(
    anchor_root: &Path,
    event_type: impl Into<String>,
    path: Option<String>,
    symbol: Option<String>,
    status: impl Into<String>,
    message: Option<String>,
) -> anyhow::Result<()> {
    let event = ExecutionEvent::new(event_type, path, symbol, status, message);
    append(anchor_root, &event)
}

pub fn record_with_meta(
    anchor_root: &Path,
    event_type: impl Into<String>,
    path: Option<String>,
    symbol: Option<String>,
    status: impl Into<String>,
    message: Option<String>,
    meta: BTreeMap<String, String>,
) {
    let event = ExecutionEvent::new_with_meta(event_type, path, symbol, status, message, meta);
    if let Err(err) = append(anchor_root, &event) {
        eprintln!("anchor: failed to record event: {err}");
    }
}
