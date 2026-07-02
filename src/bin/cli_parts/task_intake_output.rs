struct TaskIntakeOutput<'a> {
    store: &'a AnchorStore,
    symbol_index: &'a SymbolIndex,
    call_index: &'a CallIndex,
    history_index: &'a HistoryIndex,
    scoped_files: usize,
    intent: &'a str,
    candidates: &'a [SymbolEntry],
    context_limit: usize,
    packet: &'a TaskPacket,
    related_files: &'a std::collections::BTreeSet<String>,
    historical_files: &'a std::collections::BTreeMap<String, usize>,
    likely_tests: &'a [(&'a String, usize)],
    likely_test_count: usize,
    historical_tests: &'a std::collections::BTreeMap<String, usize>,
}

fn print_task_intake_output(input: TaskIntakeOutput<'_>) -> Result<()> {
    let TaskIntakeOutput {
        store,
        symbol_index,
        call_index,
        history_index,
        scoped_files,
        intent,
        candidates,
        context_limit,
        packet,
        related_files,
        historical_files,
        likely_tests,
        likely_test_count,
        historical_tests,
    } = input;

    println!("<task_intake>");
    println!("<intent>{}</intent>", escape_xml_text(intent));
    println!("<strategy>");
    println!("  <step>Use this intake as the first context read.</step>");
    println!("  <step>Start from task_packet likely_files and owner_chunks.</step>");
    println!(
        "  <step>Drill down with anchor context when the packet names a file but not the exact owner.</step>"
    );
    println!(
        "  <step>Legacy packet: use as optional context only; current core flow is execution contract, scoped patch, verification, and receipt.</step>"
    );
    println!("</strategy>");
    println!("<scoped_files>{scoped_files}</scoped_files>");

    println!(
        "<ranked_symbols count=\"{}\" shown=\"{}\">",
        candidates.len(),
        context_limit.min(candidates.len())
    );
    for sym in candidates {
        println!(
            "  <symbol name=\"{}\" kind=\"{}\" file=\"{}\" line=\"{}\"/>",
            escape_xml_text(&sym.name),
            escape_xml_text(&sym.kind),
            escape_xml_text(&sym.path),
            sym.line_start
        );
    }
    println!("</ranked_symbols>");

    print_task_packet(packet, &task_packet_path(store));
    let (emitted, shown_paths) =
        print_task_intake_context(store, symbol_index, call_index, candidates, context_limit, packet)?;
    print_task_file_sections(
        history_index,
        related_files,
        historical_files,
        likely_tests,
        likely_test_count,
        historical_tests,
        &shown_paths,
    );
    println!("<context_symbols>{emitted}</context_symbols>");
    println!("</task_intake>");
    Ok(())
}

fn print_task_intake_context(
    store: &AnchorStore,
    symbol_index: &SymbolIndex,
    call_index: &CallIndex,
    candidates: &[SymbolEntry],
    context_limit: usize,
    packet: &TaskPacket,
) -> Result<(usize, std::collections::BTreeSet<String>)> {
    println!("<context>");
    let mut emitted = 0usize;
    let mut shown_paths = std::collections::BTreeSet::new();
    for sym in candidates.iter().take(context_limit) {
        let Ok(proj) = store.create_projection(sym) else {
            continue;
        };
        emitted += 1;
        shown_paths.insert(sym.path.clone());
        let call_lines = store.call_lines_for_symbol(sym);
        let sliced = slice_code(&proj.text, &call_lines, sym.line_start);
        let callers = call_index.callers_of(&sym.name);
        let callees = call_index.callees_of(&sym.name);

        println!("<symbol>");
        println!("<name>{}</name>", escape_xml_text(&sym.name));
        println!("<kind>{}</kind>", escape_xml_text(&sym.kind));
        println!("<file>{}</file>", escape_xml_text(&sym.path));
        println!("<line>{}</line>", sym.line_start);
        println!("<file_hash>{}</file_hash>", sym.source_hash);
        print_call_links("called_by", &callers);
        print_call_links("calls", &callees);
        print_task_symbol_code(sym, &sliced, packet);
        print_constructor_child_context(store, &symbol_index.symbols, sym)?;
        println!("</symbol>");
        record_context_read(store, sym, "ok", Some("task_intake".to_string()));
    }
    println!("</context>");
    Ok((emitted, shown_paths))
}

fn print_call_links(tag: &str, names: &[&str]) {
    if names.is_empty() {
        return;
    }
    println!(
        "<{tag}>{}</{tag}>",
        escape_xml_text(&names.iter().take(6).cloned().collect::<Vec<_>>().join(", "))
    );
}

fn print_task_symbol_code(
    sym: &SymbolEntry,
    sliced: &anchor::query::slice::SliceResult,
    packet: &TaskPacket,
) {
    if packet_has_owner_chunk(packet, sym) {
        println!(
            "<owner_chunk_ref file=\"{}\" symbol=\"{}\" line=\"{}\"/>",
            escape_xml_text(&sym.path),
            escape_xml_text(&sym.name),
            sym.line_start
        );
        return;
    }
    println!("<code>");
    if sliced.was_sliced {
        println!(
            "[{}/{} lines, {} calls]",
            sliced.shown_lines, sliced.total_lines, sliced.call_count
        );
        print_bounded_numbered_code(&sliced.code, false);
    } else {
        print_bounded_plain_code(&sliced.code, sym.line_start, false);
    }
    println!("</code>");
}
