# 014b - Deserializer Diagnostics

**Status**: TODO  
**Parent**: 014-TODO-diagnostics.md  
**Depends on**: 014a (parser diagnostics pattern)

## Goal

Add ariadne-based error rendering to `facet-styx` for nice deserializer error messages.

## Implementation

1. Add `ariadne` dependency to `facet-styx`
2. Add `render()` method to `StyxError`
3. Add snapshot tests using `insta`

## Files to modify

- `crates/facet-styx/Cargo.toml` - add ariadne, insta
- `crates/facet-styx/src/error.rs` - add render method
- `crates/facet-styx/src/snapshots/` - snapshot test outputs

## Spec coverage

Deserializer diagnostics from `r[diagnostic.deser.*]`:
- [ ] `r[diagnostic.deser.invalid-value]`
- [ ] `r[diagnostic.deser.enum-invalid]`
- [ ] `r[diagnostic.deser.unknown-variant]`
- [ ] `r[diagnostic.deser.missing-field]`
- [ ] `r[diagnostic.deser.unknown-field]`
- [ ] `r[diagnostic.deser.expected-object]`
- [ ] `r[diagnostic.deser.expected-sequence]`

## Notes

`StyxError` already has `span: Option<Span>` - just need to add rendering.
