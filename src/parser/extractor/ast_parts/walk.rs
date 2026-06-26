pub fn extract_symbols_and_calls(
    root: &Node,
    source: &[u8],
    lang: SupportedLanguage,
    file_path: &str,
) -> (Vec<ExtractedSymbol>, Vec<ExtractedCall>) {
    let mut state = ExtractState::default();
    let mut containers = Vec::new();
    let mut scopes = Vec::new();

    walk(
        root,
        source,
        lang,
        file_path,
        &mut containers,
        &mut scopes,
        &mut state,
    );

    (state.symbols, state.calls)
}

fn walk(
    node: &Node,
    source: &[u8],
    lang: SupportedLanguage,
    file_path: &str,
    containers: &mut Vec<String>,
    scopes: &mut Vec<String>,
    state: &mut ExtractState,
) {
    let container_name = container_name(node, source, lang);
    if let Some(name) = &container_name {
        containers.push(name.clone());
    }

    let symbol = symbol_from_node(node, source, lang);
    let mut pushed_scope = false;
    if let Some((name, kind)) = symbol {
        let parent = parent_for_symbol(&name, kind, containers);
        let features = generate_features(&name, kind, parent.as_deref(), file_path);

        state.symbols.push(ExtractedSymbol {
            name: name.clone(),
            kind,
            line_start: node.start_position().row + 1,
            line_end: node.end_position().row + 1,
            code_snippet: bounded_snippet(node, source),
            parent,
            features,
        });

        if is_scope(kind) {
            scopes.push(name);
            pushed_scope = true;
        }
    } else if let Some(name) = scope_name(node, source, lang) {
        scopes.push(name);
        pushed_scope = true;
    }

    if let Some(callee) = call_from_node(node, source, lang) {
        if let Some(caller) = scopes.last() {
            state.calls.push(ExtractedCall {
                callee,
                caller: caller.clone(),
                line: node.start_position().row + 1,
                line_end: node.end_position().row + 1,
            });
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk(&child, source, lang, file_path, containers, scopes, state);
        }
    }

    if pushed_scope {
        scopes.pop();
    }
    if container_name.is_some() {
        containers.pop();
    }
}

