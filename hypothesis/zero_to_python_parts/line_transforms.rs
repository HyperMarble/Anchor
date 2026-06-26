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
