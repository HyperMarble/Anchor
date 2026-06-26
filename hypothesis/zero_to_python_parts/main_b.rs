fn run_main_b() {
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

}
