fn print_query_report(report: &QueryReport) {
    println!(
        "anchor query\nintent: {}\nschema: {} scoped_files: {}",
        report.intent, report.schema, report.scoped_files
    );
    println!("\nowner chunks:");
    for chunk in &report.chunks {
        println!(
            "  {} {} {}:{}-{} kind:{} score:{} hash:{}",
            chunk.handle,
            chunk.path,
            chunk.symbol,
            chunk.line_start,
            chunk.line_end,
            chunk.kind,
            chunk.score,
            chunk.source_hash
        );
        if !chunk.reasons.is_empty() {
            println!("    reasons: {}", chunk.reasons.join(","));
        }
        if !chunk.calls.is_empty() {
            println!("    calls: {}", chunk.calls.join(", "));
        }
        if !chunk.called_by.is_empty() {
            println!("    called_by: {}", chunk.called_by.join(", "));
        }
    }
    if report.chunks.is_empty() {
        println!("  (none)");
    }
    println!("\nlikely tests:");
    for test in &report.tests {
        println!(
            "  {} {} score:{} reasons:{}",
            test.handle,
            test.path,
            test.score,
            test.reasons.join(",")
        );
    }
    if report.tests.is_empty() {
        println!("  (none)");
    }
    println!("\nfile handles:");
    for file in &report.files {
        println!(
            "  {} {} score:{} hash:{} reason:{}",
            file.handle, file.path, file.score, file.source_hash, file.reason
        );
    }
    if report.files.is_empty() {
        println!("  (none)");
    }
    println!("\nnext:");
    for next in &report.next {
        println!("  - {next}");
    }
}

fn print_view_report(report: &ViewReport) {
    println!("anchor view");
    println!("handle: {}", report.handle);
    println!(
        "kind: {} path: {} lines: {}-{}",
        report.kind, report.path, report.line_start, report.line_end
    );
    println!("source_hash: {}", report.source_hash);
    println!("slice_hash: {}", report.slice_hash);
    println!("refreshed: {}", report.refreshed);
    if let Some(symbol) = &report.symbol {
        println!("symbol: {symbol}");
    }
    println!("\ncode:");
    print!("{}", report.code);
}
