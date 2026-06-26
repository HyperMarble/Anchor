// hypothesis/zero_to_python.rs
// Comprehensive Zero → Python transpiler — handles everything
// Compile: rustc zero_to_python.rs -o zero_to_python && ./zero_to_python

// ── State for multi-line constructs ──────────────────────────────────────────

#[derive(Clone)]
enum State {
    Normal,
    Match { var: String, first: bool, entry_indent: usize },
}

// ── Value transforms ──────────────────────────────────────────────────────────

fn transform_params(params: &str) -> String {
    params
        .split(',')
        .filter_map(|p| {
            let p = p.trim();
            if p.is_empty() { return None; }
            if p.starts_with("self:") { return Some("self".to_string()); }
            let name = p.split(':').next()?.trim().to_string();
            if name == "world" { return None; }
            // handle default: "name: Type = default" → "name=default"
            if p.contains('=') {
                let mut parts = p.splitn(2, '=');
                let lhs = parts.next().unwrap_or("").split(':').next().unwrap_or("").trim();
                let rhs = parts.next().unwrap_or("").trim();
                return Some(format!("{}={}", lhs, transform_value(rhs)));
            }
            Some(name)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn transform_value(v: &str) -> String {
    let v = v.trim();
    if v == "none" { return "None".to_string(); }
    if v.is_empty() { return String::new(); }

    // leading ! handled inline below
    let mut r = v.to_string();

    // strip type casts: "x as i32" → "x", "x as f64" → "x", etc.
    for cast in &[" as i32", " as i64", " as i16", " as i8",
                  " as u32", " as u64", " as usize", " as isize",
                  " as f32", " as f64", " as String", " as str", " as bool"] {
        r = r.replace(cast, "");
    }

    // logical operators
    r = r.replace("&&", " and ").replace("||", " or ");

    // booleans
    r = r.replace("true", "True").replace("false", "False");

    // none in expressions → None
    r = r
        .replace("== none", "== None")
        .replace("!= none", "!= None")
        .replace(", none", ", None")
        .replace("(none)", "(None)")
        .replace(" none", " None")
        .replace("= none", "= None");

    // None comparisons → is/is not
    r = r.replace("== None", "is None").replace("!= None", "is not None");

    // not_in keyword
    r = r.replace(" not_in ", " not in ");

    // string methods
    r = r.replace(".to_uppercase()", ".upper()")
         .replace(".to_lowercase()", ".lower()")
         .replace(".to_string()", "")       // Python: already a str
         .replace(".trim()", ".strip()");

    // isinstance with generics: isinstance(x, List<T>) → isinstance(x, list)
    r = strip_isinstance_generics(&r);

    // not in the middle of expressions (avoid strings)
    r = expand_inline_not(&r);

    // string interpolation ${var} → f"{var}"
    if r.contains("${") {
        let inner = r.trim_matches('"').replace("${", "{");
        r = format!("f\"{}\"", inner);
    }

    // struct initialisation: Type { a: a, b: b } → Type(a=a, b=b)
    r = struct_init_to_call(&r);

    r
}

fn strip_isinstance_generics(s: &str) -> String {
    // isinstance(x, List<T>) → isinstance(x, list)
    // isinstance(x, Dict<K,V>) → isinstance(x, dict)
    let mappings = [
        ("List<", "list"), ("Vec<", "list"), ("Set<", "set"),
        ("Dict<", "dict"), ("Map<", "dict"), ("Maybe<", "object"),
    ];
    let mut r = s.to_string();
    for (generic, py) in &mappings {
        while let Some(start) = r.find(generic) {
            if let Some(end) = r[start..].find('>') {
                r = format!("{}{}{}", &r[..start], py, &r[start + end + 1..]);
            } else { break; }
        }
    }
    r
}

fn expand_inline_not(s: &str) -> String {
    // "!x.contains(y)" → "y not in x", "!flag" → "not flag"
    // but not inside string literals and not "!="
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut in_str = false;
    let mut str_ch = '"';
    while i < chars.len() {
        if in_str {
            out.push(chars[i]);
            if chars[i] == '\\' { i += 1; if i < chars.len() { out.push(chars[i]); } }
            else if chars[i] == str_ch { in_str = false; }
        } else if chars[i] == '"' || chars[i] == '\'' {
            in_str = true; str_ch = chars[i]; out.push(chars[i]);
        } else if chars[i] == '!' && chars.get(i+1).map(|&c| c != '=').unwrap_or(false) {
            // collect the expression after !
            let rest: String = chars[i+1..].iter().collect();
            let rest = rest.trim_start();
            // check for .contains(y)
            if let Some(dot) = rest.find(".contains(") {
                let container = &rest[..dot];
                let item_end = rest[dot + ".contains(".len()..].find(')').unwrap_or(0);
                let item = &rest[dot + ".contains(".len()..dot + ".contains(".len() + item_end];
                out.push_str(&format!("{} not in {}", item, container));
                i += 1 + (chars.len() - i - 1) - rest.len() + dot + ".contains(".len() + item_end + 1;
                continue;
            }
            out.push_str("not ");
        } else {
            out.push(chars[i]);
        }
        i += 1;
    }
    out
}

fn struct_init_to_call(s: &str) -> String {
    if let Some(brace) = s.find(" {") {
        let prefix = &s[..brace];
        let rest = s[brace+2..].trim();
        if rest.ends_with('}') && !rest.starts_with('{') {
            let inner = rest.trim_end_matches('}').trim();
            let args: Vec<String> = inner.split(',').filter_map(|field| {
                let f = field.trim();
                if f.is_empty() { return None; }
                let mut p = f.splitn(2, ':');
                let k = p.next()?.trim();
                let v = p.next()?.trim();
                if k.is_empty() || v.is_empty() { return None; }
                Some(format!("{}={}", k, v))
            }).collect();
            if !args.is_empty() {
                return format!("{}({})", prefix, args.join(", "));
            }
        }
    }
    s.to_string()
}

// ── Line transforms ───────────────────────────────────────────────────────────

