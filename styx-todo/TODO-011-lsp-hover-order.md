# TODO-011: LSP Hover Display Order

## Status
TODO

## Description
Fix the order of information displayed in hover tooltips to match user expectations.

## Current Order
1. Breadcrumb path (`@ > syntax_highlight`)
2. Type annotation (`**Type:** @optional(@object{...})`)
3. Doc comment (if present)
4. Schema link (`Defined in ...`)

## Expected Order
1. Doc comment (most important - what does this field do?)
2. Breadcrumb path (where am I?)
3. Type annotation (what type is it?)
4. Schema link (`Defined at schema.styx:LINE:COL`)

## Implementation
Edit `format_field_hover()` in `crates/styx-lsp/src/server.rs` (~line 1783).

## Notes
- Change "Defined in" to "Defined at" with line:col format
- Keep the link clickable for external schemas
- For cached embedded schemas, link to the cache file path
