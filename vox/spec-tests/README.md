# rapace-spec-tests

Test orchestrator for rapace spec conformance.

This binary:
1. Lists test cases from `rapace-spec-tester --list --format json`
2. For each test, spawns both `rapace-spec-tester` and `rapace-spec-subject`
3. Proxies stdin/stdout between them (can spy on traffic for debugging)
4. Reports pass/fail based on tester exit code

## Usage

```bash
# Run all tests
cargo run -p rapace-spec-tests

# Run specific test
cargo run -p rapace-spec-tests -- handshake.valid_hello_exchange

# List all tests
cargo run -p rapace-spec-tests -- --list
```
