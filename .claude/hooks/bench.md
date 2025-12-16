Run benchmarks or view results.

Arguments: $ARGUMENTS (optional: "view" to serve existing results without re-running)

**To run benchmarks and serve results:**
```
cargo xtask bench-report --serve
```
Runs all benchmarks then serves interactive report at http://localhost:1999

**To view existing results (no re-run):**
```
cargo xtask bench-report --serve --no-run
```
Useful when results were generated elsewhere (e.g., remote server).

**To just generate results (no server):**
```
cargo xtask bench-report
```
Dumps results to file for later viewing.

Note: These are long-running commands. Start in background or let the user run them directly.
