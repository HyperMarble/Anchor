fn print_query_report(report: &QueryReport) {
    println!(
        "<query schema=\"{}\" intent=\"{}\" scoped_files=\"{}\">",
        report.schema,
        escape_xml_text(&report.intent),
        report.scoped_files
    );
    println!("<files count=\"{}\">", report.files.len());
    for file in &report.files {
        println!(
            "  <file handle=\"{}\" path=\"{}\" score=\"{}\" hash=\"{}\" reason=\"{}\"/>",
            escape_xml_text(&file.handle),
            escape_xml_text(&file.path),
            file.score,
            escape_xml_text(&file.source_hash),
            escape_xml_text(&file.reason)
        );
    }
    println!("</files>");
    println!("<chunks count=\"{}\">", report.chunks.len());
    for chunk in &report.chunks {
        println!(
            "  <chunk handle=\"{}\" path=\"{}\" symbol=\"{}\" kind=\"{}\" lines=\"{}-{}\" score=\"{}\" hash=\"{}\">",
            escape_xml_text(&chunk.handle),
            escape_xml_text(&chunk.path),
            escape_xml_text(&chunk.symbol),
            escape_xml_text(&chunk.kind),
            chunk.line_start,
            chunk.line_end,
            chunk.score,
            escape_xml_text(&chunk.source_hash)
        );
        if !chunk.reasons.is_empty() {
            println!(
                "    <reasons>{}</reasons>",
                escape_xml_text(&chunk.reasons.join(","))
            );
        }
        if !chunk.calls.is_empty() {
            println!("    <calls>{}</calls>", escape_xml_text(&chunk.calls.join(", ")));
        }
        if !chunk.called_by.is_empty() {
            println!(
                "    <called_by>{}</called_by>",
                escape_xml_text(&chunk.called_by.join(", "))
            );
        }
        println!("  </chunk>");
    }
    println!("</chunks>");
    println!("<tests count=\"{}\">", report.tests.len());
    for test in &report.tests {
        println!(
            "  <test handle=\"{}\" path=\"{}\" score=\"{}\" reasons=\"{}\"/>",
            escape_xml_text(&test.handle),
            escape_xml_text(&test.path),
            test.score,
            escape_xml_text(&test.reasons.join(","))
        );
    }
    println!("</tests>");
    println!("<next>");
    for next in &report.next {
        println!("  <step>{}</step>", escape_xml_text(next));
    }
    println!("</next>");
    println!("</query>");
}

fn print_view_report(report: &ViewReport) {
    println!(
        "<view schema=\"{}\" handle=\"{}\" kind=\"{}\" path=\"{}\" lines=\"{}-{}\" hash=\"{}\" slice_hash=\"{}\" refreshed=\"{}\">",
        report.schema,
        escape_xml_text(&report.handle),
        escape_xml_text(&report.kind),
        escape_xml_text(&report.path),
        report.line_start,
        report.line_end,
        escape_xml_text(&report.source_hash),
        escape_xml_text(&report.slice_hash),
        if report.refreshed { "true" } else { "false" }
    );
    if let Some(symbol) = &report.symbol {
        println!("  <symbol>{}</symbol>", escape_xml_text(symbol));
    }
    println!("<code>");
    print!("{}", report.code);
    println!("</code>");
    println!("</view>");
}
