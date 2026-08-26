fn cmd_status(root: &Path) -> Result<()> {
    let store = open_store(root)?;
    let events = events::load(store.anchor_root())?;
    let summary = execution_summary(root, &events)?;
    let quality = summary.quality_profile();

    println!("<status>");
    println!("<events>{}</events>", summary.event_count);
    println!("<context_reads>{}</context_reads>", summary.context_reads);
    println!("<cache_hits>{}</cache_hits>", summary.cache_hits);
    println!("<edits>{}</edits>", summary.edits_ok);
    println!("<writes>{}</writes>", summary.writes_ok);
    println!("<checks_ok>{}</checks_ok>", summary.checks_ok);
    println!("<checks_failed>{}</checks_failed>", summary.checks_failed);
    println!(
        "<unresolved_checks_failed>{}</unresolved_checks_failed>",
        summary.unresolved_checks_failed
    );
    println!(
        "<test_checks_ok>{}</test_checks_ok>",
        summary.test_checks_ok
    );
    println!(
        "<test_checks_failed>{}</test_checks_failed>",
        summary.test_checks_failed
    );
    println!(
        "<unresolved_test_checks_failed>{}</unresolved_test_checks_failed>",
        summary.unresolved_test_checks_failed
    );
    println!(
        "<check_commands>{}</check_commands>",
        summary.check_commands.len()
    );
    for command in &summary.check_commands {
        println!(
            "  <check_command>{}</check_command>",
            escape_xml_text(command)
        );
    }
    println!(
        "<unresolved_check_commands>{}</unresolved_check_commands>",
        summary.unresolved_check_commands.len()
    );
    for command in &summary.unresolved_check_commands {
        println!(
            "  <unresolved_check_command>{}</unresolved_check_command>",
            escape_xml_text(command)
        );
    }
    println!(
        "<check_target_paths>{}</check_target_paths>",
        summary.check_target_paths.len()
    );
    for path in &summary.check_target_paths {
        println!("  <check_target_path>{path}</check_target_path>");
    }
    println!("<lock_blocks>{}</lock_blocks>", summary.lock_blocks);
    println!(
        "<stale_write_blocks>{}</stale_write_blocks>",
        summary.stale_write_blocks
    );
    println!(
        "<unresolved_stale_write_blocks>{}</unresolved_stale_write_blocks>",
        summary.unresolved_stale_write_blocks
    );
    for path in &summary.unresolved_stale_write_paths {
        println!("  <unresolved_stale_write_path>{path}</unresolved_stale_write_path>");
    }
    println!(
        "<guarded_writes>{}</guarded_writes>",
        summary.guarded_writes
    );
    println!(
        "<edits_without_file_context>{}</edits_without_file_context>",
        summary.edits_without_file_context
    );
    println!(
        "<unresolved_edits_without_file_context>{}</unresolved_edits_without_file_context>",
        summary.unresolved_edits_without_file_context
    );
    for path in &summary.unresolved_edits_without_file_context_paths {
        println!("  <unresolved_edit_without_context>{path}</unresolved_edit_without_context>");
    }
    println!(
        "<changed_line_total>{}</changed_line_total>",
        summary.changed_line_total
    );
    println!(
        "<max_changed_lines>{}</max_changed_lines>",
        summary.max_changed_lines
    );
    println!(
        "<oversized_edits>{}</oversized_edits>",
        summary.oversized_edits
    );
    println!(
        "<changed_file_scope>{}</changed_file_scope>",
        summary.changed_file_scope
    );
    for path in &summary.changed_file_scope_paths {
        println!("  <changed_file>{path}</changed_file>");
    }
    println!(
        "<unrecorded_changed_files>{}</unrecorded_changed_files>",
        summary.unrecorded_changed_files
    );
    for path in &summary.unrecorded_changed_file_list {
        println!("  <unrecorded_changed_file>{path}</unrecorded_changed_file>");
    }
    println!("<errors>{}</errors>", summary.errors);
    println!(
        "<unresolved_errors>{}</unresolved_errors>",
        summary.unresolved_errors
    );
    println!("<quality_score>{}</quality_score>", quality.score);
    println!("<risk>{}</risk>", quality.risk);
    println!("<quality_flags>{}</quality_flags>", quality.flags.len());
    for flag in &quality.flags {
        println!("  <flag>{flag}</flag>");
    }
    println!(
        "<recommendations>{}</recommendations>",
        quality.recommendations.len()
    );
    for recommendation in &quality.recommendations {
        println!("  <recommendation>{recommendation}</recommendation>");
    }
    let handoff = handoff_state(&summary);
    println!(
        "<handoff_ready>{}</handoff_ready>",
        if handoff.ready { "true" } else { "false" }
    );
    println!(
        "<handoff_blockers>{}</handoff_blockers>",
        handoff.blockers.len()
    );
    for blocker in &handoff.blockers {
        println!(
            "  <handoff_blocker reason=\"{}\">{}</handoff_blocker>",
            escape_xml_text(blocker.reason),
            escape_xml_text(blocker.message)
        );
    }
    println!("<paths>{}</paths>", summary.paths.len());
    for path in &summary.paths {
        println!("  <path>{path}</path>");
    }
    println!("<symbols>{}</symbols>", summary.symbols.len());
    for symbol in &summary.symbols {
        println!("  <symbol>{symbol}</symbol>");
    }
    println!("<risky_paths>{}</risky_paths>", summary.risky_paths.len());
    for path in &summary.risky_paths {
        println!("  <path>{path}</path>");
    }
    println!("<signals>");
    println!(
        "  <signal name=\"context_used\" status=\"{}\"/>",
        if summary.context_reads > 0 {
            "ok"
        } else {
            "missing"
        }
    );
    println!(
        "  <signal name=\"edits_applied\" status=\"{}\"/>",
        if summary.edits_ok + summary.writes_ok > 0 {
            "ok"
        } else {
            "missing"
        }
    );
    println!(
        "  <signal name=\"lock_conflicts\" status=\"{}\" count=\"{}\"/>",
        if summary.lock_blocks == 0 {
            "ok"
        } else {
            "blocked"
        },
        summary.lock_blocks
    );
    println!(
        "  <signal name=\"checks\" status=\"{}\" passed=\"{}\" failed=\"{}\"/>",
        if summary.unresolved_checks_failed == 0 {
            "ok"
        } else {
            "failed"
        },
        summary.checks_ok,
        summary.unresolved_checks_failed
    );
    println!(
        "  <signal name=\"errors\" status=\"{}\" count=\"{}\"/>",
        if summary.unresolved_errors == 0 {
            "ok"
        } else {
            "failed"
        },
        summary.unresolved_errors
    );
    println!("</signals>");
    println!("</status>");

    Ok(())
}

