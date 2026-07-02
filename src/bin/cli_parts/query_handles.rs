#[derive(Debug, Clone, Serialize)]
struct QueryReport {
    schema: &'static str,
    intent: String,
    scoped_files: usize,
    chunks: Vec<QueryChunk>,
    tests: Vec<QueryTestFile>,
    files: Vec<QueryFile>,
    next: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct QueryFile {
    handle: String,
    path: String,
    source_hash: String,
    score: i32,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct QueryChunk {
    handle: String,
    path: String,
    symbol: String,
    kind: String,
    line_start: usize,
    line_end: usize,
    source_hash: String,
    score: i32,
    reasons: Vec<String>,
    calls: Vec<String>,
    called_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct QueryTestFile {
    handle: String,
    path: String,
    score: usize,
    reasons: Vec<String>,
}

#[derive(Debug, Clone)]
enum QueryHandle {
    File(String),
    Test(String),
    Chunk {
        path: String,
        symbol: String,
        line_start: usize,
        line_end: usize,
    },
}

fn file_handle(path: &str) -> String {
    format!("file:{path}")
}

fn test_handle(path: &str) -> String {
    format!("test:{path}")
}

fn chunk_handle(path: &str, symbol: &str, line_start: usize, line_end: usize) -> String {
    format!("chunk:{path}#{symbol}@{line_start}-{line_end}")
}

fn parse_query_handle(handle: &str) -> Result<QueryHandle> {
    if let Some(path) = handle.strip_prefix("file:") {
        if path.is_empty() {
            bail!("file handle has no path");
        }
        return Ok(QueryHandle::File(path.to_string()));
    }
    if let Some(path) = handle.strip_prefix("test:") {
        if path.is_empty() {
            bail!("test handle has no path");
        }
        return Ok(QueryHandle::Test(path.to_string()));
    }
    let Some(rest) = handle.strip_prefix("chunk:") else {
        bail!("unknown handle kind: {handle}");
    };
    let Some((owner, range)) = rest.rsplit_once('@') else {
        bail!("chunk handle must end with @<start>-<end>");
    };
    let Some((path, symbol)) = owner.rsplit_once('#') else {
        bail!("chunk handle must include #<symbol>");
    };
    let Some((start, end)) = range.split_once('-') else {
        bail!("chunk handle range must be <start>-<end>");
    };
    let line_start = start.parse::<usize>()?;
    let line_end = end.parse::<usize>()?;
    if path.is_empty() || symbol.is_empty() || line_start == 0 || line_end < line_start {
        bail!("invalid chunk handle: {handle}");
    }
    Ok(QueryHandle::Chunk {
        path: path.to_string(),
        symbol: symbol.to_string(),
        line_start,
        line_end,
    })
}

fn line_count(text: &str) -> usize {
    text.lines().count()
}
