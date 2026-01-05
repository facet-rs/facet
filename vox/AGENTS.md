<INSTRUCTIONS>
Repo focus (Jan 2026):

- `docs/content/` is the canonical specification and should be treated as read-only unless explicitly asked to edit it.
- `rust/` is the new Rust implementation. Prefer adding new code here.
- `rust-legacy/` contains the legacy Rust implementation. Treat it as reference-only unless explicitly asked to modify it.

Workflow guidance:
- If you need reusable legacy components (e.g. `shm-primitives`), copy them into `rust/` and update dependencies to point to the new location.
- Avoid building compatibility shims; implement the canonical spec directly.
</INSTRUCTIONS>
