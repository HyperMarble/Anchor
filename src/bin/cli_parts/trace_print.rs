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
            println!("    <message>{message}</message>");
        }
        println!("  </event>");
    }
    println!("</trace>");

    Ok(())
}

const DEFAULT_CONTEXT_LINE_BUDGET: usize = 120;

fn print_bounded_numbered_code(code: &str, full: bool) {
    for (idx, line) in code.lines().enumerate() {
        if !full && idx >= DEFAULT_CONTEXT_LINE_BUDGET {
            println!(
                "    ... [context truncated at {DEFAULT_CONTEXT_LINE_BUDGET} lines; rerun with --full for complete symbol]"
            );
            break;
        }
        println!("{line}");
    }
}

fn print_bounded_plain_code(code: &str, start_line: usize, full: bool) {
    for (i, line) in code.lines().enumerate() {
        if !full && i >= DEFAULT_CONTEXT_LINE_BUDGET {
            println!(
                "    ... [context truncated at {DEFAULT_CONTEXT_LINE_BUDGET} lines; rerun with --full for complete symbol]"
            );
            break;
        }
        println!(" {:>3}: {}", start_line + i, line);
    }
}

fn print_constructor_child_context(
    store: &AnchorStore,
    symbols: &[SymbolEntry],
    parent: &SymbolEntry,
) -> Result<()> {
    if !is_class_like_symbol(parent) {
        return Ok(());
    }

    let mut children: Vec<&SymbolEntry> = symbols
        .iter()
        .filter(|candidate| {
            candidate.path == parent.path
                && candidate.line_start > parent.line_start
                && candidate.line_end <= parent.line_end
                && is_constructor_like_symbol(candidate)
        })
        .collect();
    children.sort_by_key(|symbol| symbol.line_start);
    children.truncate(2);

    for child in children {
        let projection = store.create_projection(child)?;
        println!(
            "<child_context role=\"constructor\" name=\"{}\" line=\"{}\">",
            escape_xml_text(&child.name),
            child.line_start
        );
        print_bounded_plain_code(&projection.text, child.line_start, false);
        println!("</child_context>");
    }

    Ok(())
}

fn is_class_like_symbol(symbol: &SymbolEntry) -> bool {
    matches!(
        symbol.kind.to_ascii_lowercase().as_str(),
        "class" | "struct" | "interface" | "enum"
    )
}

fn is_constructor_like_symbol(symbol: &SymbolEntry) -> bool {
    let name = symbol.name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "__init__" | "constructor" | "init" | "new" | "default"
    )
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

