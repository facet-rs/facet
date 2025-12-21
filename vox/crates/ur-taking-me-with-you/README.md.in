# ur-taking-me-with-you

[![crates.io](https://img.shields.io/crates/v/ur-taking-me-with-you.svg)](https://crates.io/crates/ur-taking-me-with-you)
[![documentation](https://docs.rs/ur-taking-me-with-you/badge.svg)](https://docs.rs/ur-taking-me-with-you)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/ur-taking-me-with-you.svg)](./LICENSE)

Ensure child processes die when their parent dies.

Child processes normally continue running even after their parent exits (on Unix they get reparented to init/PID 1). This crate provides mechanisms to ensure child processes are terminated when their parent dies, even if the parent is killed with SIGKILL.

Originally created for the [rapace](https://github.com/bearcove/rapace) RPC framework to manage plugin processes.

## Platform Support

- **Linux**: Uses `prctl(PR_SET_PDEATHSIG, SIGKILL)` - the child receives SIGKILL when its parent thread dies
- **macOS**: Uses a pipe-based approach - the child monitors a pipe from the parent and exits when the pipe closes (indicating parent death)
- **Windows**: Uses job objects with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` - all processes in the job are terminated when the last handle closes

## Usage

### For the child process (call early in main):

```rust
ur_taking_me_with_you::die_with_parent();
```

### For spawning children with std::process::Command:

```rust,no_run
use std::process::Command;

let mut cmd = Command::new("my-plugin");
cmd.arg("--foo");

let child = ur_taking_me_with_you::spawn_dying_with_parent(cmd)
    .expect("failed to spawn");
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/rapace/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/rapace/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
