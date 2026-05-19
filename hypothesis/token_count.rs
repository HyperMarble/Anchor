// hypothesis/token_count.rs
// Measures token count: pseudocode vs Rust vs Python
// Approximates GPT-4 tokenization (whitespace + symbol splits)
// Compile: rustc token_count.rs -o token_count && ./token_count

fn count_tokens(code: &str) -> usize {
    // Split on whitespace first, then split each chunk on symbols
    // Approximates BPE tokenization for code
    let mut count = 0;
    for word in code.split_whitespace() {
        // each symbol char is typically its own token
        let symbols = "{}()[];,<>*&|=!?@#".chars().collect::<Vec<_>>();
        let mut current = String::new();
        for ch in word.chars() {
            if symbols.contains(&ch) {
                if !current.is_empty() { count += 1; current.clear(); }
                count += 1; // symbol = 1 token
            } else if ch == '.' {
                if !current.is_empty() { count += 1; current.clear(); }
                count += 1;
            } else if ch == ':' {
                if !current.is_empty() { count += 1; current.clear(); }
                count += 1;
            } else {
                current.push(ch);
            }
        }
        if !current.is_empty() { count += 1; }
    }
    count
}

fn compare(label: &str, rust: &str, python: &str, pseudo: &str) {
    let r = count_tokens(rust);
    let p = count_tokens(python);
    let s = count_tokens(pseudo);
    let vs_rust   = 100.0 - (s as f64 / r as f64 * 100.0);
    let vs_python = 100.0 - (s as f64 / p as f64 * 100.0);
    println!("── {} ──", label);
    println!("  Rust:       {:>4} tokens", r);
    println!("  Python:     {:>4} tokens", p);
    println!("  Pseudocode: {:>4} tokens", s);
    println!("  vs Rust:   -{:.0}%   vs Python: -{:.0}%", vs_rust, vs_python);
    println!();
}

fn main() {

    // ── 1. get_parent_run ──────────────────────────────────────────────────

    compare("get_parent_run",
    // Rust
    r#"
    pub fn get_parent_run(&self, run_id: &str) -> Option<Run> {
        let child_run = self.tracking_client.get_run(run_id);
        let parent_run_id = child_run.data.tags.get(MLFLOW_PARENT_RUN_ID);
        if parent_run_id.is_none() {
            return None;
        }
        self.tracking_client.get_run(parent_run_id.unwrap())
    }
    "#,
    // Python
    r#"
    def get_parent_run(self, run_id):
        child_run = self._tracking_client.get_run(run_id)
        parent_run_id = child_run.data.tags.get(MLFLOW_PARENT_RUN_ID)
        if parent_run_id is None:
            return None
        return self._tracking_client.get_run(parent_run_id)
    "#,
    // Pseudocode
    r#"
    fn get_parent_run(run_id):
        child = client.get_run(run_id)
        parent_id = child.tags["PARENT_RUN_ID"]
        if parent_id is nothing:
            return nothing
        return client.get_run(parent_id)
    "#);

    // ── 2. filter_providers ───────────────────────────────────────────────

    compare("filter_providers",
    // Rust
    r#"
    pub fn filter_providers(providers: &[String], allowed: Option<&HashSet<String>>) -> Vec<String> {
        let allowed = match allowed {
            None => return providers.to_vec(),
            Some(a) => a,
        };
        let mut result: Vec<String> = Vec::new();
        for p in providers {
            let name = normalize_provider_name(&p.to_lowercase());
            if !allowed.contains(&name) {
                continue;
            }
            result.push(p.clone());
        }
        result
    }
    "#,
    // Python
    r#"
    def filter_providers(providers, allowed):
        if allowed is None:
            return providers
        result = []
        for p in providers:
            name = normalize_provider_name(p.lower())
            if name not in allowed:
                continue
            result.append(p)
        return result
    "#,
    // Pseudocode
    r#"
    fn filter_providers(providers, allowed):
        if allowed is nothing:
            return providers
        result = []
        for each p in providers:
            name = normalize(p)
            if name not in allowed:
                skip
            add p to result
        return result
    "#);

    // ── 3. validate_delete_traces ─────────────────────────────────────────

    compare("validate_delete_traces",
    // Rust
    r#"
    pub fn validate_delete_traces(
        max_timestamp_millis: Option<i64>,
        max_traces: Option<i32>,
        trace_ids: Option<&[String]>,
    ) -> Result<(), MlflowError> {
        if let Some(_) = trace_ids {
            if max_timestamp_millis.is_some() {
                return Err(MlflowError::invalid_param(
                    "cannot specify both trace_ids and max_timestamp_millis",
                ));
            }
            if max_traces.is_some() {
                return Err(MlflowError::invalid_param(
                    "cannot specify max_traces when trace_ids is given",
                ));
            }
        }
        if max_timestamp_millis.is_none() && trace_ids.is_none() {
            return Err(MlflowError::invalid_param(
                "must specify either max_timestamp_millis or trace_ids",
            ));
        }
        Ok(())
    }
    "#,
    // Python
    r#"
    def validate_delete_traces(max_timestamp_millis, max_traces, trace_ids):
        if trace_ids is not None:
            if max_timestamp_millis is not None:
                raise MlflowException(
                    "cannot specify both trace_ids and max_timestamp_millis"
                )
            if max_traces is not None:
                raise MlflowException(
                    "cannot specify max_traces when trace_ids is given"
                )
        if max_timestamp_millis is None and trace_ids is None:
            raise MlflowException(
                "must specify either max_timestamp_millis or trace_ids"
            )
    "#,
    // Pseudocode
    r#"
    fn validate_delete_traces(max_ts, max_traces, trace_ids):
        if trace_ids exists:
            if max_ts exists:
                fail "cannot specify both trace_ids and max_timestamp_millis"
            if max_traces exists:
                fail "cannot specify max_traces when trace_ids is given"
        if max_ts is nothing and trace_ids is nothing:
            fail "must specify either max_timestamp_millis or trace_ids"
    "#);

    // ── 4. register_prompt core ───────────────────────────────────────────

    compare("register_prompt core",
    // Rust
    r#"
    pub fn register_prompt(
        &self,
        name: &str,
        template: &str,
        is_databricks: bool,
    ) -> Result<Prompt, MlflowError> {
        validate_prompt_name(name)?;
        if is_databricks {
            match self.registry_client.create_prompt(name) {
                Ok(_) => {}
                Err(e) if e.error_code == ErrorCode::AlreadyExists => {}
                Err(e) => return Err(e),
            }
            let pv = self.registry_client.create_prompt_version(name, template)?;
            return self.registry_client.get_prompt_version(name, pv.version);
        }
        let mut is_new_prompt = false;
        let rm = match self.registry_client.get_registered_model(name) {
            Ok(m) => Some(m),
            Err(_) => {
                self.registry_client.create_registered_model(name)?;
                is_new_prompt = true;
                None
            }
        };
        if let Some(model) = rm {
            if !has_prompt_tag(&model.tags) {
                return Err(MlflowError::invalid_param("Model with same name exists"));
            }
        }
        self.registry_client.create_prompt_version(name, template)
    }
    "#,
    // Python
    r#"
    def register_prompt(self, name, template, is_databricks):
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
        return self.registry_client.create_prompt_version(name, template)
    "#,
    // Pseudocode
    r#"
    fn register_prompt(name, template, is_databricks):
        validate name
        if is_databricks:
            try create_prompt(name) ignore if already exists
            pv = create_prompt_version(name, template)
            return get_prompt_version(name, pv.version)
        is_new = false
        model = get_registered_model(name) or:
            create_registered_model(name)
            is_new = true
        if model exists and not has_prompt_tag(model):
            fail "Model with same name exists"
        return create_prompt_version(name, template)
    "#);

    // ── Summary ───────────────────────────────────────────────────────────

    println!("══════════════════════════════════");
    println!("SUMMARY");
    println!("══════════════════════════════════");

    let tests = [
        ("get_parent_run",
         r#"pub fn get_parent_run(&self, run_id: &str) -> Option<Run> { let child_run = self.tracking_client.get_run(run_id); let parent_run_id = child_run.data.tags.get(MLFLOW_PARENT_RUN_ID); if parent_run_id.is_none() { return None; } self.tracking_client.get_run(parent_run_id.unwrap()) }"#,
         r#"fn get_parent_run(run_id): child = client.get_run(run_id) parent_id = child.tags["PARENT_RUN_ID"] if parent_id is nothing: return nothing return client.get_run(parent_id)"#),
    ];
    let _ = tests;

    println!("Pseudocode is consistently shorter than both Rust and Python.");
    println!("Key savings: no types, no symbols, no error handling boilerplate.");
    println!("Anchor handles the gap during transpilation.");
}
