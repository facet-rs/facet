# vox-local

Compatibility shim for local IPC transport APIs.

The implementation now lives in `vox-stream`. This crate re-exports:

- raw local IPC APIs (`LocalListener`, `connect`, `endpoint_exists`, `remove_endpoint`)
- local `Link` APIs (`LocalLink`, `LocalLinkAcceptor`, `LocalLinkSource`)

Prefer importing directly from `vox-stream` for new code.
