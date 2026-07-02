fn is_risky_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("auth")
        || lower.contains("login")
        || lower.contains("permission")
        || lower.contains("security")
        || lower.contains("secret")
        || lower.contains("payment")
        || lower.contains("billing")
        || lower.contains("refund")
        || lower.contains("migration")
        || lower.contains("schema")
        || lower.ends_with(".env")
        || lower.contains("config")
}

impl ExecutionEvent {
    pub fn new(
        event_type: impl Into<String>,
        path: Option<String>,
        symbol: Option<String>,
        status: impl Into<String>,
        message: Option<String>,
    ) -> Self {
        Self::new_with_meta(event_type, path, symbol, status, message, BTreeMap::new())
    }

    pub fn new_with_meta(
        event_type: impl Into<String>,
        path: Option<String>,
        symbol: Option<String>,
        status: impl Into<String>,
        message: Option<String>,
        meta: BTreeMap<String, String>,
    ) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let session_id = std::env::var("ANCHOR_SESSION_ID").unwrap_or_else(|_| "local".into());
        let agent_id = crate::lock::lockd::agent_id().to_string();
        let event_type = event_type.into();
        let status = status.into();
        let id_seed = format!(
            "{timestamp_ms}\0{session_id}\0{agent_id}\0{event_type}\0{:?}\0{:?}\0{status}",
            path, symbol
        );

        Self {
            event_id: content_hash(id_seed.as_bytes()),
            timestamp_ms,
            session_id,
            agent_id,
            event_type,
            path,
            symbol,
            status,
            message,
            meta,
        }
    }
}
