# Diff Theming Design

This document describes the color theming system for `facet-diff-core`.

## Accent Colors

Each diff operation has an **accent color** that establishes its identity:

| Operation | Accent | Example |
|-----------|--------|---------|
| Deleted / Before | Orange | `#ffa759` |
| Inserted / After | Blue | `#61afef` |
| Moved | Purple | `#c678dd` |

These accent colors are chosen to be **colorblind-friendly** (deuteranopia-safe), avoiding the traditional red/green which ~8% of males cannot distinguish.

## Derived Colors

From each accent color, we derive:

### Background Colors

1. **Line Background** - Very subtle, applied to the entire line
   - Low saturation, low brightness
   - Just enough to distinguish changed lines from unchanged context
   - Example: Orange accent `#ffa759` → Line bg `rgb(45, 30, 25)`

2. **Highlight Background** - Stronger, applied to changed values
   - Medium saturation, medium brightness
   - Clearly marks the specific content that changed
   - Example: Orange accent `#ffa759` → Highlight bg `rgb(80, 50, 35)`

### Text Colors

Text colors are blended based on their context:

1. **On dim background (line bg)** - Dimmed text
   - Syntax colors (keys, strings, numbers, punctuation) are muted
   - Provides context without drawing attention

2. **On highlight background** - Bright text
   - The accent color at full brightness
   - Draws the eye to the actual change

## Syntax Color Blending

The theme defines base syntax colors:

| Element | Role |
|---------|------|
| `key` | Field names in structs |
| `structure` | Brackets, braces, tags |
| `comment` | Type hints, muted annotations |
| `unchanged` | Unmodified content |

These base colors get **blended** with the current context:

```
final_color = blend(base_syntax_color, context_accent, blend_factor)
```

Where `blend_factor` varies:
- On line background: higher blend (more muted)
- On highlight background: lower blend (syntax colors shine through)

## Symbols

Different symbols distinguish operation types:

| Symbol | Meaning |
|--------|---------|
| `-` | Full line deleted |
| `+` | Full line inserted |
| `←` | Changed value (before state) |
| `→` | Changed value (after state) |
| `∅` | Empty slot (placeholder for alignment) |

## Implementation Notes

The `SemanticColor` enum captures the rendering intent:

```rust
pub enum SemanticColor {
    Deleted,           // Line bg + dimmed accent
    DeletedHighlight,  // Highlight bg + bright accent
    Inserted,          // Line bg + dimmed accent
    InsertedHighlight, // Highlight bg + bright accent
    Moved,             // Line bg + dimmed accent
    MovedHighlight,    // Highlight bg + bright accent
    Unchanged,         // No background
    Structure,         // Structural elements
    Comment,           // Muted annotations
}
```

The `ColorBackend` trait translates semantic colors to actual ANSI escape codes based on the active `DiffTheme`.

## Future Enhancements

- [ ] Dynamic blend factor calculation based on accent luminance
- [ ] Light theme support (invert brightness relationships)
- [ ] Per-syntax-element blend factors
- [ ] User-configurable accent colors with auto-derived backgrounds
