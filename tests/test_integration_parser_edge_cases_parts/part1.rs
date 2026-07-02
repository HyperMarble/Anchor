use anchor::parser::extract_file;
use std::path::PathBuf;

fn symbol_names(file: &str, src: &str) -> Vec<String> {
    extract_file(&PathBuf::from(file), src)
        .unwrap()
        .symbols
        .into_iter()
        .map(|symbol| symbol.name)
        .collect()
}

fn call_names(file: &str, src: &str) -> Vec<String> {
    extract_file(&PathBuf::from(file), src)
        .unwrap()
        .calls
        .into_iter()
        .map(|call| call.callee)
        .collect()
}

#[test]
fn test_typescript_exported_and_property_functions() {
    let src = r#"
export class ApiClient {
    fetchUser = async (id: string) => request(id);

    saveUser(user: User) {
        return request(user.id);
    }
}

export const makeClient = () => new ApiClient();
export default function boot() {
    return makeClient();
}
"#;
    let names = symbol_names("client.ts", src);
    assert!(names.contains(&"ApiClient".to_string()));
    assert!(names.contains(&"fetchUser".to_string()));
    assert!(names.contains(&"saveUser".to_string()));
    assert!(names.contains(&"makeClient".to_string()));
    assert!(names.contains(&"boot".to_string()));

    let calls = call_names("client.ts", src);
    assert!(calls.contains(&"request".to_string()));
    assert!(calls.contains(&"ApiClient".to_string()));
    assert!(calls.contains(&"makeClient".to_string()));
}

#[test]
fn test_typescript_abstract_namespace_generator_and_signatures() {
    let src = r#"
export namespace Workbench {
    export abstract class Contribution {
        abstract activate(context: Context): Promise<void>;

        protected *events(): Iterable<Event> {
            yield createEvent();
        }
    }

    export interface Registry {
        register(id: string): void;
        readonly size: number;
    }
}

export function* enumerate() {
    yield Workbench;
}
"#;
    let names = symbol_names("workbench.ts", src);
    assert!(names.contains(&"Workbench".to_string()));
    assert!(names.contains(&"Contribution".to_string()));
    assert!(names.contains(&"activate".to_string()));
    assert!(names.contains(&"events".to_string()));
    assert!(names.contains(&"Registry".to_string()));
    assert!(names.contains(&"register".to_string()));
    assert!(names.contains(&"size".to_string()));
    assert!(names.contains(&"enumerate".to_string()));

    let calls = call_names("workbench.ts", src);
    assert!(calls.contains(&"createEvent".to_string()));
}

#[test]
fn test_javascript_object_literal_methods() {
    let src = r#"
const service = {
    start() {
        return boot();
    },
    stop: function stopService() {
        return shutdown();
    },
    restart: () => service.start(),
};
"#;
    let names = symbol_names("service.js", src);
    assert!(names.contains(&"start".to_string()));
    assert!(names.contains(&"stopService".to_string()));
    assert!(names.contains(&"restart".to_string()));

    let calls = call_names("service.js", src);
    assert!(calls.contains(&"boot".to_string()));
    assert!(calls.contains(&"shutdown".to_string()));
    assert!(calls.contains(&"start".to_string()));
}

#[test]
fn test_javascript_generator_and_commonjs_exports() {
    let src = r#"
function* ids() {
    yield nextId();
}

module.exports.create = function createService() {
    return build();
};
"#;
    let names = symbol_names("service.js", src);
    assert!(names.contains(&"ids".to_string()));
    assert!(names.contains(&"createService".to_string()));

    let calls = call_names("service.js", src);
    assert!(calls.contains(&"nextId".to_string()));
    assert!(calls.contains(&"build".to_string()));
}

#[test]
fn test_rust_trait_signatures_macros_and_scoped_calls() {
    let src = r#"
macro_rules! make_service {
    () => {};
}

trait Service {
    type Output;
    const NAME: &'static str;
    fn execute(&self) -> Self::Output;
}

impl Service for Runner {
    type Output = ();
    const NAME: &'static str = "runner";

    fn execute(&self) -> Self::Output {
        make_service!();
        Runner::boot();
    }
}
"#;
    let names = symbol_names("service.rs", src);
    assert!(names.contains(&"make_service".to_string()));
    assert!(names.contains(&"Service".to_string()));
    assert!(names.contains(&"Output".to_string()));
    assert!(names.contains(&"NAME".to_string()));
    assert!(names.contains(&"execute".to_string()));

    let calls = call_names("service.rs", src);
    assert!(calls.contains(&"make_service".to_string()));
    assert!(calls.contains(&"boot".to_string()));
}

