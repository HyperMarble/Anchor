fn cmd_receipt(root: &Path) -> Result<()> {
    let store = open_store(root)?;
    let events = events::load(store.anchor_root())?;
    let summary = execution_summary(root, &events);
    let quality = summary.quality_profile();
    let receipt = serde_json::json!({
        "schema": "anchor.receipt.v1",
        "repo_root": store.repo_root().to_string_lossy(),
        "event_log": events::log_path(store.anchor_root()).to_string_lossy(),
        "summary": summary,
        "quality": quality,
    });

    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}

struct HandoffBlocker {
    reason: &'static str,
    message: &'static str,
}

struct HandoffState {
    ready: bool,
    blockers: Vec<HandoffBlocker>,
}

fn handoff_state(summary: &events::EventSummary) -> HandoffState {
    let changed = summary.edits_ok + summary.writes_ok + summary.unrecorded_changed_files > 0;
    let checked = summary.checks_ok + summary.checks_failed > 0;
    let mut blockers = Vec::new();

    if changed && summary.context_reads == 0 {
        blockers.push(HandoffBlocker {
            reason: "missing_context",
            message: "changed files without recorded Anchor context",
        });
    }
    if summary.unresolved_edits_without_file_context > 0 {
        blockers.push(HandoffBlocker {
            reason: "edited_file_without_prior_context",
            message: "some edited files have no later Anchor context read",
        });
    }
    if changed && !checked {
        blockers.push(HandoffBlocker {
            reason: "missing_check",
            message: "changed files without any recorded Anchor check",
        });
    }
    if changed && checked && summary.test_checks_ok + summary.test_checks_failed == 0 {
        blockers.push(HandoffBlocker {
            reason: "missing_test_check",
            message: "changed files without a test-like Anchor check",
        });
    }
    if summary.unresolved_checks_failed > 0 {
        blockers.push(HandoffBlocker {
            reason: "unresolved_failed_check",
            message: "at least one check command still has a failing latest result",
        });
    }
    if summary.unresolved_errors > 0 {
        blockers.push(HandoffBlocker {
            reason: "execution_error",
            message: "at least one Anchor-recorded operation error is unresolved",
        });
    }
    if summary.unresolved_stale_write_blocks > 0 {
        blockers.push(HandoffBlocker {
            reason: "stale_write_blocked",
            message: "at least one stale write block is unresolved",
        });
    }
    if summary.unrecorded_changed_files > 0 {
        blockers.push(HandoffBlocker {
            reason: "unrecorded_changed_files",
            message: "repo has changed files not recorded through Anchor writes",
        });
    }

    HandoffState {
        ready: blockers.is_empty(),
        blockers,
    }
}

fn cmd_gate(root: &Path, min_score: u8) -> Result<()> {
    let store = open_store(root)?;
    let events = events::load(store.anchor_root())?;
    let summary = execution_summary(root, &events);
    let quality = summary.quality_profile();
    let handoff = handoff_state(&summary);

    println!("<gate>");
    println!("<score>{}</score>", quality.score);
    println!("<min_score>{min_score}</min_score>");
    println!("<risk>{}</risk>", quality.risk);
    println!("<flags>{}</flags>", quality.flags.len());
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

    // No receipt, no merge: every changed file must have a recorded Anchor
    // write event, or the change is untracked work the receipt cannot vouch for.
    if summary.unrecorded_changed_files > 0 {
        println!(
            "<unreceipted_files>{}</unreceipted_files>",
            summary.unrecorded_changed_files
        );
        for path in &summary.unrecorded_changed_file_list {
            println!(
                "  <unreceipted_file>{}</unreceipted_file>",
                escape_xml_text(path)
            );
        }
        println!("<status>failed</status>");
        println!("</gate>");
        bail!(
            "receipt gate failed: {} changed file(s) have no recorded write event",
            summary.unrecorded_changed_files
        );
    }

    if handoff.ready && quality.score >= min_score {
        println!("<status>ok</status>");
        println!("</gate>");
        Ok(())
    } else {
        println!("<status>failed</status>");
        println!("</gate>");
        if !handoff.ready {
            bail!("handoff gate failed: unresolved blockers remain");
        }
        bail!(
            "quality gate failed: score {} below {}",
            quality.score,
            min_score
        )
    }
}

