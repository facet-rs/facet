<INSTRUCTIONS>
This is the conformance “reference peer”.

Current state:
- This peer currently speaks the **legacy** frame protocol (length prefix + 64-byte descriptor + payload) via TCP.
- It does not yet implement the `docs/content/spec/_index.md` `Message` model (request_id/stream_id/Credit/Goodbye) or COBS framing.

Rules:
- Treat this as legacy until explicitly migrated.
- For new work, prefer creating a new peer implementation under `spec/` that uses `rust/rapace-wire` + `rust/rapace-codec`.
</INSTRUCTIONS>
