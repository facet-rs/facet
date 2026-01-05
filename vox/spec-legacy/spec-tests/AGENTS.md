<INSTRUCTIONS>
This directory is the conformance test harness.

Current state:
- The existing `spec-*` suite targets the **legacy** Rust wire model (length-prefixed frames + `MsgDescHot` descriptors).
- The canonical spec for the new protocol lives in `docs/content/` and differs significantly.

Rules:
- Avoid expanding the legacy protocol further unless explicitly requested.
- For new work, prefer adding a parallel suite under `spec/` that tests `docs/content/spec/_index.md` directly.
</INSTRUCTIONS>
