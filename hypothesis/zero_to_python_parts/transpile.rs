// ── Main transpile loop ───────────────────────────────────────────────────────

fn transpile(source: &str) -> String {
    let mut out   = String::new();
    let mut indent: usize = 0;
    let mut state  = State::Normal;
    let mut prev_was_block_open = false;  // for empty body → pass

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        i += 1;

        if trimmed.is_empty() { out.push('\n'); prev_was_block_open = false; continue; }

        // ── Closing brace ────────────────────────────────────────────────────
        if trimmed == "}" {
            if prev_was_block_open {
                out.push_str(&"    ".repeat(indent));
                out.push_str("pass\n");
            }
            if let State::Match { entry_indent, .. } = state {
                if indent == entry_indent {
                    // outer match brace closing
                    indent = indent.saturating_sub(1);
                    state = State::Normal;
                } else {
                    // arm body closing — stay in match state
                    indent = indent.saturating_sub(1);
                }
            } else {
                indent = indent.saturating_sub(1);
            }
            prev_was_block_open = false;
            continue;
        }

        // ── } else if ────────────────────────────────────────────────────────
        if let Some(rest) = trimmed.strip_prefix("} else if ") {
            indent = indent.saturating_sub(1);
            let cond = transform_value(rest.trim_end_matches('{').trim());
            out.push_str(&"    ".repeat(indent));
            out.push_str(&format!("elif {}:\n", cond));
            indent += 1;
            prev_was_block_open = true;
            continue;
        }

        // ── } else { ────────────────────────────────────────────────────────
        if trimmed == "} else {" {
            indent = indent.saturating_sub(1);
            out.push_str(&"    ".repeat(indent));
            out.push_str("else:\n");
            indent += 1;
            prev_was_block_open = true;
            continue;
        }

        // ── } rescue ────────────────────────────────────────────────────────
        if let Some(rest) = trimmed.strip_prefix("} rescue") {
            indent = indent.saturating_sub(1);
            let rest = rest.trim().trim_end_matches('{').trim();
            if rest.is_empty() {
                out.push_str(&"    ".repeat(indent));
                out.push_str("except:\n");
            } else if let Some(as_pos) = rest.find(" as ") {
                let exc = rest[..as_pos].trim();
                let var = rest[as_pos+4..].trim();
                out.push_str(&"    ".repeat(indent));
                out.push_str(&format!("except {} as {}:\n", exc, var));
            } else {
                out.push_str(&"    ".repeat(indent));
                out.push_str(&format!("except {}:\n", rest));
            }
            indent += 1;
            prev_was_block_open = true;
            continue;
        }

        // ── match x { ───────────────────────────────────────────────────────
        if let Some(rest) = trimmed.strip_prefix("match ") {
            let var = rest.trim_end_matches('{').trim().to_string();
            state = State::Match { var, first: true, entry_indent: indent };
            prev_was_block_open = false;
            continue;
        }

        // ── match case arms: .variant => binding { ──────────────────────────
        if let State::Match { ref var, ref mut first, .. } = state {
            if trimmed.starts_with('.') && trimmed.contains(" => ") {
                let rest = &trimmed[1..]; // strip leading dot
                if let Some(arrow) = rest.find(" => ") {
                    let variant = &rest[..arrow];
                    let after   = rest[arrow+4..].trim_end_matches('{').trim();
                    let kw = if *first { "if" } else { "elif" };
                    *first = false;
                    out.push_str(&"    ".repeat(indent));
                    out.push_str(&format!("{} {}.{} is not None:\n", kw, var, variant));
                    indent += 1;
                    if !after.is_empty() && after != "_" {
                        out.push_str(&"    ".repeat(indent));
                        out.push_str(&format!("{} = {}.{}\n", after, var, variant));
                    }
                    prev_was_block_open = true;
                    continue;
                }
            }
            // wildcard: "_ {" or "_ => {"
            if trimmed.starts_with("_ ") || trimmed == "_ {" {
                indent = indent.saturating_sub(1);
                out.push_str(&"    ".repeat(indent));
                out.push_str("else:\n");
                indent += 1;
                prev_was_block_open = true;
                continue;
            }
        }

        // ── static fun → @staticmethod ───────────────────────────────────────
        let is_static = trimmed.starts_with("pub static fun ")
            || trimmed.starts_with("static fun ");
        if is_static {
            out.push_str(&"    ".repeat(indent));
            out.push_str("@staticmethod\n");
            let stripped = trimmed
                .strip_prefix("pub static fun ")
                .or_else(|| trimmed.strip_prefix("static fun "))
                .unwrap();
            let py = transform_line(&format!("fun {}", stripped));
            out.push_str(&"    ".repeat(indent));
            out.push_str(&py);
            out.push('\n');
            if py.ends_with(':') { indent += 1; }
            prev_was_block_open = true;
            continue;
        }

        // ── @annotation / decorator ─────────────────────────────────────────
        if trimmed.starts_with('@') {
            out.push_str(&"    ".repeat(indent));
            out.push_str(trimmed);
            out.push('\n');
            prev_was_block_open = false;
            continue;
        }

        // ── Normal line ──────────────────────────────────────────────────────
        let py = transform_line(trimmed);
        if py.is_empty() { prev_was_block_open = false; continue; }

        out.push_str(&"    ".repeat(indent));
        out.push_str(&py);
        out.push('\n');

        prev_was_block_open = py.ends_with(':');
        if py.ends_with(':') { indent += 1; }
    }

    out
}

// ── Test harness ──────────────────────────────────────────────────────────────

fn check(label: &str, input: &str, expected: &str) {
    let got = transpile(input).trim().to_string();
    let exp = expected.trim().to_string();
    if got == exp {
        println!("PASS  {}", label);
    } else {
        println!("FAIL  {}", label);
        let exp_lines: Vec<&str> = exp.lines().collect();
        let got_lines: Vec<&str> = got.lines().collect();
        let max = exp_lines.len().max(got_lines.len());
        for n in 0..max {
            let e = exp_lines.get(n).copied().unwrap_or("<missing>");
            let g = got_lines.get(n).copied().unwrap_or("<missing>");
            if e != g { println!("  L{}: exp {:?}", n+1, e);
                        println!("       got {:?}", g); }
        }
    }
}
