fn run_main_a() {
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
}
