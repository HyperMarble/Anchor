struct EventLogLock {
    path: PathBuf,
}

impl EventLogLock {
    fn acquire(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join("events.lock");
        for _ in 0..500 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(err) => return Err(err.into()),
            }
        }
        anyhow::bail!(
            "timed out waiting for Anchor event log lock: {}",
            path.display()
        )
    }
}

impl Drop for EventLogLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn log_path(anchor_root: &Path) -> PathBuf {
    anchor_root.join("events").join("events.jsonl")
}

pub fn load(anchor_root: &Path) -> anyhow::Result<Vec<ExecutionEvent>> {
    let path = log_path(anchor_root);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    let mut corrupt_lines = 0usize;
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(&line) {
            Ok(event) => events.push(event),
            // One torn or corrupted line must not make the whole log — and
            // with it the write guard — unreadable.
            Err(_) => corrupt_lines += 1,
        }
    }
    if corrupt_lines > 0 {
        eprintln!("anchor: skipped {corrupt_lines} corrupt event log line(s)");
    }

    Ok(events)
}
