# styx-parse Spec Compliance Audit

This document audits the styx-parse crate against the parser spec (`docs/content/spec/parser.md`).

## Summary

| Rule | Status | Notes |
|------|--------|-------|
| comment.line | ⚠️ Partial | Missing "must be preceded by whitespace" check |
| comment.doc | ⚠️ Partial | Missing "must be followed by entry" validation |
| scalar.bare.chars | ✅ OK | Correctly excludes forbidden chars |
| scalar.bare.termination | ✅ OK | Terminates on forbidden char or EOF |
| scalar.quoted.escapes | ⚠️ Partial | Missing `\uXXXX` (4-digit), only has `\u{...}` |
| scalar.raw.syntax | ✅ OK | Hash count matching works |
| scalar.heredoc.syntax | ⚠️ Partial | Missing delimiter validation (uppercase, max 16 chars) |
| value.unit | ✅ OK | `@` alone is unit |
| tag.syntax | ⚠️ Partial | Missing validation of `[A-Za-z_][A-Za-z0-9_.-]*` pattern |
| tag.payload | ❌ Missing | Tag payloads (`@tag{...}`, `@tag(...)`, `@tag"..."`) NOT implemented |
| sequence.syntax | ⚠️ Partial | Parses but doesn't emit proper events for contents |
| sequence.elements | ⚠️ Partial | Elements not properly parsed |
| object.syntax | ✅ OK | Braces work |
| object.separators | ❌ Missing | Mixed separator detection NOT enforced |
| entry.structure | ✅ OK | 1/2/N atom handling works |
| entry.keypath | ✅ OK | Nested key paths work |
| entry.keys | ⚠️ Partial | Heredoc key rejection not checked |
| entry.key-equality | ❌ Missing | Duplicate key detection NOT implemented |
| attr.syntax | ❌ Missing | Attribute syntax (`key=value`) NOT implemented |
| attr.values | ❌ Missing | N/A (attr.syntax missing) |
| attr.atom | ❌ Missing | N/A (attr.syntax missing) |
| entry.keypath.attributes | ❌ Missing | N/A (attr.syntax missing) |
| document.root | ✅ OK | Implicit root object works |

## Detailed Findings

### Critical Issues (❌ Missing)

#### 1. Tag Payloads (`tag.payload`)

**Spec:**
> A tag MAY be immediately followed (no whitespace) by a payload:
> - `{...}` → tagged object
> - `(...)` → tagged sequence  
> - `"..."`, `r#"..."#`, `<<HEREDOC` → tagged scalar
> - `@` → tagged unit (explicit)
> - *(nothing)* → tagged unit (implicit)

**Current code** (`parse_tag_or_unit_atom`):
```rust
fn parse_tag_or_unit_atom(&mut self) -> Atom<'src> {
    let at = self.advance().unwrap(); // consume '@'
    // ...
    if let Some(token) = self.peek_raw()
        && token.kind == TokenKind::BareScalar
        && token.span.start == start_span.end
    {
        // Tag name immediately follows @
        let name_token = self.advance().unwrap();
        return Atom { ... };
    }
    // Just @ (unit)
    Atom { ... content: AtomContent::Unit }
}
```

**Problem:** After getting the tag name, it doesn't check for payload (`{`, `(`, `"`, etc.). Examples that SHOULD work but DON'T:
- `@err{message "x"}` → tagged object
- `@rgb(255 128 0)` → tagged sequence
- `@nickname"Bob"` → tagged scalar

#### 2. Attribute Syntax (`attr.syntax`, `attr.values`, `attr.atom`)

**Spec:**
> Attribute syntax `key=value` creates an object entry.
> ```styx
> server host=localhost port=8080
> ```
> Is equivalent to:
> ```styx
> server {host localhost, port 8080}
> ```

**Current code:** The lexer produces `TokenKind::Eq` for `=`, but the parser NEVER uses it. The `collect_entry_atoms` function doesn't handle `=` at all.

#### 3. Duplicate Key Detection (`entry.key-equality`)

**Spec:**
> Duplicate keys are forbidden.

**Current code:** No duplicate key detection whatsoever. The parser happily accepts:
```styx
{a 1, a 2, a 3}
```

#### 4. Separator Mode Enforcement (`object.separators`)

**Spec:**
> An object MUST use exactly one separator mode:
> - **newline-separated**: entries separated by newlines; commas forbidden
> - **comma-separated**: entries separated by commas; newlines forbidden

**Current code:** `skip_whitespace_and_newlines()` is called unconditionally, mixing modes freely.

### Partial Issues (⚠️)

#### 5. Comment Positioning (`comment.line`)

**Spec:**
> Comments MUST either start at the beginning of the file or be preceded by whitespace.
> ```styx
> url https://example.com  // the :// is not a comment
> ```

**Current code:** The lexer treats `//` as always starting a comment. The check for "preceded by whitespace" is missing.

**Note:** The example `url https://example.com` currently lexes correctly as a bare scalar because `/` is allowed after the first character. But `foo//bar` would incorrectly be parsed as `foo` followed by a comment.

#### 6. Doc Comment Validation (`comment.doc`)

**Spec:**
> A doc comment not followed by an entry (blank line or EOF) is an error.

**Current code:** Doc comments are emitted but no validation that they're followed by an entry.

#### 7. Unicode Escape (`scalar.quoted.escapes`)

**Spec:**
> `\uXXXX`, `\u{X...}`

**Current code:** Only `\u{...}` is implemented, not `\uXXXX` (4-digit fixed).

#### 8. Heredoc Delimiter Validation (`scalar.heredoc.syntax`)

**Spec:**
> The delimiter MUST match `[A-Z][A-Z0-9_]*` and not exceed 16 characters.

**Current code:** 
```rust
while let Some(c) = self.peek() {
    if c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' {
        // ...
    }
}
```
Missing: first char must be uppercase letter, max 16 chars.

#### 9. Tag Name Validation (`tag.syntax`)

**Spec:**
> A tag MUST match the pattern `@[A-Za-z_][A-Za-z0-9_.-]*`.

**Current code:** Just checks that a bare scalar follows `@` with no gap. No validation of the pattern (allows invalid chars, doesn't require letter/underscore start).

#### 10. Sequence Content Parsing (`sequence.syntax`, `sequence.elements`)

**Current code:** `parse_sequence_atom` just counts parens and returns an empty atom:
```rust
fn parse_sequence_atom(&mut self) -> Atom<'src> {
    // ...counts parens...
    Atom {
        content: AtomContent::Sequence,  // No actual content!
    }
}
```

And `emit_atom_as_value`:
```rust
AtomContent::Sequence => {
    // Re-parse the sequence content
    // For now, emit as empty sequence  // <-- BUG!
    callback.event(Event::SequenceStart { span: atom.span })
    callback.event(Event::SequenceEnd { span: atom.span })
}
```

#### 11. Heredoc Keys (`entry.keys`)

**Spec:**
> Heredoc scalars are not allowed as keys.

**Current code:** No check for this.

## Required Fixes (Priority Order)

1. **Tag payloads** - Core functionality missing
2. **Attribute syntax** - Core functionality missing  
3. **Sequence/Object content parsing** - Currently emits empty containers
4. **Duplicate key detection** - Data integrity
5. **Separator mode enforcement** - Spec compliance
6. **Tag name validation** - Spec compliance
7. **Heredoc delimiter validation** - Spec compliance
8. **Comment positioning** - Edge case
9. **Doc comment validation** - Nice to have
10. **Unicode escape \uXXXX** - Nice to have

## Tracey Annotations Needed

Once fixed, add `[impl r[rule.name]]` comments to the implementing code, e.g.:

```rust
// [impl r[tag.payload]]
fn parse_tag_payload(&mut self, tag_name: &str) -> Option<Atom<'src>> {
    // ...
}
```

And `[verify r[rule.name]]` in tests:

```rust
#[test]
// [verify r[tag.payload]]
fn test_tagged_object() {
    let events = parse("result @err{message x}");
    // ...
}
```
