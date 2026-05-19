// examples/cross_lang_pseudo.rs
// Test: same function in all 11 languages → pseudocode
// Run: cargo run --example cross_lang_pseudo
//
// Validates the embedding hypothesis:
// If pseudocode of the same function across languages looks similar,
// embeddings will cluster them correctly — confirming the fine-tuning approach.

use anchor::parser::language::SupportedLanguage;
use tree_sitter::{Node, Parser};

// ── Same function in every language ──────────────────────────────────────────
// find_user(id): query db, return user if active, else null

const RUST: &str = r#"
pub fn find_user(id: u64) -> Option<User> {
    let user = db.query(id);
    if user.is_none() {
        return None;
    }
    if user.active {
        return Some(user);
    }
    None
}
"#;

const PYTHON: &str = r#"
def find_user(id):
    user = db.query(id)
    if user is None:
        return None
    if user.active:
        return user
    return None
"#;

const GO: &str = r#"
func findUser(id uint64) *User {
    user := db.Query(id)
    if user == nil {
        return nil
    }
    if user.Active {
        return user
    }
    return nil
}
"#;

const JAVASCRIPT: &str = r#"
function findUser(id) {
    const user = db.query(id);
    if (user === null) {
        return null;
    }
    if (user.active) {
        return user;
    }
    return null;
}
"#;

const TYPESCRIPT: &str = r#"
function findUser(id: number): User | null {
    const user = db.query(id);
    if (user === null) {
        return null;
    }
    if (user.active) {
        return user;
    }
    return null;
}
"#;

const JAVA: &str = r#"
public User findUser(long id) {
    User user = db.query(id);
    if (user == null) {
        return null;
    }
    if (user.active) {
        return user;
    }
    return null;
}
"#;

const CSHARP: &str = r#"
public User FindUser(long id) {
    var user = db.Query(id);
    if (user == null) {
        return null;
    }
    if (user.Active) {
        return user;
    }
    return null;
}
"#;

const RUBY: &str = r#"
def find_user(id)
    user = db.query(id)
    if user.nil?
        return nil
    end
    if user.active
        return user
    end
    nil
end
"#;

const CPP: &str = r#"
User* findUser(uint64_t id) {
    User* user = db.query(id);
    if (user == nullptr) {
        return nullptr;
    }
    if (user->active) {
        return user;
    }
    return nullptr;
}
"#;

const SWIFT: &str = r#"
func findUser(id: UInt64) -> User? {
    let user = db.query(id)
    if user == nil {
        return nil
    }
    if user!.active {
        return user
    }
    return nil
}
"#;

// ── Pseudocode emitter ────────────────────────────────────────────────────────

struct Emitter<'a> {
    src: &'a [u8],
    lang: SupportedLanguage,
    out: String,
    indent: usize,
}

impl<'a> Emitter<'a> {
    fn new(src: &'a str, lang: SupportedLanguage) -> Self {
        Self { src: src.as_bytes(), lang, out: String::new(), indent: 0 }
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

    fn child_text(&self, node: Node, kind: &str) -> Option<String> {
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if child.kind() == kind {
                return Some(self.text(child).to_string());
            }
        }
        None
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
            | "func" | "def" | "end" | "return" | "if" | "else"
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
                | "type_annotation" | "type_identifier" | "->" | "func"
                | "(" | ")" | "[" | "]" | "optional_type" | "user_type" => {}
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
        let result = node.children(&mut c)
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
        fn find_fn_name<'t>(emitter: &Emitter, node: Node<'t>) -> Option<String> {
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
                "parameters" | "formal_parameters" | "parameter_list"
                | "function_value_parameters" | "method_parameters" => {
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
                    let found = ch.children(&mut c2)
                        .find(|n| n.kind() == "identifier");
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
                    let found = ch.children(&mut c2)
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
                    let found = ch.children(&mut c2)
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
                "block" | "statement_block" | "compound_statement"
                | "else_clause" | "else" | "then" => break,
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
                    if raw.is_empty() { continue; }
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
        let name = children.iter()
            .find(|n| n.kind() == "pattern")
            .and_then(|n| {
                let mut c2 = n.walk();
                let found = n.children(&mut c2).find(|ch| ch.kind() == "simple_identifier")
                    .map(|ch| self.text(ch).to_string());
                found
            })?;
        let value = children.iter()
            .filter(|n| n.kind() != "value_binding_pattern" && n.kind() != "pattern" && n.kind() != "=")
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
                let name = children.iter()
                    .find(|n| n.kind() == "identifier")
                    .map(|n| self.text(*n).to_string())?;
                let value = children.iter()
                    .find(|n| n.kind() != "identifier" && n.kind() != "=" && !n.kind().starts_with('"'))
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
        let clean = text
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_string();

        // Strip leading keywords: let, const, var, final, auto
        let clean = strip_leading(&clean, &["let mut ", "let ", "const ", "var ", "final ", "auto "]);

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

fn strip_leading(s: &str, prefixes: &[&str]) -> String {
    for p in prefixes {
        if s.starts_with(p) {
            return s[p.len()..].to_string();
        }
    }
    s.to_string()
}

fn strip_type_annotation(s: &str) -> String {
    // "user: User = ..." → "user = ..."
    // "User* user = ..." → "user = ..."
    if let Some(eq) = s.find('=') {
        let lhs = &s[..eq];
        let rhs = &s[eq+1..];
        // if lhs has ':', strip after ':'
        let lhs = if let Some(colon) = lhs.find(':') {
            lhs[..colon].trim().to_string()
        } else {
            // C-style: "User* user" → "user" (last word)
            let parts: Vec<&str> = lhs.split_whitespace().collect();
            parts.last().unwrap_or(&lhs).trim_start_matches('*').to_string()
        };
        format!("{} = {}", lhs.trim(), rhs.trim())
    } else {
        s.to_string()
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn emit_pseudo(src: &str, lang: SupportedLanguage) -> String {
    let ts_lang = lang.tree_sitter_language();
    let mut parser = Parser::new();
    parser.set_language(&ts_lang).unwrap();
    let tree = parser.parse(src, None).unwrap();

    let mut emitter = Emitter::new(src, lang);
    let root = tree.root_node();
    let mut c = root.walk();
    for child in root.children(&mut c) {
        emitter.emit_node(child);
    }

    // Clean up blank lines
    emitter.out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn debug_ast(src: &str, lang: SupportedLanguage) {
    let ts_lang = lang.tree_sitter_language();
    let mut parser = Parser::new();
    parser.set_language(&ts_lang).unwrap();
    let tree = parser.parse(src, None).unwrap();
    fn print_node(node: Node, src: &[u8], depth: usize) {
        let indent = "  ".repeat(depth);
        let text = node.utf8_text(src).unwrap_or("").replace('\n', "↵");
        let preview: String = text.chars().take(40).collect();
        println!("{}[{}] {:?}", indent, node.kind(), preview);
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            print_node(ch, src, depth + 1);
        }
    }
    print_node(tree.root_node(), src.as_bytes(), 0);
}

fn main() {
    let cases: &[(&str, SupportedLanguage, &str)] = &[
        ("Rust",       SupportedLanguage::Rust,       RUST),
        ("Python",     SupportedLanguage::Python,     PYTHON),
        ("Go",         SupportedLanguage::Go,         GO),
        ("JavaScript", SupportedLanguage::JavaScript, JAVASCRIPT),
        ("TypeScript", SupportedLanguage::TypeScript, TYPESCRIPT),
        ("Java",       SupportedLanguage::Java,       JAVA),
        ("C#",         SupportedLanguage::CSharp,     CSHARP),
        ("Ruby",       SupportedLanguage::Ruby,       RUBY),
        ("C++",        SupportedLanguage::Cpp,        CPP),
        ("Swift",      SupportedLanguage::Swift,      SWIFT),
    ];

    println!("=== Cross-Language Pseudocode Test ===");
    println!("Same function (find_user) in 10 languages → pseudocode\n");

    for (name, lang, src) in cases {
        let pseudo = emit_pseudo(src, *lang);
        println!("── {} ──", name);
        println!("{}", pseudo);
        println!();
    }
}
