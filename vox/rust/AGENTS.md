<INSTRUCTIONS>
You are working in `rust/`, which is the **new Rust implementation** targeting the canonical spec in `docs/content/`.

Rules:
- Prefer implementing new functionality in `rust/` only.
- Treat `rust-legacy/` as reference-only unless explicitly asked to modify it.
- Do not “shim” the legacy protocol into the new one; implement the canonical model directly (`Message`, `request_id`, `stream_id`, `Credit`, `Goodbye`, COBS framing, SHM layout).
- When you need reusable legacy code (e.g. `shm-primitives`), copy it into `rust/` and then update dependencies to point at the new location.
</INSTRUCTIONS>
