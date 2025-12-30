# Real Conformance Tests for rapace-core

## The Problem

The old conformance test setup was a lie. It tested the **conformance harness against itself**, not the actual `rapace-core` implementation. Meanwhile, the actual demos (`http-over-rapace`, etc.) were failing because `RpcSession` doesn't even do the Hello handshake - a fundamental spec requirement.

We had "100% spec coverage" while basic RPC calls timed out. The conformance tests validated that the harness could parse frames correctly, not that rapace-core actually implemented the protocol.

## The Solution

Test the **real rapace-core implementation** against a reference harness that knows the spec.

## Architecture

```
┌─────────────────────┐         pipes          ┌─────────────────────┐
│  rapace-spec-tester │ ◄────────────────────► │  rapace-spec-subject│
│                     │                        │                     │
│  - knows the spec   │                        │  - uses rapace-core │
│  - validates frames │                        │  - RpcSession       │
│  - reference impl   │                        │  - real handshake   │
│  - exits 0/1        │                        │  - real services    │
└─────────────────────┘                        └─────────────────────┘
         ▲                                              ▲
         │                                              │
         └──────────────────┬───────────────────────────┘
                            │
                   ┌────────▼────────┐
                   │   spec-tests    │
                   │                 │
                   │  - orchestrator │
                   │  - spawns both  │
                   │  - proxies I/O  │
                   │  - can spy      │
                   │  - reports      │
                   └─────────────────┘
```

## Crate Structure (FLAT)

All crates live at workspace root level:

```
spec-tester/              # rapace-spec-tester binary
├── Cargo.toml
├── src/
│   ├── main.rs           # CLI: --list, --case <name>
│   ├── harness.rs        # Peer abstraction for stdio communication
│   ├── testcase.rs       # TestResult, TestCase types
│   └── tests/            # Test implementations using #[conformance]
│       ├── mod.rs
│       ├── handshake.rs
│       ├── call.rs
│       ├── channel.rs
│       └── ...
└── tests/
    # (empty - no Rust integration tests here)

spec-tester-macros/       # Proc macro crate
├── Cargo.toml
└── src/
    └── lib.rs            # #[conformance(name = "...", rules = "...")]

spec-proto/               # Shared service definitions
├── Cargo.toml
└── src/
    └── lib.rs            # Services defined with rapace::service macro

spec-subject/             # Real rapace-core implementation under test
├── Cargo.toml
└── src/
    └── main.rs           # Uses RpcSession, implements spec-proto services

spec-tests/               # Test orchestrator
├── Cargo.toml
└── src/
    └── main.rs           # Spawns tester+subject, proxies, reports
```

## Dependencies

### spec-tester
- `spec-tester-macros` (for `#[conformance]` attribute)
- `spec-proto` (shared service definitions)
- `rapace-protocol` (frame types, constants)
- `facet`, `facet-postcard`, `facet-json` (serialization)
- `tokio`, `clap`, `inventory`
- **NO `rapace-core`** - this is a reference implementation

### spec-tester-macros
- `proc-macro2`, `quote` (proc macro basics)
- Nothing else

### spec-proto
- `rapace` (for `#[rapace::service]` macro)
- Defines traits that both tester and subject implement

### spec-subject
- `rapace-core` (the thing being tested!)
- `spec-proto` (implements the services)
- `tokio`

### spec-tests
- `libtest-mimic` (test harness)
- `facet-json` (parse test list)
- **NO `rapace-core`** - just spawns processes
- **NO `spec-proto`** - doesn't need to know the protocol

## spec-proto Services

These are the services used in conformance tests. Both tester and subject implement them.

```rust
// spec-proto/src/lib.rs

use rapace::service;

/// Simple echo service for testing basic RPC
#[service]
pub trait EchoService {
    /// Echo back the input data
    async fn echo(&self, data: Vec<u8>) -> Vec<u8>;
}

/// Health check service
#[service]
pub trait HealthService {
    /// Returns health status
    async fn health(&self) -> HealthResponse;
}

#[derive(facet::Facet)]
pub struct HealthResponse {
    pub status: String,
}

// Add more services as needed by conformance tests
```

## spec-subject Behavior

The subject uses real `rapace-core` and switches behavior based on `--case`:

```rust
// spec-subject/src/main.rs

use rapace_core::{RpcSession, StreamTransport};
use spec_proto::{EchoService, HealthService};

fn main() {
    let args = parse_args();
    let transport = StreamTransport::from_stdio();
    let session = RpcSession::new(transport);
    
    // Register services from spec-proto
    session.set_dispatcher(/* dispatcher using spec-proto services */);
    
    match args.case.as_str() {
        // Most tests: just run session (Hello + respond to requests)
        _ => {
            session.run().await;
        }
    }
    
    // Some tests may need specific behavior:
    // "call.one_req_one_resp" => make a specific call
    // But default is: Hello handshake + respond to whatever tester sends
}
```

## spec-tests Orchestrator

```rust
// spec-tests/src/main.rs

use std::process::{Command, Stdio};
use libtest_mimic::{Arguments, Trial};

fn main() {
    let args = Arguments::from_args();
    
    // 1. Get test list from tester
    let tester_bin = find_binary("rapace-spec-tester");
    let subject_bin = find_binary("rapace-spec-subject");
    
    let output = Command::new(&tester_bin)
        .args(["--list", "--format", "json"])
        .output()
        .expect("failed to list tests");
    
    let tests: Vec<TestCase> = facet_json::from_slice(&output.stdout)
        .expect("failed to parse test list");
    
    // 2. Create a trial for each test
    let trials: Vec<Trial> = tests
        .iter()
        .map(|test| {
            let name = test.name.clone();
            let tester = tester_bin.clone();
            let subject = subject_bin.clone();
            
            Trial::test(&name, move || {
                run_test(&tester, &subject, &name)
            })
        })
        .collect();
    
    libtest_mimic::run(&args, trials).exit();
}

fn run_test(tester: &str, subject: &str, case: &str) -> Result<(), Failed> {
    // Spawn tester
    let mut tester_proc = Command::new(tester)
        .args(["--case", case])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    
    // Spawn subject
    let mut subject_proc = Command::new(subject)
        .args(["--case", case])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    
    // Proxy I/O between them (can spy/log here)
    // tester.stdout -> subject.stdin
    // subject.stdout -> tester.stdin
    proxy_bidirectional(
        tester_proc.stdout.take().unwrap(),
        subject_proc.stdin.take().unwrap(),
        subject_proc.stdout.take().unwrap(),
        tester_proc.stdin.take().unwrap(),
    );
    
    // Wait for both to finish
    let tester_status = tester_proc.wait()?;
    let subject_status = subject_proc.wait()?;
    
    // Pass if tester exits 0
    if tester_status.success() {
        Ok(())
    } else {
        Err(Failed::from(format!(
            "tester exited with {:?}, subject exited with {:?}",
            tester_status.code(),
            subject_status.code()
        )))
    }
}
```

## Test Flow

1. **spec-tests** runs `rapace-spec-tester --list --format json` to get all test cases
2. For each test case:
   a. Spawn `rapace-spec-tester --case <name>` with piped stdio
   b. Spawn `rapace-spec-subject --case <name>` with piped stdio
   c. Proxy bytes between them:
      - tester's stdout → subject's stdin
      - subject's stdout → tester's stdin
   d. Optionally log/spy on the traffic for debugging
   e. Wait for both processes to exit
   f. **PASS** if tester exits 0, **FAIL** otherwise

## What Gets Tested

- **RpcSession::run()** - must do Hello handshake (currently doesn't!)
- **StreamTransport** - frame encoding/decoding
- **facet-postcard serialization** - protocol message encoding
- **Service dispatch** - handling incoming RPC calls
- **The actual rapace-core implementation** - not a fake

## Expected Outcome

Initially: **~200 failing tests** because:
- `RpcSession` doesn't do Hello handshake
- Various other spec violations

Then we fix rapace-core to be spec-compliant, and tests go green.

## Migration Steps

1. **Nuke old structure**
   - Delete `conformance/tests-runner/` entirely
   - Delete `conformance/tests/coverage.rs`

2. **Rename existing crates**
   - `conformance/` → `spec-tester/`
   - `conformance/macros/` → `spec-tester-macros/`
   - Update package names in Cargo.toml files
   - Update binary name to `rapace-spec-tester`

3. **Create new crates**
   - `spec-proto/` - shared service definitions
   - `spec-subject/` - real rapace-core runner
   - `spec-tests/` - test orchestrator

4. **Update workspace Cargo.toml**
   - Remove old members
   - Add new members

5. **Update spec-tester to use spec-proto**
   - Import service definitions from spec-proto
   - Update harness to use shared types

6. **Implement spec-subject**
   - Use real RpcSession, StreamTransport
   - Implement spec-proto services
   - Handle --case argument

7. **Implement spec-tests orchestrator**
   - List tests from tester
   - Spawn both processes
   - Proxy I/O
   - Report results

8. **Update Swift/TypeScript conformance**
   - Change binary name from `rapace-conformance` to `rapace-spec-tester`

9. **Update CI**
   - Update job names and commands

## File Changes Summary

### Delete
- `conformance/tests-runner/` (entire directory)
- `conformance/tests/coverage.rs`

### Rename/Move
- `conformance/` → `spec-tester/`
- `conformance/macros/` → `spec-tester-macros/`
- Package name: `rapace-conformance` → `rapace-spec-tester`
- Package name: `rapace-conformance-macros` → `rapace-spec-tester-macros`

### Create New
- `spec-proto/Cargo.toml`
- `spec-proto/src/lib.rs`
- `spec-subject/Cargo.toml`
- `spec-subject/src/main.rs`
- `spec-tests/Cargo.toml`
- `spec-tests/src/main.rs`

### Update
- Root `Cargo.toml` (workspace members)
- `.github/workflows/ci.yml` (binary names)
- `swift/Tests/RapaceTests/ConformanceTests.swift` (binary name)
- `typescript/...` (binary name)
