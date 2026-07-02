impl EventSummary {
    pub fn from_events(events: &[ExecutionEvent]) -> Self {
        let mut summary = Self {
            event_count: events.len(),
            ..Self::default()
        };
        let mut paths = BTreeSet::new();
        let mut recorded_write_paths = BTreeSet::new();
        let mut symbols = BTreeSet::new();
        let mut sessions = BTreeSet::new();
        let mut agents = BTreeSet::new();
        let mut risky_paths = BTreeSet::new();
        let mut raw_terminal_write_paths = BTreeSet::new();
        let mut check_commands = BTreeSet::new();
        let mut latest_check_status: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut check_target_paths = BTreeSet::new();
        let mut latest_mutation_error_status: BTreeMap<String, String> = BTreeMap::new();
        let mut unresolved_error_events = 0usize;
        let mut unresolved_stale_paths = BTreeSet::new();
        let mut unresolved_context_miss_paths = BTreeSet::new();
        let mut read_hashes: BTreeMap<(String, String, String), String> = BTreeMap::new();
        let mut read_paths: BTreeSet<(String, String)> = BTreeSet::new();

        for event in events {
            sessions.insert(event.session_id.clone());
            agents.insert(event.agent_id.clone());
            if let Some(path) = &event.path {
                paths.insert(path.clone());
            }
            if let Some(symbol) = &event.symbol {
                symbols.insert(symbol.clone());
            }
            if event.status == "error" && event.event_type != "check.run" {
                summary.errors += 1;
                if !matches!(event.event_type.as_str(), "edit.apply" | "write.apply")
                    || event.path.is_none()
                {
                    unresolved_error_events += 1;
                }
            }
            match (event.event_type.as_str(), event.status.as_str()) {
                ("context.read", "ok") => {
                    summary.context_reads += 1;
                    if let (Some(path), Some(source_hash)) =
                        (&event.path, event.meta.get("source_hash"))
                    {
                        read_paths.insert((event.session_id.clone(), path.clone()));
                        unresolved_context_miss_paths.remove(path);
                        read_hashes.insert(
                            (
                                event.session_id.clone(),
                                event.agent_id.clone(),
                                path.clone(),
                            ),
                            source_hash.clone(),
                        );
                    }
                }
                ("context.read", "cached") => {
                    summary.context_reads += 1;
                    summary.cache_hits += 1;
                    if let (Some(path), Some(source_hash)) =
                        (&event.path, event.meta.get("source_hash"))
                    {
                        read_paths.insert((event.session_id.clone(), path.clone()));
                        unresolved_context_miss_paths.remove(path);
                        read_hashes.insert(
                            (
                                event.session_id.clone(),
                                event.agent_id.clone(),
                                path.clone(),
                            ),
                            source_hash.clone(),
                        );
                    }
                }
                ("edit.apply", "ok") => {
                    summary.edits_ok += 1;
                    if let Some(path) = &event.path {
                        recorded_write_paths.insert(path.clone());
                        latest_mutation_error_status.insert(path.clone(), "ok".to_string());
                        unresolved_stale_paths.remove(path);
                    }
                    summary.record_write_quality(
                        event,
                        &read_paths,
                        &read_hashes,
                        &mut unresolved_context_miss_paths,
                        &mut risky_paths,
                    );
                }
                ("write.apply", "ok") => {
                    summary.writes_ok += 1;
                    if let Some(path) = &event.path {
                        recorded_write_paths.insert(path.clone());
                        latest_mutation_error_status.insert(path.clone(), "ok".to_string());
                        unresolved_stale_paths.remove(path);
                    }
                    summary.record_write_quality(
                        event,
                        &read_paths,
                        &read_hashes,
                        &mut unresolved_context_miss_paths,
                        &mut risky_paths,
                    );
                }
                ("check.run", "ok") => {
                    summary.checks_ok += 1;
                    summary.record_check_quality(
                        event,
                        &mut check_commands,
                        &mut latest_check_status,
                        &mut check_target_paths,
                    );
                    if event.meta.get("check_kind").map(String::as_str) == Some("test") {
                        summary.test_checks_ok += 1;
                    }
                }
                ("check.run", "error") => {
                    summary.checks_failed += 1;
                    summary.record_check_quality(
                        event,
                        &mut check_commands,
                        &mut latest_check_status,
                        &mut check_target_paths,
                    );
                    if event.meta.get("check_kind").map(String::as_str) == Some("test") {
                        summary.test_checks_failed += 1;
                    }
                }
                ("edit.apply", "error") | ("write.apply", "error") => {
                    if let Some(path) = &event.path {
                        latest_mutation_error_status.insert(path.clone(), "error".to_string());
                    }
                }
                ("lock.acquire", "blocked") => summary.lock_blocks += 1,
                ("edit.guard", "blocked") | ("write.guard", "blocked") => {
                    summary.stale_write_blocks += 1;
                    if let Some(path) = &event.path {
                        unresolved_stale_paths.insert(path.clone());
                    }
                }
                ("terminal.raw_write", "error") => {
                    summary.raw_terminal_writes += 1;
                    if let Some(path) = &event.path {
                        raw_terminal_write_paths.insert(path.clone());
                    }
                }
                _ => {}
            }
        }

        summary.recorded_write_paths = recorded_write_paths.into_iter().collect();
        summary.refresh_changed_scope();
        summary.paths = paths.into_iter().collect();
        summary.symbols = symbols.into_iter().collect();
        summary.sessions = sessions.into_iter().collect();
        summary.agents = agents.into_iter().collect();
        summary.risky_paths = risky_paths.into_iter().collect();
        summary.raw_terminal_write_paths = raw_terminal_write_paths.into_iter().collect();
        summary.check_commands = check_commands.into_iter().collect();
        let unresolved: Vec<(String, String)> = latest_check_status
            .into_iter()
            .filter_map(|(command, (status, kind))| {
                if status == "error" {
                    Some((command, kind))
                } else {
                    None
                }
            })
            .collect();
        summary.unresolved_checks_failed = unresolved.len();
        summary.unresolved_test_checks_failed =
            unresolved.iter().filter(|(_, kind)| kind == "test").count();
        summary.unresolved_check_commands =
            unresolved.into_iter().map(|(command, _)| command).collect();
        summary.check_target_paths = check_target_paths.into_iter().collect();
        summary.unresolved_errors = unresolved_error_events
            + latest_mutation_error_status
                .values()
                .filter(|status| status.as_str() == "error")
                .count();
        summary.unresolved_stale_write_blocks = unresolved_stale_paths.len();
        summary.unresolved_stale_write_paths = unresolved_stale_paths.into_iter().collect();
        summary.unresolved_edits_without_file_context = unresolved_context_miss_paths.len();
        summary.unresolved_edits_without_file_context_paths =
            unresolved_context_miss_paths.into_iter().collect();
        summary
    }
}
