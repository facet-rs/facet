# Theming Implementation Brief

This document describes how to implement the color theming system from `theming-design.md`.

## Overview

The goal is to make syntax colors (keys, structure, comments) blend with the current diff context's accent color, creating a cohesive visual effect where deleted lines feel "warm" (orange-tinted) and inserted lines feel "cool" (blue-tinted).

## Dependencies

Add to `facet-diff-core/Cargo.toml`:
```toml
palette = { version = "0.7", default-features = false, features = ["std"] }
```

## Step 1: Add Context-Aware Semantic Colors

Currently `SemanticColor` has generic `Structure` and `Comment` variants. We need context-aware variants.

**File: `src/layout/backend.rs`**

```rust
/// Semantic color meaning for diff elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticColor {
    // === Accent colors (full brightness) ===
    /// Deleted content on line background
    Deleted,
    /// Deleted content on highlight background (the actual changed value)
    DeletedHighlight,
    /// Inserted content on line background
    Inserted,
    /// Inserted content on highlight background (the actual changed value)
    InsertedHighlight,
    /// Moved content on line background
    Moved,
    /// Moved content on highlight background
    MovedHighlight,

    // === Syntax colors (context-aware) ===
    /// Key/field name in deleted context (blended toward orange)
    DeletedKey,
    /// Key/field name in inserted context (blended toward blue)
    InsertedKey,
    /// Key/field name in unchanged context
    Key,

    /// Structural element in deleted context
    DeletedStructure,
    /// Structural element in inserted context
    InsertedStructure,
    /// Structural element in unchanged context
    Structure,

    /// Comment/type hint in deleted context
    DeletedComment,
    /// Comment/type hint in inserted context
    InsertedComment,
    /// Comment in unchanged context
    Comment,

    // === Other ===
    /// Unchanged content (neutral)
    Unchanged,
}
```

## Step 2: Add Color Blending to DiffTheme

**File: `src/theme.rs`**

```rust
use palette::{LinSrgb, Srgb, Mix};

impl DiffTheme {
    /// Blend two colors in linear sRGB space.
    /// `t` ranges from 0.0 (all `a`) to 1.0 (all `b`).
    pub fn blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
        // Convert to linear sRGB for perceptually correct blending
        let a_lin: LinSrgb = Srgb::new(a.0 as f32 / 255.0, a.1 as f32 / 255.0, a.2 as f32 / 255.0).into_linear();
        let b_lin: LinSrgb = Srgb::new(b.0 as f32 / 255.0, b.1 as f32 / 255.0, b.2 as f32 / 255.0).into_linear();

        // Mix in linear space
        let mixed = a_lin.mix(b_lin, t);

        // Convert back to sRGB
        let result: Srgb = mixed.into();
        Rgb(
            (result.red * 255.0).round() as u8,
            (result.green * 255.0).round() as u8,
            (result.blue * 255.0).round() as u8,
        )
    }

    /// Get the key color blended for a deleted context.
    pub fn deleted_key(&self) -> Rgb {
        Self::blend(self.key, self.deleted, 0.3)
    }

    /// Get the key color blended for an inserted context.
    pub fn inserted_key(&self) -> Rgb {
        Self::blend(self.key, self.inserted, 0.3)
    }

    /// Get the structure color blended for a deleted context.
    pub fn deleted_structure(&self) -> Rgb {
        Self::blend(self.structure, self.deleted, 0.25)
    }

    /// Get the structure color blended for an inserted context.
    pub fn inserted_structure(&self) -> Rgb {
        Self::blend(self.structure, self.inserted, 0.25)
    }

    /// Get the comment color blended for a deleted context.
    pub fn deleted_comment(&self) -> Rgb {
        Self::blend(self.comment, self.deleted, 0.2)
    }

    /// Get the comment color blended for an inserted context.
    pub fn inserted_comment(&self) -> Rgb {
        Self::blend(self.comment, self.inserted, 0.2)
    }
}
```

### Blend Factors

| Element | Blend Factor | Rationale |
|---------|-------------|-----------|
| Key | 0.30 | Field names should clearly show context |
| Structure | 0.25 | Brackets/braces are less important |
| Comment | 0.20 | Already muted, subtle tint is enough |

## Step 3: Update AnsiBackend

**File: `src/layout/backend.rs`**

```rust
impl ColorBackend for AnsiBackend {
    fn write_styled<W: Write>(
        &self,
        w: &mut W,
        text: &str,
        color: SemanticColor,
    ) -> std::fmt::Result {
        let (fg, bg) = match color {
            // Accent colors
            SemanticColor::Deleted => (self.theme.deleted, self.theme.deleted_line_bg),
            SemanticColor::DeletedHighlight => (self.theme.deleted, self.theme.deleted_highlight_bg),
            SemanticColor::Inserted => (self.theme.inserted, self.theme.inserted_line_bg),
            SemanticColor::InsertedHighlight => (self.theme.inserted, self.theme.inserted_highlight_bg),
            SemanticColor::Moved => (self.theme.moved, self.theme.moved_line_bg),
            SemanticColor::MovedHighlight => (self.theme.moved, self.theme.moved_highlight_bg),

            // Context-aware syntax colors
            SemanticColor::DeletedKey => (self.theme.deleted_key(), self.theme.deleted_line_bg),
            SemanticColor::InsertedKey => (self.theme.inserted_key(), self.theme.inserted_line_bg),
            SemanticColor::Key => (self.theme.key, None),

            SemanticColor::DeletedStructure => (self.theme.deleted_structure(), self.theme.deleted_line_bg),
            SemanticColor::InsertedStructure => (self.theme.inserted_structure(), self.theme.inserted_line_bg),
            SemanticColor::Structure => (self.theme.structure, None),

            SemanticColor::DeletedComment => (self.theme.deleted_comment(), self.theme.deleted_line_bg),
            SemanticColor::InsertedComment => (self.theme.inserted_comment(), self.theme.inserted_line_bg),
            SemanticColor::Comment => (self.theme.comment, None),

            // Neutral
            SemanticColor::Unchanged => (self.theme.unchanged, None),
        };

        if let Some(bg) = bg {
            write!(w, "{}", text.color(fg).on_color(bg))
        } else {
            write!(w, "{}", text.color(fg))
        }
    }
}
```

## Step 4: Update Render Code

**File: `src/layout/render.rs`**

Add a helper to get context-aware semantic colors:

```rust
/// Get the appropriate semantic color for a syntax element in a given context.
fn syntax_color(base: SyntaxElement, context: ElementChange) -> SemanticColor {
    match (base, context) {
        (SyntaxElement::Key, ElementChange::Deleted) => SemanticColor::DeletedKey,
        (SyntaxElement::Key, ElementChange::Inserted) => SemanticColor::InsertedKey,
        (SyntaxElement::Key, _) => SemanticColor::Key,

        (SyntaxElement::Structure, ElementChange::Deleted) => SemanticColor::DeletedStructure,
        (SyntaxElement::Structure, ElementChange::Inserted) => SemanticColor::InsertedStructure,
        (SyntaxElement::Structure, _) => SemanticColor::Structure,

        (SyntaxElement::Comment, ElementChange::Deleted) => SemanticColor::DeletedComment,
        (SyntaxElement::Comment, ElementChange::Inserted) => SemanticColor::InsertedComment,
        (SyntaxElement::Comment, _) => SemanticColor::Comment,
    }
}

#[derive(Clone, Copy)]
enum SyntaxElement {
    Key,
    Structure,
    Comment,
}
```

Then update rendering functions to pass context:

```rust
// Before:
opts.backend.write_styled(w, &open, SemanticColor::Structure)?;

// After (in deleted context):
opts.backend.write_styled(w, &open, SemanticColor::DeletedStructure)?;
```

## Step 5: Update Inline Element Rendering

The `render_inline_element` function already tracks context (deleted vs inserted lines). Update it to use context-aware colors for structural elements:

```rust
// On the "before" line (deleted context):
opts.backend.write_styled(w, &open, SemanticColor::DeletedStructure)?;
opts.backend.write_styled(w, &flavor.format_field_prefix(&attr.name), SemanticColor::DeletedKey)?;

// On the "after" line (inserted context):
opts.backend.write_styled(w, &open, SemanticColor::InsertedStructure)?;
opts.backend.write_styled(w, &flavor.format_field_prefix(&attr.name), SemanticColor::InsertedKey)?;
```

## Step 6: Testing

1. Run existing tests: `cargo nextest run -p facet-diff-core`
2. Update snapshots if output format changed: `cargo insta review`
3. Visual inspection with a test binary that outputs colored diff

## Visual Result

Before:
```
← Point { x: 10, y: 20 }   // All same gray for structure/keys
→ Point { x: 15, y: 20 }
```

After:
```
← Point { x: 10, y: 20 }   // Structure/keys tinted orange
→ Point { x: 15, y: 20 }   // Structure/keys tinted blue
```

The effect is subtle but creates visual cohesion - your eye can follow the "warm" deleted line and "cool" inserted line more easily.

## Future Enhancements

1. **Configurable blend factors** - Let users adjust how much context affects syntax colors
2. **Oklab blending** - Even more perceptually uniform than linear sRGB
3. **Light theme support** - Invert brightness relationships, use darker accents
4. **Moved context colors** - `MovedKey`, `MovedStructure`, `MovedComment` (purple-tinted)
