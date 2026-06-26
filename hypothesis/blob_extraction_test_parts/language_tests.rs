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
