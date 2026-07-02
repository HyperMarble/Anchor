fn run_main_c() {
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
