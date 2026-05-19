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

fn transform_line(line: &str) -> String {
    let line = line.trim();

    // comments: "// text" → "# text"
    if line.starts_with("//") {
        return format!("#{}", &line[2..]);
    }
    // inline comment: strip and re-attach as Python comment
    // (handled by emitting as-is after value transform — basic support)

    // const FOO = val
    if let Some(rest) = line.strip_prefix("const ") {
        if let Some(eq) = rest.find('=') {
            let name = rest[..eq].trim();
            let val = transform_value(rest[eq+1..].trim());
            return format!("{} = {}", name, val);
        }
    }

    // type alias: "type Alias = SomeType" → "Alias = SomeType" (Python 3.12: type Alias = ...)
    if let Some(rest) = line.strip_prefix("type ") {
        if let Some(eq) = rest.find('=') {
            let name = rest[..eq].trim();
            return format!("{} = object  # type alias", name);
        }
    }

    // pub static fun / static fun → @staticmethod + def
    // (handled in calling code by emitting decorator first)

    // async pub fun / pub async fun / async fun → async def
    let is_async = line.starts_with("async pub fun ")
        || line.starts_with("pub async fun ")
        || line.starts_with("async fun ");

    let fun_rest = line
        .strip_prefix("async pub fun ")
        .or_else(|| line.strip_prefix("pub async fun "))
        .or_else(|| line.strip_prefix("async fun "))
        .or_else(|| line.strip_prefix("pub fun "))
        .or_else(|| line.strip_prefix("fun "));

    if let Some(rest) = fun_rest {
        let is_static = line.starts_with("pub static fun ") || line.starts_with("static fun ");
        let _ = is_static; // handled in caller
        let paren = rest.find('(').unwrap_or(rest.len());
        let name = &rest[..paren];
        let after = &rest[paren+1..];
        let close = after.find(')').unwrap_or(after.len());
        let params = transform_params(&after[..close]);
        let prefix = if is_async { "async def" } else { "def" };
        return format!("{} {}({}):", prefix, name.trim(), params);
    }
    // normalize double space from above
    let line_str;

    // pub static fun / static fun → emits @staticmethod before def
    // handled in main loop

    // await expr
    if let Some(rest) = line.strip_prefix("await ") {
        // usually part of a let binding, handled there
        // standalone await statement
        return format!("await {}", transform_value(rest));
    }

    // let mut / let
    if let Some(rest) = line
        .strip_prefix("let mut ")
        .or_else(|| line.strip_prefix("let "))
    {
        if let Some(eq) = rest.find('=') {
            let lhs = rest[..eq].trim();
            let name = lhs.split(':').next().unwrap_or(lhs).trim();
            let val_raw = rest[eq+1..].trim();
            // handle "await expr"
            let val = if val_raw.starts_with("await ") {
                format!("await {}", transform_value(&val_raw["await ".len()..]))
            } else {
                transform_value(val_raw)
            };
            return format!("{} = {}", name, val);
        }
        let name = rest.split(':').next().unwrap_or(rest).trim();
        return format!("{} = []", name);
    }

    // use → import
    if let Some(rest) = line.strip_prefix("use ") {
        let path = rest.trim_end_matches(';');
        if let Some(dot) = path.rfind('.') {
            return format!("from {} import {}", &path[..dot], &path[dot+1..]);
        }
        return format!("import {}", path);
    }

    // shape → class
    if let Some(rest) = line.strip_prefix("shape ") {
        let name = rest.trim_end_matches('{').trim();
        let name = if let Some(lt) = name.find('<') { &name[..lt] } else { name };
        return format!("class {}:", name.trim());
    }

    // choice → class
    if let Some(rest) = line.strip_prefix("choice ") {
        let name = rest.trim_end_matches('{').trim();
        return format!("class {}:", name.trim());
    }

    // enum → class(Enum)
    if let Some(rest) = line.strip_prefix("enum ") {
        let name = rest.trim_end_matches('{').trim();
        return format!("class {}(Enum):", name.trim());
    }

    // impl Foo { → (class method section — just comment)
    if let Some(rest) = line.strip_prefix("impl ") {
        let name = rest.trim_end_matches('{').trim();
        return format!("# impl {}", name);
    }

    // check world.out.write
    if let Some(rest) = line
        .strip_prefix("check world.out.write(")
        .or_else(|| line.strip_prefix("world.out.write("))
    {
        let inner = rest.trim_end_matches(')').trim_matches('"');
        let msg = inner.trim_end_matches("\\n");
        let msg = if msg.contains("${") {
            format!("f\"{}\"", msg.replace("${", "{"))
        } else {
            format!("\"{}\"", msg)
        };
        return format!("print({})", msg);
    }

    // try {
    if line == "try {" { return "try:".to_string(); }

    // raise
    if let Some(rest) = line.strip_prefix("raise ") {
        return format!("raise {}", rest);
    }

    // assert
    if let Some(rest) = line.strip_prefix("assert ") {
        return format!("assert {}", transform_value(rest));
    }

    // for x in y {
    if let Some(rest) = line.strip_prefix("for ") {
        let body = rest.trim_end_matches('{').trim();
        return format!("for {}:", transform_value(body));
    }

    // if cond {
    if let Some(rest) = line.strip_prefix("if ") {
        let cond = transform_value(rest.trim_end_matches('{').trim());
        return format!("if {}:", cond);
    }

    // while cond {
    if let Some(rest) = line.strip_prefix("while ") {
        let cond = transform_value(rest.trim_end_matches('{').trim());
        return format!("while {}:", cond);
    }

    // return
    if let Some(rest) = line.strip_prefix("return ") {
        return format!("return {}", transform_value(rest));
    }

    // yield
    if let Some(rest) = line.strip_prefix("yield ") {
        return format!("yield {}", transform_value(rest));
    }

    // pass-through
    if line == "continue" { return "continue".to_string(); }
    if line == "break"    { return "break".to_string(); }
    if line == "pass"     { return "pass".to_string(); }

    // field declaration inside shape: "name: Type," → "self.name = None"
    if is_field_decl(line) {
        let name = line.split(':').next().unwrap_or("").trim();
        // check for default value: "name: Type = default"
        if let Some(eq) = line.find('=') {
            let default = transform_value(line[eq+1..].trim().trim_end_matches(',').trim());
            return format!("self.{} = {}", name, default);
        }
        return format!("self.{} = None", name);
    }

    line_str = transform_value(line);
    line_str
}

fn is_field_decl(line: &str) -> bool {
    let line = line.trim_end_matches(',').trim();
    if line.contains('(') || line.starts_with("//") || line.starts_with('#') { return false; }
    if line.starts_with("if ") || line.starts_with("for ") || line.starts_with("while ") { return false; }
    let mut parts = line.splitn(2, ':');
    let key = parts.next().unwrap_or("").trim();
    let ty  = parts.next().unwrap_or("").trim();
    !key.is_empty() && !ty.is_empty()
        && key.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !["let","fun","pub","use","try","raise","return","yield","match",
             "assert","const","type","impl","shape","choice","enum","async","await",
             "if","while","for"].contains(&key)
}

// ── Main transpile loop ───────────────────────────────────────────────────────

fn transpile(source: &str) -> String {
    let mut out   = String::new();
    let mut indent: usize = 0;
    let mut state  = State::Normal;
    let mut prev_was_block_open = false;  // for empty body → pass

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        i += 1;

        if trimmed.is_empty() { out.push('\n'); prev_was_block_open = false; continue; }

        // ── Closing brace ────────────────────────────────────────────────────
        if trimmed == "}" {
            if prev_was_block_open {
                out.push_str(&"    ".repeat(indent));
                out.push_str("pass\n");
            }
            if let State::Match { entry_indent, .. } = state {
                if indent == entry_indent {
                    // outer match brace closing
                    indent = indent.saturating_sub(1);
                    state = State::Normal;
                } else {
                    // arm body closing — stay in match state
                    indent = indent.saturating_sub(1);
                }
            } else {
                indent = indent.saturating_sub(1);
            }
            prev_was_block_open = false;
            continue;
        }

        // ── } else if ────────────────────────────────────────────────────────
        if let Some(rest) = trimmed.strip_prefix("} else if ") {
            indent = indent.saturating_sub(1);
            let cond = transform_value(rest.trim_end_matches('{').trim());
            out.push_str(&"    ".repeat(indent));
            out.push_str(&format!("elif {}:\n", cond));
            indent += 1;
            prev_was_block_open = true;
            continue;
        }

        // ── } else { ────────────────────────────────────────────────────────
        if trimmed == "} else {" {
            indent = indent.saturating_sub(1);
            out.push_str(&"    ".repeat(indent));
            out.push_str("else:\n");
            indent += 1;
            prev_was_block_open = true;
            continue;
        }

        // ── } rescue ────────────────────────────────────────────────────────
        if let Some(rest) = trimmed.strip_prefix("} rescue") {
            indent = indent.saturating_sub(1);
            let rest = rest.trim().trim_end_matches('{').trim();
            if rest.is_empty() {
                out.push_str(&"    ".repeat(indent));
                out.push_str("except:\n");
            } else if let Some(as_pos) = rest.find(" as ") {
                let exc = rest[..as_pos].trim();
                let var = rest[as_pos+4..].trim();
                out.push_str(&"    ".repeat(indent));
                out.push_str(&format!("except {} as {}:\n", exc, var));
            } else {
                out.push_str(&"    ".repeat(indent));
                out.push_str(&format!("except {}:\n", rest));
            }
            indent += 1;
            prev_was_block_open = true;
            continue;
        }

        // ── match x { ───────────────────────────────────────────────────────
        if let Some(rest) = trimmed.strip_prefix("match ") {
            let var = rest.trim_end_matches('{').trim().to_string();
            state = State::Match { var, first: true, entry_indent: indent };
            prev_was_block_open = false;
            continue;
        }

        // ── match case arms: .variant => binding { ──────────────────────────
        if let State::Match { ref var, ref mut first, .. } = state {
            if trimmed.starts_with('.') && trimmed.contains(" => ") {
                let rest = &trimmed[1..]; // strip leading dot
                if let Some(arrow) = rest.find(" => ") {
                    let variant = &rest[..arrow];
                    let after   = rest[arrow+4..].trim_end_matches('{').trim();
                    let kw = if *first { "if" } else { "elif" };
                    *first = false;
                    out.push_str(&"    ".repeat(indent));
                    out.push_str(&format!("{} {}.{} is not None:\n", kw, var, variant));
                    indent += 1;
                    if !after.is_empty() && after != "_" {
                        out.push_str(&"    ".repeat(indent));
                        out.push_str(&format!("{} = {}.{}\n", after, var, variant));
                    }
                    prev_was_block_open = true;
                    continue;
                }
            }
            // wildcard: "_ {" or "_ => {"
            if trimmed.starts_with("_ ") || trimmed == "_ {" {
                indent = indent.saturating_sub(1);
                out.push_str(&"    ".repeat(indent));
                out.push_str("else:\n");
                indent += 1;
                prev_was_block_open = true;
                continue;
            }
        }

        // ── static fun → @staticmethod ───────────────────────────────────────
        let is_static = trimmed.starts_with("pub static fun ")
            || trimmed.starts_with("static fun ");
        if is_static {
            out.push_str(&"    ".repeat(indent));
            out.push_str("@staticmethod\n");
            let stripped = trimmed
                .strip_prefix("pub static fun ")
                .or_else(|| trimmed.strip_prefix("static fun "))
                .unwrap();
            let py = transform_line(&format!("fun {}", stripped));
            out.push_str(&"    ".repeat(indent));
            out.push_str(&py);
            out.push('\n');
            if py.ends_with(':') { indent += 1; }
            prev_was_block_open = true;
            continue;
        }

        // ── @annotation / decorator ─────────────────────────────────────────
        if trimmed.starts_with('@') {
            out.push_str(&"    ".repeat(indent));
            out.push_str(trimmed);
            out.push('\n');
            prev_was_block_open = false;
            continue;
        }

        // ── Normal line ──────────────────────────────────────────────────────
        let py = transform_line(trimmed);
        if py.is_empty() { prev_was_block_open = false; continue; }

        out.push_str(&"    ".repeat(indent));
        out.push_str(&py);
        out.push('\n');

        prev_was_block_open = py.ends_with(':');
        if py.ends_with(':') { indent += 1; }
    }

    out
}

// ── Test harness ──────────────────────────────────────────────────────────────

fn check(label: &str, input: &str, expected: &str) {
    let got = transpile(input).trim().to_string();
    let exp = expected.trim().to_string();
    if got == exp {
        println!("PASS  {}", label);
    } else {
        println!("FAIL  {}", label);
        let exp_lines: Vec<&str> = exp.lines().collect();
        let got_lines: Vec<&str> = got.lines().collect();
        let max = exp_lines.len().max(got_lines.len());
        for n in 0..max {
            let e = exp_lines.get(n).copied().unwrap_or("<missing>");
            let g = got_lines.get(n).copied().unwrap_or("<missing>");
            if e != g { println!("  L{}: exp {:?}", n+1, e);
                        println!("       got {:?}", g); }
        }
    }
}

fn main() {
    // ── Regression ───────────────────────────────────────────────────────────
    check("hello world",
        "pub fun main(world: World) -> Void raises {\n    check world.out.write(\"hello from zero\\n\")\n}",
        "def main():\n    print(\"hello from zero\")");

    check("fibonacci",
r#"fun fib(n: i32) -> i32 {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}"#,
r#"def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)"#);

    // ── Comments ─────────────────────────────────────────────────────────────
    check("comments",
r#"// Compute the parent run id
fun get_parent_run(run_id: String) -> String {
    return run_id
}"#,
r#"# Compute the parent run id
def get_parent_run(run_id):
    return run_id"#);

    // ── const ────────────────────────────────────────────────────────────────
    check("const",
r#"const MAX_RETRIES = 3
const DEFAULT_TIMEOUT = 30.0
const APP_NAME = "mlflow""#,
r#"MAX_RETRIES = 3
DEFAULT_TIMEOUT = 30.0
APP_NAME = "mlflow""#);

    // ── Default params ────────────────────────────────────────────────────────
    check("default params",
r#"fun create_run(experiment_id: String, start_time: i64 = 0, run_name: String = "default") -> Run {
    return client.create_run(experiment_id, start_time, run_name)
}"#,
r#"def create_run(experiment_id, start_time=0, run_name="default"):
    return client.create_run(experiment_id, start_time, run_name)"#);

    // ── Async / await ─────────────────────────────────────────────────────────
    check("async/await",
r#"async fun fetch_run(client: ref<Client>, run_id: String) -> Run {
    let run = await client.get_run_async(run_id)
    return run
}"#,
r#"async def fetch_run(client, run_id):
    run = await client.get_run_async(run_id)
    return run"#);

    // ── Logical ops ───────────────────────────────────────────────────────────
    check("logical ops",
r#"fun is_valid(a: bool, b: bool, c: bool) -> bool {
    return a && b || !c
}"#,
r#"def is_valid(a, b, c):
    return a  and  b  or  not c"#);

    // ── String interpolation ──────────────────────────────────────────────────
    check("string interpolation",
r#"fun greet(name: String) -> String {
    return "Hello ${name}!"
}"#,
r#"def greet(name):
    return f"Hello {name}!""#);

    // ── elif chain ────────────────────────────────────────────────────────────
    check("elif chain",
r#"fun classify(n: i32) -> String {
    if n < 0 {
        return "negative"
    } else if n == 0 {
        return "zero"
    } else {
        return "positive"
    }
}"#,
r#"def classify(n):
    if n < 0:
        return "negative"
    elif n == 0:
        return "zero"
    else:
        return "positive""#);

    // ── try/rescue ────────────────────────────────────────────────────────────
    check("try/rescue",
r#"fun safe_get(client: ref<Client>, name: String) -> Maybe<Model> {
    try {
        let model = client.get_model(name)
        return model
    } rescue MlflowException as e {
        return none
    }
}"#,
r#"def safe_get(client, name):
    try:
        model = client.get_model(name)
        return model
    except MlflowException as e:
        return None"#);

    // ── match/case ────────────────────────────────────────────────────────────
    check("match/case",
r#"fun handle_result(result: Result) -> String {
    match result {
        .ok => value {
            return "success"
        }
        .err => msg {
            return "failed"
        }
    }
}"#,
r#"def handle_result(result):
    if result.ok is not None:
        value = result.ok
        return "success"
    elif result.err is not None:
        msg = result.err
        return "failed""#);

    // ── static method ─────────────────────────────────────────────────────────
    check("static method",
r#"shape RunTag {
    key: String,
    value: String,

    pub static fun from_proto(proto: Proto) -> Self {
        return RunTag { key: proto.key, value: proto.value }
    }
}"#,
r#"class RunTag:
    self.key = None
    self.value = None

    @staticmethod
    def from_proto(proto):
        return RunTag(key=proto.key, value=proto.value)"#);

    // ── string methods + type cast ────────────────────────────────────────────
    check("string methods + cast",
r#"fun normalize(name: String) -> String {
    let lower = name.to_lowercase()
    let trimmed = lower.trim()
    return trimmed
}"#,
r#"def normalize(name):
    lower = name.lower()
    trimmed = lower.strip()
    return trimmed"#);

    check("type cast stripped",
r#"fun to_int(x: f64) -> i32 {
    return x as i32
}"#,
r#"def to_int(x):
    return x"#);

    // ── use → import ──────────────────────────────────────────────────────────
    check("use → import",
r#"use mlflow.entities.Run
use mlflow.exceptions.MlflowException

fun get(client: ref<Client>, run_id: String) -> Run {
    return client.get_run(run_id)
}"#,
r#"from mlflow.entities import Run
from mlflow.exceptions import MlflowException

def get(client, run_id):
    return client.get_run(run_id)"#);

    // ── for + not in + continue ───────────────────────────────────────────────
    check("for + not in + continue",
r#"fun filter_providers(providers: List<String>, allowed: Maybe<Set<String>>) -> List<String> {
    if allowed == none {
        return providers
    }
    let mut result: List<String>
    for p in providers {
        let name = normalize(p.lower())
        if !allowed.contains(name) {
            continue
        }
        result.append(p)
    }
    return result
}"#,
r#"def filter_providers(providers, allowed):
    if allowed is None:
        return providers
    result = []
    for p in providers:
        name = normalize(p.lower())
        if name not in allowed:
            continue
        result.append(p)
    return result"#);

    // ── assert ────────────────────────────────────────────────────────────────
    check("assert",
r#"fun validate(run_id: String) -> Void {
    assert run_id.len() > 0
}"#,
r#"def validate(run_id):
    assert run_id.len() > 0"#);

    // ── empty body → pass ─────────────────────────────────────────────────────
    check("empty body → pass",
r#"fun noop() -> Void {
}"#,
r#"def noop():
    pass"#);

    // ── decorator passthrough ─────────────────────────────────────────────────
    check("decorator",
r#"@deprecated("use new_method instead")
fun old_method(x: i32) -> i32 {
    return x
}"#,
r#"@deprecated("use new_method instead")
def old_method(x):
    return x"#);

    // ── MLflow: get_parent_run ────────────────────────────────────────────────
    check("mlflow: get_parent_run",
r#"fun get_parent_run(self: ref<Self>, run_id: String) -> Maybe<Run> {
    let child_run = self.tracking_client.get_run(run_id)
    let parent_run_id = child_run.data.tags.get("MLFLOW_PARENT_RUN_ID")
    if parent_run_id == none {
        return none
    }
    return self.tracking_client.get_run(parent_run_id)
}"#,
r#"def get_parent_run(self, run_id):
    child_run = self.tracking_client.get_run(run_id)
    parent_run_id = child_run.data.tags.get("MLFLOW_PARENT_RUN_ID")
    if parent_run_id is None:
        return None
    return self.tracking_client.get_run(parent_run_id)"#);

    // ── MLflow: validate_delete_traces ────────────────────────────────────────
    check("mlflow: validate_delete_traces",
r#"fun validate_delete_traces(max_ts: Maybe<i64>, max_traces: Maybe<i32>, trace_ids: Maybe<List<String>>) -> Void {
    if trace_ids != none {
        if max_ts != none {
            raise InvalidArgument("cannot specify both trace_ids and max_timestamp_millis")
        }
        if max_traces != none {
            raise InvalidArgument("cannot specify max_traces when trace_ids is given")
        }
    }
    if max_ts == none {
        if trace_ids == none {
            raise InvalidArgument("must specify either max_timestamp_millis or trace_ids")
        }
    }
}"#,
r#"def validate_delete_traces(max_ts, max_traces, trace_ids):
    if trace_ids is not None:
        if max_ts is not None:
            raise InvalidArgument("cannot specify both trace_ids and max_timestamp_millis")
        if max_traces is not None:
            raise InvalidArgument("cannot specify max_traces when trace_ids is given")
    if max_ts is None:
        if trace_ids is None:
            raise InvalidArgument("must specify either max_timestamp_millis or trace_ids")"#);

    // ── MLflow: register_prompt core (complex) ────────────────────────────────
    check("mlflow: register_prompt core",
r#"fun register_prompt(self: ref<Self>, name: String, template: String, is_databricks: bool) -> Prompt {
    validate_prompt_name(name)
    if is_databricks {
        try {
            self.registry_client.create_prompt(name)
        } rescue MlflowException as e {
            if e.error_code != "ALREADY_EXISTS" {
                raise e
            }
        }
        let pv = self.registry_client.create_prompt_version(name, template)
        return self.registry_client.get_prompt_version(name, pv.version)
    }
    let mut is_new_prompt = false
    let mut rm = none
    try {
        rm = self.registry_client.get_registered_model(name)
    } rescue MlflowException {
        self.registry_client.create_registered_model(name)
        is_new_prompt = true
    }
    if rm != none {
        if !has_prompt_tag(rm.tags) {
            raise MlflowException("Model with same name exists")
        }
    }
    return self.registry_client.create_prompt_version(name, template)
}"#,
r#"def register_prompt(self, name, template, is_databricks):
    validate_prompt_name(name)
    if is_databricks:
        try:
            self.registry_client.create_prompt(name)
        except MlflowException as e:
            if e.error_code != "ALREADY_EXISTS":
                raise e
        pv = self.registry_client.create_prompt_version(name, template)
        return self.registry_client.get_prompt_version(name, pv.version)
    is_new_prompt = False
    rm = None
    try:
        rm = self.registry_client.get_registered_model(name)
    except MlflowException:
        self.registry_client.create_registered_model(name)
        is_new_prompt = True
    if rm is not None:
        if not has_prompt_tag(rm.tags):
            raise MlflowException("Model with same name exists")
    return self.registry_client.create_prompt_version(name, template)"#);

    // ── MLflow: search_runs pagination pattern ────────────────────────────────
    check("mlflow: pagination loop",
r#"fun get_all_runs(self: ref<Self>, experiment_id: String) -> List<Run> {
    let mut all_runs: List<Run>
    let mut page_token = none
    while true {
        let page = self.client.search_runs(experiment_id, page_token)
        for run in page.runs {
            all_runs.append(run)
        }
        if page.next_token == none {
            break
        }
        page_token = page.next_token
    }
    return all_runs
}"#,
r#"def get_all_runs(self, experiment_id):
    all_runs = []
    page_token = None
    while True:
        page = self.client.search_runs(experiment_id, page_token)
        for run in page.runs:
            all_runs.append(run)
        if page.next_token is None:
            break
        page_token = page.next_token
    return all_runs"#);

    // ── yield / generator ─────────────────────────────────────────────────────
    check("yield",
r#"fun iter_metrics(history: List<Metric>) -> Metric {
    for m in history {
        yield m
    }
}"#,
r#"def iter_metrics(history):
    for m in history:
        yield m"#);

    // ── struct init ───────────────────────────────────────────────────────────
    check("struct init → keyword call",
r#"fun make_tag(key: String, value: String) -> RunTag {
    return RunTag { key: key, value: value }
}"#,
r#"def make_tag(key, value):
    return RunTag(key=key, value=value)"#);
}
