fn strip_leading(s: &str, prefixes: &[&str]) -> String {
    for p in prefixes {
        if let Some(stripped) = s.strip_prefix(p) {
            return stripped.to_string();
        }
    }
    s.to_string()
}

fn strip_type_annotation(s: &str) -> String {
    // "user: User = ..." → "user = ..."
    // "User* user = ..." → "user = ..."
    if let Some(eq) = s.find('=') {
        let lhs = &s[..eq];
        let rhs = &s[eq + 1..];
        // if lhs has ':', strip after ':'
        let lhs = if let Some(colon) = lhs.find(':') {
            lhs[..colon].trim().to_string()
        } else {
            // C-style: "User* user" → "user" (last word)
            let parts: Vec<&str> = lhs.split_whitespace().collect();
            parts
                .last()
                .unwrap_or(&lhs)
                .trim_start_matches('*')
                .to_string()
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
    emitter
        .out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[allow(dead_code)]
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
        ("Rust", SupportedLanguage::Rust, RUST),
        ("Python", SupportedLanguage::Python, PYTHON),
        ("Go", SupportedLanguage::Go, GO),
        ("JavaScript", SupportedLanguage::JavaScript, JAVASCRIPT),
        ("TypeScript", SupportedLanguage::TypeScript, TYPESCRIPT),
        ("Java", SupportedLanguage::Java, JAVA),
        ("C#", SupportedLanguage::CSharp, CSHARP),
        ("Ruby", SupportedLanguage::Ruby, RUBY),
        ("C++", SupportedLanguage::Cpp, CPP),
        ("Swift", SupportedLanguage::Swift, SWIFT),
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
