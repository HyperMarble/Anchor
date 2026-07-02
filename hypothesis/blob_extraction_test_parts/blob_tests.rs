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
