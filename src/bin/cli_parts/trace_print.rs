fn cmd_trace(root: &Path, limit: usize) -> Result<()> {
    let store = open_store(root)?;
    let events = events::load(store.anchor_root())?;
    let start = events.len().saturating_sub(limit);

    println!(
        "<trace count=\"{}\" shown=\"{}\">",
        events.len(),
        events.len().saturating_sub(start)
    );
    for event in events.iter().skip(start) {
        println!(
            "  <event id=\"{}\" type=\"{}\" status=\"{}\" agent=\"{}\" session=\"{}\" ts=\"{}\">",
            event.event_id,
            event.event_type,
            event.status,
            event.agent_id,
            event.session_id,
            event.timestamp_ms
        );
        if let Some(path) = &event.path {
            println!("    <path>{path}</path>");
        }
        if let Some(symbol) = &event.symbol {
            println!("    <symbol>{symbol}</symbol>");
        }
        if let Some(message) = &event.message {
            println!("    <message>{}</message>", escape_xml_text(message));
        }
        println!("  </event>");
    }
    println!("</trace>");

    Ok(())
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
