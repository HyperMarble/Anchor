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
