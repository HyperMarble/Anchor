# Anchor Benchmark Suite

Compares Anchor against real traditional tools (grep, find, cat).

## Running Benchmarks

### CLI Tool
```bash
# Run on Anchor's own codebase
./target/release/anchor-bench /Users/hak/Anchor

# Run on any repository
./target/release/anchor-bench /path/to/your/repo
```

### Rust Tests
```bash
cargo test -p anchor-sdk --test benchmark
```

## What It Measures

| Metric | Description |
|--------|-------------|
| Tool Calls | Number of shell commands executed |
| Time | Execution time in milliseconds |
| Output Tokens | Size of results (estimated) |

## Tests

- `test_real_benchmark_runs` - Verifies benchmark executes correctly
- `test_traditional_tools_actually_run` - Confirms traditional tools are measured

## Expected Results

```
TOOL CALLS:
  Traditional: 28
  Anchor:       8
  Reduction:   71.4%

SPEED:
  Anchor is 2036ms faster overall
```

## Files

- `benchmark.rs` - Main benchmark implementation
- `tests/` - Rust test module
