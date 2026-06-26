impl EventSummary {
    fn record_write_quality(
        &mut self,
        event: &ExecutionEvent,
        read_paths: &BTreeSet<(String, String)>,
        read_hashes: &BTreeMap<(String, String, String), String>,
        unresolved_context_miss_paths: &mut BTreeSet<String>,
        risky_paths: &mut BTreeSet<String>,
    ) {
        let Some(path) = &event.path else {
            return;
        };

        if event.meta.get("expected_hash_source").map(String::as_str) != Some("none") {
            self.guarded_writes += 1;
        }

        let exact_key = (
            event.session_id.clone(),
            event.agent_id.clone(),
            path.clone(),
        );
        let session_key = (event.session_id.clone(), path.clone());
        if !read_hashes.contains_key(&exact_key) && !read_paths.contains(&session_key) {
            self.edits_without_file_context += 1;
            unresolved_context_miss_paths.insert(path.clone());
        }

        if let Some(new_changed_lines) = event
            .meta
            .get("new_changed_lines")
            .and_then(|value| value.parse::<usize>().ok())
        {
            self.changed_line_total += new_changed_lines;
            self.max_changed_lines = self.max_changed_lines.max(new_changed_lines);
            if new_changed_lines > 150 {
                self.oversized_edits += 1;
            }
        }

        if is_risky_path(path) {
            risky_paths.insert(path.clone());
        }
    }

    fn record_check_quality(
        &mut self,
        event: &ExecutionEvent,
        check_commands: &mut BTreeSet<String>,
        latest_check_status: &mut BTreeMap<String, (String, String)>,
        check_target_paths: &mut BTreeSet<String>,
    ) {
        let command = event
            .meta
            .get("command")
            .cloned()
            .or_else(|| {
                event.message.as_ref().and_then(|message| {
                    message
                        .strip_prefix("exit=")
                        .and_then(|msg| msg.split_once(" cmd=").map(|(_, cmd)| cmd.to_string()))
                })
            })
            .unwrap_or_else(|| "<unknown>".to_string());
        let kind = event
            .meta
            .get("check_kind")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        check_commands.insert(command.clone());
        latest_check_status.insert(command, (event.status.clone(), kind));

        if let Some(targets) = event.meta.get("target_paths") {
            for path in targets
                .lines()
                .map(str::trim)
                .filter(|path| !path.is_empty())
            {
                check_target_paths.insert(path.to_string());
            }
        }
    }

    fn changed_scope_paths(&self) -> BTreeSet<String> {
        self.recorded_write_paths
            .iter()
            .chain(self.unrecorded_changed_file_list.iter())
            .filter(|path| !path.starts_with(".anchor/"))
            .cloned()
            .collect()
    }

    fn refresh_changed_scope(&mut self) {
        self.changed_file_scope_paths = self.changed_scope_paths().into_iter().collect();
        self.changed_file_scope = self.changed_file_scope_paths.len();
    }
}
