impl Emitter<'_> {
    fn fn_name(&self, node: Node) -> String {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.kind() == "identifier" || ch.kind() == "simple_identifier" {
                return self.text(ch).to_string();
            }
        }
        "unknown".to_string()
    }

    fn cpp_fn_name(&self, node: Node) -> String {
        // C++: function_definition → pointer_declarator → function_declarator → identifier
        fn find_fn_name(emitter: &Emitter, node: Node<'_>) -> Option<String> {
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.kind() == "identifier" {
                    return Some(emitter.text(ch).to_string());
                }
                if ch.kind() == "function_declarator" || ch.kind() == "pointer_declarator" {
                    if let Some(name) = find_fn_name(emitter, ch) {
                        return Some(name);
                    }
                }
            }
            None
        }
        find_fn_name(self, node).unwrap_or_else(|| "unknown".to_string())
    }

    fn fn_params(&self, node: Node) -> String {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "parameters"
                | "formal_parameters"
                | "parameter_list"
                | "function_value_parameters"
                | "method_parameters" => {
                    return self.extract_param_names(ch);
                }
                _ => {}
            }
        }
        String::new()
    }

    fn cpp_fn_params(&self, node: Node) -> String {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.kind() == "function_declarator" {
                let mut c2 = ch.walk();
                for ch2 in ch.children(&mut c2) {
                    if ch2.kind() == "parameter_list" {
                        return self.extract_param_names(ch2);
                    }
                }
            }
        }
        String::new()
    }

    fn extract_param_names(&self, params: Node) -> String {
        let mut names = Vec::new();
        let mut c = params.walk();
        for ch in params.children(&mut c) {
            match ch.kind() {
                // Go: parameter_declaration contains identifier then type
                "parameter_declaration" => {
                    let mut c2 = ch.walk();
                    let found = ch.children(&mut c2).find(|n| n.kind() == "identifier");
                    if let Some(n) = found {
                        let name = self.text(n).to_string();
                        if !name.is_empty() {
                            names.push(name);
                        }
                    }
                }
                // Java/C#: formal_parameter, required_parameter
                "formal_parameter" | "required_parameter" | "optional_parameter" => {
                    let mut c2 = ch.walk();
                    let found = ch
                        .children(&mut c2)
                        .find(|n| n.kind() == "identifier" || n.kind() == "simple_identifier");
                    if let Some(n) = found {
                        let name = self.text(n).to_string();
                        if !name.is_empty() {
                            names.push(name);
                        }
                    }
                }
                // Swift: parameter
                "parameter" => {
                    let mut c2 = ch.walk();
                    let found = ch
                        .children(&mut c2)
                        .find(|n| n.kind() == "identifier" || n.kind() == "simple_identifier");
                    if let Some(n) = found {
                        let name = self.text(n).to_string();
                        if !name.is_empty() && name != "_" {
                            names.push(name);
                        }
                    }
                }
                "identifier" | "simple_identifier" => {
                    let name = self.text(ch).to_string();
                    if !name.is_empty() && name != "self" && name != "&self" {
                        names.push(name);
                    }
                }
                _ => {}
            }
        }
        names.join(", ")
    }

    fn if_condition(&self, node: Node) -> String {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "if" | "(" | ")" => continue,
                "block" | "statement_block" | "compound_statement" | "else_clause" | "else"
                | "then" => break,
                _ => return self.normalize_condition(self.text(ch)),
            }
        }
        "condition".to_string()
    }

    fn normalize_condition(&self, raw: &str) -> String {
        raw.trim()
            // strict equality first (=== before ==)
            .replace("=== null", "is null")
            .replace("=== nil", "is null")
            .replace("=== None", "is null")
            .replace("=== undefined", "is null")
            .replace("!== null", "is not null")
            .replace("!== nil", "is not null")
            // standard equality
            .replace("== null", "is null")
            .replace("== nil", "is null")
            .replace("== None", "is null")
            .replace("== nullptr", "is null")
            .replace("!= null", "is not null")
            .replace("!= nil", "is not null")
            .replace("!= None", "is not null")
            // language-specific idioms
            .replace(".is_none()", " is null")
            .replace(".nil?", " is null")
            .replace("user!", "user")  // Swift force-unwrap
            .replace("->", ".")        // C++ arrow → dot
            .replace("nullptr", "null")
            // strip outer parens
            .trim_matches(|c: char| c == '(' || c == ')')
            .to_string()
    }

    fn return_value(&self, node: Node) -> String {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "return" | ";" => continue,
                // nil/null literals
                "nil" | "null_literal" => return "null".to_string(),
                _ => {
                    let raw = self.text(ch).trim().to_string();
                    if raw.is_empty() {
                        continue;
                    }
                    return raw
                        .replace("None", "null")
                        .replace("nil", "null")
                        .replace("nullptr", "null")
                        .replace("Some(user)", "user")
                        .replace("user!", "user");
                }
            }
        }
        "".to_string()
    }

    fn ruby_return_value(&self, node: Node) -> String {
        // Ruby return: "return" keyword node, then argument_list child
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.kind() == "argument_list" {
                let mut c2 = ch.walk();
                for inner in ch.children(&mut c2) {
                    let t = self.text(inner);
                    if t != "(" && t != ")" {
                        return t.replace("nil", "null").to_string();
                    }
                }
            }
        }
        String::new()
    }

    fn extract_swift_assignment(&self, node: Node) -> Option<String> {
        // property_declaration: value_binding_pattern, pattern(simple_identifier), =, value
        let mut c = node.walk();
        let children: Vec<_> = node.children(&mut c).collect();
        let name = children
            .iter()
            .find(|n| n.kind() == "pattern")
            .and_then(|n| {
                let mut c2 = n.walk();
                let found = n
                    .children(&mut c2)
                    .find(|ch| ch.kind() == "simple_identifier")
                    .map(|ch| self.text(ch).to_string());
                found
            })?;
        let value = children
            .iter()
            .filter(|n| {
                n.kind() != "value_binding_pattern" && n.kind() != "pattern" && n.kind() != "="
            })
            .map(|n| self.text(*n).trim().to_string())
            .find(|s| !s.is_empty())?;
        Some(format!("{} = {}", name, value))
    }

    fn extract_js_assignment(&self, node: Node) -> Option<String> {
        // variable_declaration → variable_declarator → (identifier, initializer)
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.kind() == "variable_declarator" {
                let mut c2 = ch.walk();
                let children: Vec<_> = ch.children(&mut c2).collect();
                let name = children
                    .iter()
                    .find(|n| n.kind() == "identifier")
                    .map(|n| self.text(*n).to_string())?;
                let value = children
                    .iter()
                    .find(|n| {
                        n.kind() != "identifier" && n.kind() != "=" && !n.kind().starts_with('"')
                    })
                    .map(|n| self.text(*n).trim().to_string())
                    .unwrap_or_default();
                return Some(format!("{} = {}", name, value));
            }
        }
        None
    }

    fn extract_assignment(&self, node: Node) -> Option<String> {
        let text = self.text(node);
        // Simple: strip type annotations and language keywords
        let clean = text.trim().trim_end_matches(';').trim().to_string();

        // Strip leading keywords: let, const, var, final, auto
        let clean = strip_leading(
            &clean,
            &["let mut ", "let ", "const ", "var ", "final ", "auto "],
        );

        // Strip type annotations: `: Type` before `=`
        let clean = strip_type_annotation(&clean);

        // Normalize := (Go short var)
        let clean = clean.replace(":=", "=");

        if clean.contains('=') {
            Some(clean)
        } else {
            None
        }
    }
}
