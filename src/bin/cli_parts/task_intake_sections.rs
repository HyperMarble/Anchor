fn print_task_file_sections(
    history_index: &HistoryIndex,
    related_files: &std::collections::BTreeSet<String>,
    historical_files: &std::collections::BTreeMap<String, usize>,
    likely_tests: &[(&String, usize)],
    likely_test_count: usize,
    historical_tests: &std::collections::BTreeMap<String, usize>,
    shown_paths: &std::collections::BTreeSet<String>,
) {
    println!("<related_files count=\"{}\">", related_files.len());
    for path in related_files.iter().take(20) {
        let marker = if shown_paths.contains(path) {
            " shown=\"true\""
        } else {
            ""
        };
        println!("  <file{}>{}</file>", marker, escape_xml_text(path));
    }
    println!("</related_files>");

    println!(
        "<historical_files commits_scanned=\"{}\" count=\"{}\">",
        history_index.commits_scanned,
        historical_files.len()
    );
    for (path, score) in top_scored_paths(historical_files, 12) {
        println!(
            "  <file score=\"{}\">{}</file>",
            score,
            escape_xml_text(path)
        );
    }
    println!("</historical_files>");

    println!("<likely_tests count=\"{}\">", likely_test_count);
    for (path, score) in likely_tests {
        println!(
            "  <file score=\"{}\">{}</file>",
            score,
            escape_xml_text(path)
        );
    }
    println!("</likely_tests>");

    println!("<historical_tests count=\"{}\">", historical_tests.len());
    for (path, score) in top_scored_paths(historical_tests, 8) {
        println!(
            "  <file score=\"{}\">{}</file>",
            score,
            escape_xml_text(path)
        );
    }
    println!("</historical_tests>");
}
