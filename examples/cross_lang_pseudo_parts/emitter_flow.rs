impl<'a> Emitter<'a> {
    fn new(src: &'a str, lang: SupportedLanguage) -> Self {
        Self {
            src: src.as_bytes(),
            lang,
            out: String::new(),
            indent: 0,
        }
    }

    fn text(&self, node: Node) -> &str {
        node.utf8_text(self.src).unwrap_or("")
    }

    fn push(&mut self, line: &str) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
        self.out.push_str(line);
        self.out.push('\n');
    }

    fn emit_node(&mut self, node: Node) {
        let kind = node.kind();

        match kind {
            // ── Function definitions ───────────────────────────────────────
            "function_item"              // Rust
            | "function_definition"      // Python / C++
            | "function_declaration"     // JS/TS/Swift/Go
            | "method_declaration"       // Java
            | "local_function_statement" // C#
            | "method" => {              // Ruby
                let (name, params) = if self.lang == SupportedLanguage::Cpp {
                    (self.cpp_fn_name(node), self.cpp_fn_params(node))
                } else {
                    (self.fn_name(node), self.fn_params(node))
                };
                self.push(&format!("function {}({}):", name, params));
                self.indent += 1;
                self.emit_body(node);
                self.indent -= 1;
            }

            // ── Conditionals ───────────────────────────────────────────────
            "if_statement" | "if_expression" | "if" => {
                let cond = self.if_condition(node);
                let has_else = self.has_else(node);
                self.push(&format!("if {}:", cond));
                self.indent += 1;
                self.emit_if_body(node);
                self.indent -= 1;
                if has_else {
                    self.push("else:");
                    self.indent += 1;
                    self.emit_else_body(node);
                    self.indent -= 1;
                }
            }

            // ── Return ─────────────────────────────────────────────────────
            "return_statement" | "return_expression" | "control_transfer_statement" => {
                let val = self.return_value(node);
                self.push(&format!("return {}", val));
            }

            // Ruby: return is a bare "return" node with argument_list child
            "return" if node.child_count() > 0 => {
                let val = self.ruby_return_value(node);
                self.push(&format!("return {}", val));
            }

            // Swift: property_declaration = variable assignment
            "property_declaration" => {
                if let Some(assign) = self.extract_swift_assignment(node) {
                    self.push(&assign);
                }
            }

            // ── Assignments ────────────────────────────────────────────────
            "let_declaration"                 // Rust
            | "assignment"                    // Ruby/Python expression
            | "local_variable_declaration"    // Java
            | "local_declaration_statement"   // C#
            | "declaration"                   // C++
            | "short_var_declaration" => {    // Go
                if let Some(assign) = self.extract_assignment(node) {
                    self.push(&assign);
                }
            }

            // JS/TS: variable_declaration wraps variable_declarator(name, value)
            "variable_declaration" | "lexical_declaration" => {
                if let Some(assign) = self.extract_js_assignment(node) {
                    self.push(&assign);
                }
            }

            // Python assignment
            "expression_statement" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.kind() == "assignment" {
                        if let Some(assign) = self.extract_assignment(ch) {
                            self.push(&assign);
                            return;
                        }
                    }
                }
                // fallthrough: emit children
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    self.emit_node(ch);
                }
            }

            // ── Blocks / bodies ────────────────────────────────────────────
            "block" | "statement_block" | "compound_statement"
            | "body_statement" | "function_body" | "do_block" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    self.emit_node(ch);
                }
            }

            // ── Skip punctuation and type noise ───────────────────────────
            "{" | "}" | "(" | ")" | ";" | "," | "->" | "=>"
            | "pub" | "fn" | "let" | "mut" | "const" | "var"
            | "func" | "def" | "end" | "return" | "else"
            | "nil" | "null" | "none" | "None" | "nullptr"
            | "comment" | "line_comment" | "block_comment" => {}

            // ── Recurse into everything else ───────────────────────────────
            _ => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    self.emit_node(ch);
                }
            }
        }
    }

    fn emit_body(&mut self, node: Node) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "block" | "statement_block" | "compound_statement" | "body_statement" => {
                    let mut c2 = ch.walk();
                    for stmt in ch.children(&mut c2) {
                        self.emit_node(stmt);
                    }
                    return;
                }
                // Swift: function_body → { statements }
                "function_body" => {
                    let mut c2 = ch.walk();
                    for inner in ch.children(&mut c2) {
                        if inner.kind() == "statements" {
                            let mut c3 = inner.walk();
                            for stmt in inner.children(&mut c3) {
                                self.emit_node(stmt);
                            }
                            return;
                        }
                    }
                    return;
                }
                _ => {}
            }
        }
        // Python/Ruby: body is inline children
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "identifier" | "simple_identifier" | "def" | "end" | "parameters"
                | "type_annotation" | "type_identifier" | "->" | "func" | "(" | ")" | "[" | "]"
                | "optional_type" | "user_type" => {}
                _ => self.emit_node(ch),
            }
        }
    }

    fn emit_if_body(&mut self, node: Node) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            match ch.kind() {
                "block" | "statement_block" | "compound_statement" | "then" => {
                    let mut c2 = ch.walk();
                    for stmt in ch.children(&mut c2) {
                        self.emit_node(stmt);
                    }
                    return;
                }
                // Swift: if_statement has { statements } directly
                "statements" => {
                    let mut c2 = ch.walk();
                    for stmt in ch.children(&mut c2) {
                        self.emit_node(stmt);
                    }
                    return;
                }
                _ => {}
            }
        }
    }

    fn has_else(&self, node: Node) -> bool {
        let mut c = node.walk();
        let result = node
            .children(&mut c)
            .any(|ch| ch.kind() == "else_clause" || ch.kind() == "else");
        result
    }

    fn emit_else_body(&mut self, node: Node) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.kind() == "else_clause" || ch.kind() == "else" {
                let mut c2 = ch.walk();
                for inner in ch.children(&mut c2) {
                    self.emit_node(inner);
                }
                return;
            }
        }
    }
}
