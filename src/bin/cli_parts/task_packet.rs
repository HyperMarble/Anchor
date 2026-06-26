fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = std::collections::BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn build_task_packet(
    intent: &str,
    slices: &[TaskSlice],
    related_files: &std::collections::BTreeSet<String>,
    historical_files: &std::collections::BTreeMap<String, usize>,
    likely_tests: &[(&String, usize)],
    verification_plan: &TaskVerificationPlan,
    file_hashes: &std::collections::BTreeMap<&str, &str>,
) -> TaskPacket {
    let mut path_scores: std::collections::BTreeMap<String, (i32, String)> =
        std::collections::BTreeMap::new();
    let mut path_reasons: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut path_hashes: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut slice_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();

    let mut owner_chunks = select_diverse_task_slices(slices, 12);

    for slice in &owner_chunks {
        path_hashes.insert(slice.path.clone(), slice.source_hash.clone());
        let selected_count = slice_counts.entry(slice.path.clone()).or_default();
        let contribution = match *selected_count {
            0 => slice.score.max(0),
            1 => slice.score.max(0) / 2,
            _ => slice.score.max(0) / 4,
        };
        *selected_count += 1;
        let entry = path_scores
            .entry(slice.path.clone())
            .or_insert((0, "source".to_string()));
        entry.0 += contribution;
        let reasons = path_reasons.entry(slice.path.clone()).or_default();
        reasons.push("owner_chunk".to_string());
        reasons.extend(slice.reasons.iter().take(4).cloned());
    }

    for path in related_files {
        if let Some(entry) = path_scores.get_mut(path) {
            entry.0 += 40;
            if entry.1 == "source" {
                entry.1 = "source+related".to_string();
            }
            path_reasons
                .entry(path.clone())
                .or_default()
                .push("related_symbol_or_call".to_string());
        }
    }

    for (path, score) in historical_files {
        if let Some(entry) = path_scores.get_mut(path) {
            entry.0 += (*score).min(500) as i32;
            entry.1 = format!("{}+history", entry.1);
            path_reasons
                .entry(path.clone())
                .or_default()
                .push("git_cochange".to_string());
        } else if *score >= 50 && !looks_like_test_path(path) {
            // Files that historically co-change with the seed files belong in
            // the working set even when no slice matched the intent: registry
            // and wiring files rarely contain the task's keywords but are
            // edited in almost every related commit.
            if let Some(hash) = file_hashes.get(path.as_str()) {
                path_hashes.insert(path.clone(), (*hash).to_string());
                path_scores.insert(
                    path.clone(),
                    ((*score).min(500) as i32, "history".to_string()),
                );
                path_reasons
                    .entry(path.clone())
                    .or_default()
                    .push("git_cochange".to_string());
            }
        }
    }

    let mut likely_files: Vec<TaskPath> = path_scores
        .into_iter()
        .filter_map(|(path, (score, role))| {
            let mut reasons = path_reasons.remove(&path).unwrap_or_default();
            dedupe_strings(&mut reasons);
            path_hashes.get(&path).map(|source_hash| TaskPath {
                path,
                source_hash: source_hash.clone(),
                score,
                role,
                reasons,
            })
        })
        .collect();
    likely_files.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    likely_files.truncate(8);
    let active_path_set: std::collections::BTreeSet<&str> =
        likely_files.iter().map(|path| path.path.as_str()).collect();

    let mut workspace_related_files: Vec<TaskRelatedFile> = related_files
        .iter()
        .filter(|path| !looks_like_test_path(path))
        .filter(|path| !active_path_set.contains(path.as_str()))
        .map(|path| {
            let history_score = historical_files.get(path).copied().unwrap_or_default();
            TaskRelatedFile {
                path: path.clone(),
                score: 40 + history_score.min(500),
                reason: if history_score > 0 {
                    "related+history".to_string()
                } else {
                    "related".to_string()
                },
            }
        })
        .collect();
    workspace_related_files.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    workspace_related_files.truncate(12);

    TaskPacket {
        schema: TASK_PACKET_SCHEMA.to_string(),
        intent: intent.to_string(),
        likely_files,
        owner_chunks: {
            owner_chunks.truncate(12);
            owner_chunks
        },
        related_files: workspace_related_files,
        likely_tests: likely_tests
            .iter()
            .take(6)
            .map(|(path, score)| TaskTest {
                path: (*path).clone(),
                score: *score,
                reasons: vec!["path_or_history_affinity".to_string()],
            })
            .collect(),
        verification_plan: verification_plan.clone(),
    }
}

