fn cmd_search(root: &Path, queries: &[String], limit: usize) -> Result<()> {
    let store = ensure_indexed_store(root)?;
    let query = queries.join(" ");
    let results = store.search_symbols_hybrid(&query, limit)?;

    println!("<results query=\"{}\" count=\"{}\">", query, results.len());
    for sym in &results {
        println!(
            "  <symbol name=\"{}\" kind=\"{}\" file=\"{}\" line=\"{}\"/>",
            sym.name, sym.kind, sym.path, sym.line_start
        );
    }
    println!("</results>");
    Ok(())
}

fn cmd_context(
    root: &Path,
    queries: &[String],
    limit: usize,
    full: bool,
    bundle: bool,
) -> Result<()> {
    use std::collections::HashSet;

    let store = ensure_indexed_store(root)?;
    let call_index = store.load_call_index();
    let mut persistent_cache = PersistentCache::load(store.anchor_root());

    // track what we've printed to avoid duplicate bundle entries
    let mut shown: HashSet<String> = HashSet::new();
    let mut bundled_callees: Vec<String> = Vec::new();

    for (i, query) in queries.iter().enumerate() {
        if i > 0 {
            println!("===");
        }
        let candidates = store.search_symbols_hybrid(query, limit)?;
        println!(
            "<results query=\"{}\" count=\"{}\">",
            query,
            candidates.len()
        );

        for sym in &candidates {
            shown.insert(sym.name.clone());

            if persistent_cache.is_hit(&sym.name, &sym.path, &sym.slice_hash) {
                record_context_read(&store, sym, "cached", None);
                println!("<symbol cached=\"true\">");
                println!("<name>{}</name>", sym.name);
                println!("<kind>{}</kind>", sym.kind);
                println!("<file>{}</file>", sym.path);
                println!("<line>{}</line>", sym.line_start);
                println!("<file_hash>{}</file_hash>", sym.source_hash);
                println!("<cache>CACHED</cache>");
                println!("</symbol>");
                continue;
            }
            persistent_cache.update(&sym.name, &sym.path, &sym.slice_hash);

            let proj = match store.create_projection(sym) {
                Ok(p) => p,
                Err(e) => {
                    events::record(
                        store.anchor_root(),
                        "context.read",
                        Some(sym.path.clone()),
                        Some(sym.name.clone()),
                        "error",
                        Some(e.to_string()),
                    );
                    continue;
                }
            };
            let call_lines = store.call_lines_for_symbol(sym);
            let sliced = if full {
                anchor::query::slice::SliceResult {
                    code: proj.text.clone(),
                    total_lines: proj.text.lines().count(),
                    shown_lines: proj.text.lines().count(),
                    call_count: call_lines.len(),
                    was_sliced: false,
                }
            } else {
                slice_code(&proj.text, &call_lines, sym.line_start)
            };
            let callers = call_index.callers_of(&sym.name);
            let callees = call_index.callees_of(&sym.name);

            println!("<symbol>");
            println!("<name>{}</name>", sym.name);
            println!("<kind>{}</kind>", sym.kind);
            println!("<file>{}</file>", sym.path);
            println!("<line>{}</line>", sym.line_start);
            println!("<file_hash>{}</file_hash>", sym.source_hash);
            if !callers.is_empty() {
                println!(
                    "<called_by>{}</called_by>",
                    callers
                        .iter()
                        .take(8)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            if !callees.is_empty() {
                println!(
                    "<calls>{}</calls>",
                    callees
                        .iter()
                        .take(8)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            println!("<code>");
            if sliced.was_sliced {
                println!(
                    "[{}/{} lines, {} calls]",
                    sliced.shown_lines, sliced.total_lines, sliced.call_count
                );
                // sliced.code already includes "{:>4}: {}\n" line numbers
                print_bounded_numbered_code(&sliced.code, full);
            } else {
                print_bounded_plain_code(&sliced.code, sym.line_start, full);
            }
            println!("</code>");
            println!("</symbol>");

            record_context_read(&store, sym, "ok", None);

            if bundle {
                for callee in callees.iter().take(8) {
                    bundled_callees.push(callee.to_string());
                }
            }
        }
        println!("</results>");
    }

    if bundle && !bundled_callees.is_empty() {
        let mut already_bundled: HashSet<String> = HashSet::new();
        let mut bundle_lines = 0usize;

        println!("--- BUNDLED ---");
        for callee in &bundled_callees {
            if !already_bundled.insert(callee.clone()) || shown.contains(callee) {
                continue;
            }
            // only bundle project-defined functions (have their own callees in our index)
            if call_index.callees_of(callee).is_empty() {
                continue;
            }
            const SKIP: &[&str] = &[
                "new",
                "from",
                "into",
                "clone",
                "default",
                "collect",
                "iter",
                "map",
                "filter",
                "unwrap",
                "expect",
                "to_string",
                "as_str",
                "as_ref",
                "drop",
            ];
            if SKIP.contains(&callee.as_str()) {
                continue;
            }
            let neighbors = match store.search_symbols_hybrid(callee, 2) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for sym in &neighbors {
                if sym.name != *callee {
                    continue;
                }
                shown.insert(sym.name.clone());
                let proj = match store.create_projection(sym) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let call_lines = store.call_lines_for_symbol(sym);
                let sliced = slice_code(&proj.text, &call_lines, sym.line_start);
                println!("{} {} {}:{}", sym.name, sym.kind, sym.path, sym.line_start);
                println!("  file_hash: {}", sym.source_hash);
                if sliced.was_sliced {
                    println!("  [{}/{} lines]", sliced.shown_lines, sliced.total_lines);
                    print_bounded_numbered_code(&sliced.code, false);
                } else {
                    print_bounded_plain_code(&sliced.code, sym.line_start, false);
                }
                record_context_read(&store, sym, "ok", Some("bundle".to_string()));
                bundle_lines += sliced.shown_lines;
                println!();
            }
        }
        eprintln!("[bundle: {} lines]", bundle_lines);
    }

    persistent_cache.save(store.anchor_root());

    Ok(())
}
