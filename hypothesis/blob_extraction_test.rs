// Hypothesis: git blobs as universal symbol source.
// Tree-sitter handles 11 languages. Git blobs handle everything in the repo.
// Test: can we extract named chunks from ANY file type using raw blob content?

// ── helpers ─────────────────────────────────────────────────────────────────

fn extract_chunks(content: &str, file_ext: &str) -> Vec<(String, usize, usize, String)> {
    match file_ext {
        "rs" | "go" | "java" | "c" | "cpp" | "cs" => extract_brace_chunks(content),
        "py"                                        => extract_indent_chunks(content),
        "js" | "ts" | "tsx" | "jsx"                => extract_js_chunks(content),
        "csv"                                       => extract_csv_chunks(content),
        "md"                                        => extract_md_chunks(content),
        "json" | "toml" | "yaml" | "yml"           => extract_config_chunk(content, file_ext),
        _                                           => extract_generic_chunk(content),
    }
}

/// Strip line comments and string literals to avoid false brace matches.
fn sanitize_line(line: &str) -> String {
    let mut result = String::new();
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    let mut string_char = '"';

    while let Some(ch) = chars.next() {
        if in_string {
            if ch == '\\' { chars.next(); continue; } // escape
            if ch == string_char { in_string = false; }
            // don't push string contents — treat as blank
        } else {
            if ch == '"' || ch == '\'' {
                in_string = true;
                string_char = ch;
            } else if ch == '/' && chars.peek() == Some(&'/') {
                break; // rest is comment
            } else if ch == '#' {
                break; // Python/shell comment
            } else {
                result.push(ch);
            }
        }
    }
    result
}

const DECL_KEYWORDS: &[&str] = &[
    "pub async fn ", "pub fn ", "async fn ", "fn ",
    "pub struct ", "struct ",
    "pub enum ", "enum ",
    "pub impl ", "impl ",
    "pub trait ", "trait ",
    "pub mod ", "mod ",
];

/// Brace-based chunk extraction (Rust, Go, Java, C, C++, C#).
fn extract_brace_chunks(content: &str) -> Vec<(String, usize, usize, String)> {
    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        let kw = DECL_KEYWORDS.iter().find(|&&kw| trimmed.starts_with(kw));

        if let Some(kw) = kw {
            let rest = &trimmed[kw.len()..];
            let name: String = rest
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("")
                .to_string();

            if name.is_empty() { i += 1; continue; }

            let start = i;
            let mut depth = 0i32;
            let mut end = i;
            let mut found_open = false;

            for (j, line) in lines[i..].iter().enumerate() {
                let clean = sanitize_line(line);
                for ch in clean.chars() {
                    if ch == '{' { depth += 1; found_open = true; }
                    if ch == '}' { depth -= 1; }
                }
                end = i + j;
                if found_open && depth <= 0 { break; }
            }

            let body = lines[start..=end].join("\n");
            chunks.push((name, start + 1, end + 1, body));
            i = end + 1;
        } else {
            i += 1;
        }
    }
    chunks
}

/// Indent-based chunk extraction (Python).
fn extract_indent_chunks(content: &str) -> Vec<(String, usize, usize, String)> {
    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        let is_decl = trimmed.starts_with("def ")
            || trimmed.starts_with("async def ")
            || trimmed.starts_with("class ");

        if is_decl {
            let kw = if trimmed.starts_with("async def ") { "async def " }
                     else if trimmed.starts_with("def ") { "def " }
                     else { "class " };
            let name: String = trimmed[kw.len()..]
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("")
                .to_string();

            if name.is_empty() { i += 1; continue; }

            // measure indent of declaration line
            let base_indent = lines[i].len() - lines[i].trim_start().len();
            let start = i;
            let mut end = i;

            for j in 1..lines.len().saturating_sub(i) {
                let next = lines[i + j];
                let is_empty = next.trim().is_empty();
                let indent = next.len() - next.trim_start().len();
                if !is_empty && indent <= base_indent {
                    break;
                }
                end = i + j;
            }

            let body = lines[start..=end].join("\n");
            chunks.push((name, start + 1, end + 1, body));
            i = end + 1;
        } else {
            i += 1;
        }
    }
    chunks
}

/// JS/TS: handles both function declarations and arrow functions assigned to variables.
fn extract_js_chunks(content: &str) -> Vec<(String, usize, usize, String)> {
    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // named function declaration: function foo() {
        // arrow assigned: const foo = (...) => {  OR  const foo = (...) => expr
        let name: Option<String> = if trimmed.starts_with("function ") {
            let rest = &trimmed["function ".len()..];
            Some(rest.split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or("").to_string())
        } else if let Some(pos) = trimmed.find("= (") {
            // const/let/var foo = (...
            let before = trimmed[..pos].trim();
            let name = before.split_whitespace().last().unwrap_or("").to_string();
            if !name.is_empty() && trimmed.contains("=>") { Some(name) } else { None }
        } else if let Some(pos) = trimmed.find("= async (") {
            let before = trimmed[..pos].trim();
            let name = before.split_whitespace().last().unwrap_or("").to_string();
            if !name.is_empty() { Some(name) } else { None }
        } else {
            None
        };

        if let Some(name) = name {
            if name.is_empty() { i += 1; continue; }

            let start = i;
            let mut depth = 0i32;
            let mut end = i;
            let mut found_open = false;

            for (j, line) in lines[i..].iter().enumerate() {
                let clean = sanitize_line(line);
                for ch in clean.chars() {
                    if ch == '{' { depth += 1; found_open = true; }
                    if ch == '}' { depth -= 1; }
                }
                end = i + j;
                if found_open && depth <= 0 { break; }
                // arrow without braces: single expression on same line
                if j == 0 && !found_open && clean.contains("=>") { break; }
            }

            let body = lines[start..=end].join("\n");
            chunks.push((name, start + 1, end + 1, body));
            i = end + 1;
        } else {
            i += 1;
        }
    }
    chunks
}

fn extract_csv_chunks(content: &str) -> Vec<(String, usize, usize, String)> {
    let mut lines = content.lines().enumerate();
    let header = lines.next(); // skip header
    let _ = header;
    lines.filter_map(|(i, line)| {
        // handle quoted fields
        let name = if line.starts_with('"') {
            line.trim_start_matches('"').split('"').next()?.to_string()
        } else {
            line.split(',').next()?.trim().to_string()
        };
        if name.is_empty() { return None; }
        Some((name, i + 1, i + 1, line.to_string()))
    }).collect()
}

fn extract_md_chunks(content: &str) -> Vec<(String, usize, usize, String)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut chunks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with('#') {
            let name = lines[i].trim_start_matches('#').trim().to_string();
            let start = i;
            let end = lines[i+1..].iter().position(|l| l.starts_with('#'))
                .map(|p| i + p)
                .unwrap_or(lines.len() - 1);
            let body = lines[start..=end].join("\n");
            if !name.is_empty() {
                chunks.push((name, start + 1, end + 1, body));
            }
            i = end;
        }
        i += 1;
    }
    chunks
}

fn extract_config_chunk(content: &str, _ext: &str) -> Vec<(String, usize, usize, String)> {
    vec![("__config__".to_string(), 1, content.lines().count(), content.to_string())]
}

fn extract_generic_chunk(content: &str) -> Vec<(String, usize, usize, String)> {
    if content.is_empty() { return vec![]; }
    vec![("__file__".to_string(), 1, content.lines().count(), content.to_string())]
}

// ── helper ───────────────────────────────────────────────────────────────────

fn names(chunks: &[(String, usize, usize, String)]) -> Vec<&str> {
    chunks.iter().map(|(n, ..)| n.as_str()).collect()
}

// ── Rust / brace-based ───────────────────────────────────────────────────────

#[test]
fn rust_basic_functions() {
    let src = "pub fn acquire(key: LockKey) -> bool { true }\npub fn release(key: LockKey) { drop(key); }\n";
    let chunks = extract_brace_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"acquire"), "{:?}", ns);
    assert!(ns.contains(&"release"), "{:?}", ns);
}

#[test]
fn rust_struct_and_impl() {
    let src = "pub struct LockKey { pub symbol: String }\nimpl LockKey {\n    pub fn new() -> Self { Self { symbol: String::new() } }\n}\n";
    let chunks = extract_brace_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"LockKey"), "{:?}", ns);
    assert!(ns.contains(&"LockKey") || ns.contains(&"new"), "{:?}", ns);
}

#[test]
fn rust_string_with_braces_does_not_confuse_depth() {
    let src = "pub fn foo() {\n    let s = \"{ not a brace }\";\n    true\n}\npub fn bar() { false }\n";
    let chunks = extract_brace_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"foo"), "{:?}", ns);
    assert!(ns.contains(&"bar"), "{:?}", ns);
}

#[test]
fn rust_nested_closure_inside_function() {
    let src = "pub fn outer() {\n    let f = |x| {\n        x + 1\n    };\n    f(1)\n}\n";
    let chunks = extract_brace_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"outer"), "outer must be extracted: {:?}", ns);
}

#[test]
fn rust_line_comment_with_fake_fn_ignored() {
    let src = "// fn fake() {}\npub fn real() { true }\n";
    let chunks = extract_brace_chunks(src);
    let ns = names(&chunks);
    assert!(!ns.contains(&"fake"), "comment fn must not be extracted: {:?}", ns);
    assert!(ns.contains(&"real"), "{:?}", ns);
}

#[test]
fn rust_multiline_function_with_nested_blocks() {
    let src = r#"pub fn complex(x: i32) -> i32 {
    if x > 0 {
        let y = {
            x * 2
        };
        y
    } else {
        0
    }
}
pub fn after() {}
"#;
    let chunks = extract_brace_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"complex"), "{:?}", ns);
    assert!(ns.contains(&"after"), "{:?}", ns);
    // complex must not eat after
    let complex = chunks.iter().find(|(n, ..)| n == "complex").unwrap();
    let after = chunks.iter().find(|(n, ..)| n == "after").unwrap();
    assert!(complex.2 < after.1, "complex end must be before after start");
}

#[test]
fn rust_empty_function() {
    let src = "pub fn empty() {}\n";
    let chunks = extract_brace_chunks(src);
    assert!(!chunks.is_empty(), "empty function must still be extracted");
    assert_eq!(chunks[0].0, "empty");
}

#[test]
fn rust_enum_extracted() {
    let src = "pub enum Status {\n    Active,\n    Inactive,\n}\n";
    let chunks = extract_brace_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"Status"), "{:?}", ns);
}

#[test]
fn rust_macro_braces_ignored() {
    // vec![] and println! have braces in string args — should not break depth
    let src = "pub fn foo() {\n    let v = vec![1, 2, 3];\n    println!(\"{}\", v[0]);\n}\npub fn bar() {}\n";
    let chunks = extract_brace_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"foo"), "{:?}", ns);
    assert!(ns.contains(&"bar"), "{:?}", ns);
}

// ── Python / indent-based ────────────────────────────────────────────────────

#[test]
fn python_basic_functions() {
    let src = "def foo(x):\n    return x + 1\n\ndef bar(y):\n    return y * 2\n";
    let chunks = extract_indent_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"foo"), "{:?}", ns);
    assert!(ns.contains(&"bar"), "{:?}", ns);
}

#[test]
fn python_class_with_methods() {
    let src = "class MyClass:\n    def __init__(self):\n        self.x = 0\n\n    def method(self):\n        return self.x\n";
    let chunks = extract_indent_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"MyClass") || ns.contains(&"__init__") || ns.contains(&"method"), "{:?}", ns);
}

#[test]
fn python_async_function() {
    let src = "async def fetch(url):\n    return await get(url)\n\ndef sync(): pass\n";
    let chunks = extract_indent_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"fetch"), "{:?}", ns);
    assert!(ns.contains(&"sync"), "{:?}", ns);
}

#[test]
fn python_comment_not_extracted() {
    let src = "# def fake():\n#     pass\ndef real():\n    return 1\n";
    let chunks = extract_indent_chunks(src);
    let ns = names(&chunks);
    assert!(!ns.contains(&"fake"), "comment def must not be extracted: {:?}", ns);
    assert!(ns.contains(&"real"), "{:?}", ns);
}

#[test]
fn python_nested_function() {
    let src = "def outer():\n    def inner():\n        return 1\n    return inner()\n";
    let chunks = extract_indent_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"outer"), "{:?}", ns);
    // inner may or may not be extracted depending on implementation
}

// ── JavaScript / arrow functions ─────────────────────────────────────────────

#[test]
fn js_function_declaration() {
    let src = "function greet(name) {\n    return 'hello ' + name;\n}\n";
    let chunks = extract_js_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"greet"), "{:?}", ns);
}

#[test]
fn js_arrow_function_assigned() {
    let src = "const handler = (req, res) => {\n    res.send('ok');\n};\n";
    let chunks = extract_js_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"handler"), "{:?}", ns);
}

#[test]
fn js_async_arrow() {
    let src = "const fetchData = async (url) => {\n    const res = await fetch(url);\n    return res.json();\n};\n";
    let chunks = extract_js_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"fetchData"), "{:?}", ns);
}

#[test]
fn js_multiple_functions() {
    let src = "function foo() { return 1; }\nfunction bar() { return 2; }\n";
    let chunks = extract_js_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"foo"), "{:?}", ns);
    assert!(ns.contains(&"bar"), "{:?}", ns);
}

// ── CSV ──────────────────────────────────────────────────────────────────────

#[test]
fn csv_basic_rows() {
    let src = "id,name,value\nalice,Alice Smith,100\nbob,Bob Jones,200\n";
    let chunks = extract_csv_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"alice"), "{:?}", ns);
    assert!(ns.contains(&"bob"), "{:?}", ns);
}

#[test]
fn csv_quoted_first_field() {
    let src = "id,name\n\"alice, jr\",Alice\n\"bob\",Bob\n";
    let chunks = extract_csv_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"alice, jr"), "{:?}", ns);
}

#[test]
fn csv_empty_rows_skipped() {
    let src = "id,name\n,empty name row\nalice,Alice\n";
    let chunks = extract_csv_chunks(src);
    let ns = names(&chunks);
    assert!(!ns.contains(&""), "empty id rows must be skipped");
    assert!(ns.contains(&"alice"), "{:?}", ns);
}

// ── Markdown ─────────────────────────────────────────────────────────────────

#[test]
fn md_headings_extracted() {
    let src = "# Introduction\nSome text.\n## Usage\nHow to use.\n# API\nDetails.\n";
    let chunks = extract_md_chunks(src);
    let ns = names(&chunks);
    assert!(ns.contains(&"Introduction"), "{:?}", ns);
    assert!(ns.contains(&"API"), "{:?}", ns);
}

#[test]
fn md_no_headings_returns_empty() {
    let src = "just some text\nno headings here\n";
    let chunks = extract_md_chunks(src);
    assert!(chunks.is_empty(), "no headings = no chunks: {:?}", chunks);
}

#[test]
fn md_code_block_inside_section_included_in_body() {
    let src = "# Usage\n```rust\nfn main() {}\n```\n# End\n";
    let chunks = extract_md_chunks(src);
    let usage = chunks.iter().find(|(n, ..)| n == "Usage").unwrap();
    assert!(usage.3.contains("fn main"), "code block must be in section body");
}

// ── Generic / unknown ────────────────────────────────────────────────────────

#[test]
fn generic_unknown_ext_returns_one_chunk() {
    let src = "some random content\n";
    let chunks = extract_generic_chunk(src);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].0, "__file__");
}

#[test]
fn generic_empty_returns_no_chunks() {
    let chunks = extract_generic_chunk("");
    assert!(chunks.is_empty());
}

// ── Adaptive ─────────────────────────────────────────────────────────────────

#[test]
fn adaptive_returns_only_requested_symbol() {
    let src = "pub fn acquire() { true }\npub fn release() { false }\npub fn check() { true }\n";
    let chunks = extract_brace_chunks(src);
    let result: Vec<_> = chunks.iter().filter(|(n, ..)| n == "release").collect();
    assert_eq!(result.len(), 1, "must find exactly one release chunk");
    assert!(!result[0].3.contains("acquire"), "release body must not contain acquire");
    assert!(!result[0].3.contains("check"), "release body must not contain check");
}

#[test]
fn adaptive_line_numbers_non_overlapping() {
    let src = "pub fn foo() {\n    1\n}\npub fn bar() {\n    2\n}\npub fn baz() {\n    3\n}\n";
    let chunks = extract_brace_chunks(src);
    assert_eq!(chunks.len(), 3, "must extract 3 chunks: {:?}", names(&chunks));
    for i in 0..chunks.len()-1 {
        assert!(chunks[i].2 < chunks[i+1].1,
            "chunk {} ends at {} but chunk {} starts at {}",
            chunks[i].0, chunks[i].2, chunks[i+1].0, chunks[i+1].1);
    }
}

// ── Real file smoke test ──────────────────────────────────────────────────────

#[test]
fn smoke_real_rust_file() {
    let src = std::fs::read_to_string(
        std::path::Path::new("/Volumes/Hak_SSD/Anchor")
            .join("src/storage/anchor.rs")
    );
    let Ok(src) = src else { return; }; // skip if file missing
    let chunks = extract_brace_chunks(&src);
    assert!(!chunks.is_empty(), "real file must produce chunks");
    // every chunk must have a non-empty name
    for (name, start, end, _) in &chunks {
        assert!(!name.is_empty(), "chunk at line {start}-{end} has empty name");
        assert!(start <= end, "chunk {name}: start {start} > end {end}");
    }
}

#[test]
fn smoke_compare_blob_vs_expected_symbols() {
    let src = std::fs::read_to_string(
        std::path::Path::new("/Volumes/Hak_SSD/Anchor")
            .join("src/storage/anchor.rs")
    );
    let Ok(src) = src else { return; };
    let chunks = extract_brace_chunks(&src);
    let ns = names(&chunks);
    // top-level symbols and impl block names must be found
    let expected = ["content_hash", "AnchorStore", "CallIndex"];
    for sym in expected {
        assert!(ns.contains(&sym), "blob must extract '{}' — got: {:?}", sym, &ns[..ns.len().min(20)]);
    }
}

#[test]
fn known_limitation_methods_inside_impl_not_extracted_separately() {
    // Methods inside impl blocks are covered by the impl chunk, not as individual symbols.
    // This is a known tradeoff: blob extraction is simpler than tree-sitter but
    // doesn't drill into impl blocks. Fix: recursively extract inside impl bodies.
    let src = "impl Foo {\n    pub fn bar(&self) -> i32 { 1 }\n    pub fn baz(&self) -> i32 { 2 }\n}\n";
    let chunks = extract_brace_chunks(src);
    let ns = names(&chunks);
    // impl block is extracted as "Foo", methods bar/baz may or may not be separate
    assert!(ns.contains(&"Foo"), "impl block must be extracted: {:?}", ns);
    // document: bar and baz body IS inside the Foo chunk
    let foo_chunk = chunks.iter().find(|(n, ..)| n == "Foo").unwrap();
    assert!(foo_chunk.3.contains("bar"), "bar must be in Foo's body");
    assert!(foo_chunk.3.contains("baz"), "baz must be in Foo's body");
}
