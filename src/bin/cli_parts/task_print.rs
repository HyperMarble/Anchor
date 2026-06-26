fn print_task_packet(packet: &TaskPacket, path: &Path) {
    println!(
        "<task_packet schema=\"{}\" file=\"{}\">",
        escape_xml_text(&packet.schema),
        escape_xml_text(&path.display().to_string())
    );
    println!("<likely_files count=\"{}\">", packet.likely_files.len());
    for active in packet.likely_files.iter().take(6) {
        println!(
            "  <file path=\"{}\" score=\"{}\" role=\"{}\" hash=\"{}\" reasons=\"{}\"/>",
            escape_xml_text(&active.path),
            active.score,
            escape_xml_text(&active.role),
            escape_xml_text(&active.source_hash),
            escape_xml_text(&active.reasons.join(","))
        );
    }
    println!("</likely_files>");

    println!("<owner_chunks count=\"{}\">", packet.owner_chunks.len());
    for slice in packet.owner_chunks.iter().take(5) {
        println!(
            "<chunk file=\"{}\" owner=\"{}\" symbol=\"{}\" kind=\"{}\" lines=\"{}-{}\" score=\"{}\" reasons=\"{}\" tags=\"{}\">",
            escape_xml_text(&slice.path),
            escape_xml_text(&slice.owner),
            escape_xml_text(&slice.symbol),
            escape_xml_text(&slice.kind),
            slice.line_start,
            slice.line_end,
            slice.score,
            escape_xml_text(&slice.reasons.join(",")),
            escape_xml_text(&slice.responsibility_tags.join(","))
        );
        println!("<meaning>{}</meaning>", escape_xml_text(&slice.meaning));
        println!("<code>");
        print_bounded_numbered_code(&slice.code, false);
        println!("</code>");
        println!("</chunk>");
    }
    println!("</owner_chunks>");

    println!(
        "<workspace_related_files count=\"{}\">",
        packet.related_files.len()
    );
    for related in packet.related_files.iter().take(6) {
        println!(
            "  <file path=\"{}\" score=\"{}\" reason=\"{}\"/>",
            escape_xml_text(&related.path),
            related.score,
            escape_xml_text(&related.reason)
        );
    }
    println!("</workspace_related_files>");

    println!("<workspace_tests count=\"{}\">", packet.likely_tests.len());
    for test in &packet.likely_tests {
        println!(
            "  <file path=\"{}\" score=\"{}\" reasons=\"{}\"/>",
            escape_xml_text(&test.path),
            test.score,
            escape_xml_text(&test.reasons.join(","))
        );
    }
    println!("</workspace_tests>");
    print_verification_plan(&packet.verification_plan);
    println!("</task_packet>");
}

fn packet_has_owner_chunk(packet: &TaskPacket, symbol: &anchor::storage::SymbolEntry) -> bool {
    packet.owner_chunks.iter().any(|slice| {
        slice.path == symbol.path
            && slice.symbol == symbol.name
            && slice.line_start == symbol.line_start
    })
}

fn top_scored_paths(
    scores: &std::collections::BTreeMap<String, usize>,
    limit: usize,
) -> Vec<(&String, usize)> {
    let mut items: Vec<_> = scores.iter().map(|(path, score)| (path, *score)).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    items.truncate(limit);
    items
}

fn source_test_affinity_score(source_path: &str, test_path: &str) -> usize {
    let source_tokens = path_signal_tokens(source_path);
    let test_tokens = path_signal_tokens(test_path);
    let shared = source_tokens
        .iter()
        .filter(|source_token| {
            test_tokens
                .iter()
                .any(|test_token| soft_token_match(source_token, test_token))
        })
        .count();
    let mut score = shared * 600;

    if let (Some(source_stem), Some(test_stem)) = (
        normalised_file_stem(source_path),
        normalised_file_stem(test_path),
    ) {
        if source_stem == test_stem {
            score += 2400;
        }
    }

    if let (Some(source_parent), Some(test_parent)) = (
        Path::new(source_path).parent(),
        Path::new(test_path).parent(),
    ) {
        if source_parent == test_parent {
            score += 1200;
        }
    }

    score
}

fn normalised_file_stem(path: &str) -> Option<String> {
    let stem = Path::new(path).file_stem()?.to_str()?.to_ascii_lowercase();
    Some(
        stem.strip_prefix("test_")
            .unwrap_or(&stem)
            .strip_suffix("_test")
            .unwrap_or_else(|| stem.strip_suffix(".test").unwrap_or(&stem))
            .to_string(),
    )
}

