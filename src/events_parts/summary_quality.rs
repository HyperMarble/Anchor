impl EventSummary {
    pub fn with_unrecorded_repo_changes(mut self, changed_paths: Vec<String>) -> Self {
        let recorded_writes: BTreeSet<String> = self
            .recorded_write_paths
            .iter()
            .filter(|path| !path.starts_with(".anchor/"))
            .cloned()
            .collect();
        let unrecorded: Vec<String> = changed_paths
            .into_iter()
            .filter(|path| !recorded_writes.contains(path))
            .collect();
        self.unrecorded_changed_files = unrecorded.len();
        self.unrecorded_changed_file_list = unrecorded;
        self.refresh_changed_scope();
        self
    }

    pub fn quality_profile(&self) -> QualityProfile {
        let mut score: i32 = 100;
        let mut flags = Vec::new();
        let mut recommendations = BTreeSet::new();
        let changed = self.edits_ok + self.writes_ok > 0;
        let checked = self.checks_ok + self.checks_failed > 0;

        if changed && self.context_reads == 0 {
            score -= 30;
            flags.push("changed_without_recorded_context".to_string());
            recommendations.insert(
                "read relevant context through anchor context/task before editing".to_string(),
            );
        }
        if self.unresolved_edits_without_file_context > 0 {
            score -= 20;
            flags.push("edited_file_without_prior_context".to_string());
            recommendations
                .insert("reread each edited file through Anchor before continuing".to_string());
        }
        if changed && !checked {
            score -= 25;
            flags.push("changed_without_recorded_check".to_string());
            recommendations
                .insert("run a relevant verification command through anchor check".to_string());
        }
        if changed && checked && self.test_checks_ok + self.test_checks_failed == 0 {
            score -= 15;
            flags.push("changed_without_test_check".to_string());
            recommendations.insert(
                "run at least one focused test-like command through anchor check before handoff"
                    .to_string(),
            );
        }
        if self.unresolved_checks_failed > 0 {
            score -= 30;
            flags.push("unresolved_failed_check".to_string());
            recommendations.insert("fix or rerun failing checks before handoff".to_string());
        }
        if self.unresolved_errors > 0 {
            score -= 20;
            flags.push("execution_error".to_string());
            recommendations.insert("resolve recorded execution errors before handoff".to_string());
        }
        if self.lock_blocks > 0 {
            score -= 5;
            flags.push("lock_conflict_seen".to_string());
            recommendations
                .insert("coordinate ownership before editing blocked symbols/files".to_string());
        }
        if self.unresolved_stale_write_blocks > 0 {
            score -= 10;
            flags.push("stale_write_blocked".to_string());
            recommendations.insert("reread stale files and retry from fresh context".to_string());
        }
        if self.changed_scope_paths().len() > 3 {
            score -= 10;
            flags.push("broad_file_scope".to_string());
            recommendations
                .insert("reduce patch scope or split the work into smaller tasks".to_string());
        }
        if self.oversized_edits > 0 {
            score -= 15;
            flags.push("oversized_edit_scope".to_string());
            recommendations
                .insert("review large changed ranges and split unrelated edits".to_string());
        }
        if self.raw_terminal_writes > 0 {
            score -= 25;
            flags.push("raw_terminal_write".to_string());
            recommendations.insert(
                "rerun mutating terminal work through Anchor-controlled writes".to_string(),
            );
        }
        if self.unrecorded_changed_files > 0 {
            score -= 25;
            flags.push("unrecorded_repo_changes".to_string());
            recommendations.insert(
                "review changed files against the execution contract and inspect raw terminal writes"
                    .to_string(),
            );
        }
        if !self.risky_paths.is_empty() && !checked {
            score -= 10;
            flags.push("risky_path_changed_without_check".to_string());
            recommendations.insert(
                "run focused checks for risky files such as auth, billing, config, or migrations"
                    .to_string(),
            );
        }

        let score = score.clamp(0, 100) as u8;
        let risk = if score >= 85 {
            "low"
        } else if score >= 60 {
            "medium"
        } else {
            "high"
        }
        .to_string();

        QualityProfile {
            score,
            risk,
            flags,
            recommendations: recommendations.into_iter().collect(),
        }
    }
}
