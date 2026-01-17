# TODO-003: Revise Spec - Expressions as 2D (tag, payload)

## Status
TODO

## Description
Revise the specification to talk about Styx expressions as being two-dimensional: (tag, payload).

This conceptual framing helps users understand that every Styx value has two components:
1. **Tag** - The type/variant indicator (defaults to `@` for untagged values)
2. **Payload** - The actual content (scalar, sequence, or object)

## Files to Update
- `docs/content/spec/parser.md` - Core spec document
- `docs/content/_index.md` - Homepage primer (possibly)

## Notes
- This is a conceptual/pedagogical improvement
- Makes the tag system more intuitive
- Clarifies that `@` is not special, it's the default/empty tag
