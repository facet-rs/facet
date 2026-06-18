# ur-taking-me-with-you

Utility crate for ensuring child processes terminate when their parent exits.

## Role in the Vox stack

`ur-taking-me-with-you` is an operational helper used by process-launch workflows around Vox components and test tooling.

## What this crate provides

- Parent-death wiring for child processes across supported platforms
- Sync and optional tokio-based process helpers

## Fits with

- Runtime/test harness process orchestration where process lifetimes must stay coupled

Part of the Vox workspace: <https://github.com/bearcove/vox>
