#[test]
fn test_python_async_decorated_and_nested_functions() {
    let src = r#"
class Worker:
    @classmethod
    async def build(cls, config):
        return await create_worker(config)

def outer():
    def inner():
        return run()
    return inner()
"#;
    let names = symbol_names("worker.py", src);
    assert!(names.contains(&"Worker".to_string()));
    assert!(names.contains(&"build".to_string()));
    assert!(names.contains(&"outer".to_string()));
    assert!(names.contains(&"inner".to_string()));

    let calls = call_names("worker.py", src);
    assert!(calls.contains(&"create_worker".to_string()));
    assert!(calls.contains(&"run".to_string()));
    assert!(calls.contains(&"inner".to_string()));
}

#[test]
fn test_python_import_aliases_and_chained_calls() {
    let src = r#"
from .services import Worker as ServiceWorker
import package.module as mod

def run_all():
    return mod.factory().execute(ServiceWorker())
"#;
    let extraction = extract_file(&PathBuf::from("runner.py"), src).unwrap();
    let names: Vec<String> = extraction
        .symbols
        .into_iter()
        .map(|symbol| symbol.name)
        .collect();
    assert!(names.contains(&"run_all".to_string()));
    assert!(!extraction.imports.is_empty());

    let calls: Vec<String> = extraction
        .calls
        .into_iter()
        .map(|call| call.callee)
        .collect();
    assert!(calls.contains(&"factory".to_string()));
    assert!(calls.contains(&"execute".to_string()));
    assert!(calls.contains(&"ServiceWorker".to_string()));
}

#[test]
fn test_go_interface_methods_are_symbols() {
    let src = r#"
package storage

type Repository interface {
    Save(item any) error
    FindByID(id string) (any, error)
}
"#;
    let names = symbol_names("storage.go", src);
    assert!(names.contains(&"Repository".to_string()));
    assert!(names.contains(&"Save".to_string()));
    assert!(names.contains(&"FindByID".to_string()));
}

#[test]
fn test_java_records_annotations_and_constructors() {
    let src = r#"
public @interface Route {
    String value();
}

public record UserRecord(String id, String name) {
    public UserRecord {
        validate(id);
    }
}
"#;
    let names = symbol_names("UserRecord.java", src);
    assert!(names.contains(&"Route".to_string()));
    assert!(names.contains(&"value".to_string()));
    assert!(names.contains(&"UserRecord".to_string()));

    let calls = call_names("UserRecord.java", src);
    assert!(calls.contains(&"validate".to_string()));
}

#[test]
fn test_csharp_records_delegates_properties_and_local_functions() {
    let src = r#"
public delegate Task Handler(string id);

public record UserRecord(string Id)
{
    public string DisplayName { get; init; }

    public void Run()
    {
        void LocalStep() => Dispatch(DisplayName);
        LocalStep();
    }
}
"#;
    let names = symbol_names("UserRecord.cs", src);
    assert!(names.contains(&"Handler".to_string()));
    assert!(names.contains(&"UserRecord".to_string()));
    assert!(names.contains(&"DisplayName".to_string()));
    assert!(names.contains(&"Run".to_string()));
    assert!(names.contains(&"LocalStep".to_string()));

    let calls = call_names("UserRecord.cs", src);
    assert!(calls.contains(&"Dispatch".to_string()));
    assert!(calls.contains(&"LocalStep".to_string()));
}

#[test]
fn test_cpp_class_methods_constructors_and_qualified_calls() {
    let src = r#"
class Logger {
public:
    Logger();
    ~Logger();
    void log(const char* message);
};

Logger::Logger() {}
Logger::~Logger() {}
void Logger::log(const char* message) {
    sink.write(message);
}

void setup() {
    Logger logger;
    logger.log("ready");
}
"#;
    let names = symbol_names("logger.cpp", src);
    assert!(names.contains(&"Logger".to_string()));
    assert!(names.contains(&"log".to_string()));
    assert!(names.contains(&"setup".to_string()));

    let calls = call_names("logger.cpp", src);
    assert!(calls.contains(&"write".to_string()));
    assert!(calls.contains(&"log".to_string()));
}

#[test]
fn test_cpp_namespaces_templates_and_operators() {
    let src = r#"
namespace workbench {
template <typename T>
class Box {
public:
    T value() const;
};

Box<int> operator+(const Box<int>& left, const Box<int>& right) {
    combine(left, right);
    return left;
}
}
"#;
    let names = symbol_names("box.cpp", src);
    assert!(names.contains(&"workbench".to_string()));
    assert!(names.contains(&"Box".to_string()));
    assert!(names.contains(&"value".to_string()));

    let calls = call_names("box.cpp", src);
    assert!(calls.contains(&"combine".to_string()));
}

#[test]
fn test_swift_protocol_subscript_deinit_and_operator() {
    let src = r#"
protocol Store {
    subscript(key: String) -> String? { get }
}

class Cache: Store {
    subscript(key: String) -> String? {
        return lookup(key)
    }

    deinit {
        cleanup()
    }
}

func + (left: Cache, right: Cache) -> Cache {
    return merge(left, right)
}
"#;
    let names = symbol_names("Cache.swift", src);
    assert!(names.contains(&"Store".to_string()));
    assert!(names.contains(&"subscript".to_string()));
    assert!(names.contains(&"Cache".to_string()));
    assert!(names.contains(&"deinit".to_string()));
    assert!(names.contains(&"+".to_string()));

    let calls = call_names("Cache.swift", src);
    assert!(calls.contains(&"lookup".to_string()));
    assert!(calls.contains(&"cleanup".to_string()));
    assert!(calls.contains(&"merge".to_string()));
}
