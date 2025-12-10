# Type-Aware Value Coloring Implementation Plan

## Overview

This document describes how to implement type-aware value coloring in the diff renderer. Currently, the theming infrastructure supports type-specific colors (string, number, boolean, null) that blend with accent colors (yellow for deleted, blue for inserted), but the rendering pipeline doesn't track value types, so these colors aren't used yet.

## Current State

### What's Already Implemented

1. **Theme Infrastructure** (`theme.rs`)
   - Base colors for value types: `string`, `number`, `boolean`, `null`
   - Blending methods: `deleted_string()`, `inserted_number()`, etc.
   - 40% blend factor for value types (stronger than structure/keys)

2. **Semantic Color Variants** (`layout/backend.rs`)
   - Context-aware variants: `DeletedString`, `InsertedNumber`, etc.
   - `AnsiBackend` handles all variants with appropriate backgrounds

3. **Color Blending**
   - Linear sRGB blending for perceptually correct color mixing
   - Different blend factors for different element types

### The Problem

Values are formatted to strings early in the pipeline, losing type information:

```rust
// In build.rs: formatting happens here
let formatted = arena.format(peek, flavor)?;  // Returns just a string

// In render.rs: rendering happens here - type info is lost
opts.backend.write_styled(w, val, SemanticColor::Deleted)?;  // Can't use DeletedString!
```

## Implementation Plan

### Phase 1: Extend FormattedValue to Track Type

**File:** `src/layout/node.rs`

Currently `FormattedValue` only tracks the span and width:

```rust
pub struct FormattedValue {
    pub span: Span,
    pub width: usize,
}
```

Change to:

```rust
/// Type of a formatted value for color selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    String,
    Number,
    Boolean,
    Null,
    /// Other/unknown types (use accent color)
    Other,
}

pub struct FormattedValue {
    pub span: Span,
    pub width: usize,
    pub value_type: ValueType,
}
```

### Phase 2: Update Arena to Track Types During Formatting

**File:** `src/layout/arena.rs`

The `format` method needs to determine and store the value type. Update the signature:

```rust
pub fn format<'r>(
    &mut self,
    peek: Peek<'_, 'r>,
    flavor: &impl DiffFlavor,
) -> Result<FormattedValue, std::fmt::Error> {
    let start = self.buf.len();
    flavor.format_value(peek, &mut self.buf)?;
    let end = self.buf.len();
    let width = unicode_width::UnicodeWidthStr::width(&self.buf[start..end]);

    // Determine value type from Peek
    let value_type = determine_value_type(peek);

    Ok(FormattedValue {
        span: Span { start, end },
        width,
        value_type,
    })
}

/// Determine the type of a value for coloring purposes
fn determine_value_type(peek: Peek<'_, '_>) -> ValueType {
    use facet_core::{PrimitiveType, Type};

    match peek.shape().ty {
        Type::Primitive(p) => match p {
            PrimitiveType::Bool => ValueType::Boolean,
            PrimitiveType::I8 | PrimitiveType::I16 | PrimitiveType::I32
            | PrimitiveType::I64 | PrimitiveType::I128 | PrimitiveType::Isize
            | PrimitiveType::U8 | PrimitiveType::U16 | PrimitiveType::U32
            | PrimitiveType::U64 | PrimitiveType::U128 | PrimitiveType::Usize
            | PrimitiveType::F32 | PrimitiveType::F64 => ValueType::Number,
            PrimitiveType::Char | PrimitiveType::Str => ValueType::String,
            PrimitiveType::Unit => ValueType::Null,
        },
        Type::Option => {
            // Check if it's None
            if let Ok(opt) = peek.into_option() {
                if opt.is_none() {
                    return ValueType::Null;
                }
                // If Some, recurse to get inner type
                if let Ok(Some(inner)) = opt.into_inner() {
                    return determine_value_type(inner);
                }
            }
            ValueType::Other
        }
        _ => ValueType::Other,
    }
}
```

**Testing:** Add unit tests in `arena.rs` to verify type detection works correctly:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_value_type_detection() {
        // Test string
        let value = "hello";
        let peek = facet::to_ref(&value);
        assert_eq!(determine_value_type(peek), ValueType::String);

        // Test number
        let value = 42i32;
        let peek = facet::to_ref(&value);
        assert_eq!(determine_value_type(peek), ValueType::Number);

        // Test boolean
        let value = true;
        let peek = facet::to_ref(&value);
        assert_eq!(determine_value_type(peek), ValueType::Boolean);

        // Test None
        let value: Option<i32> = None;
        let peek = facet::to_ref(&value);
        assert_eq!(determine_value_type(peek), ValueType::Null);
    }
}
```

### Phase 3: Add Helper Function to Get Semantic Color from Type + Context

**File:** `src/layout/render.rs`

Add a helper function alongside the existing `syntax_color` helper:

```rust
/// Get the appropriate semantic color for a value based on its type and context.
fn value_color(value_type: ValueType, context: ElementChange) -> SemanticColor {
    match (value_type, context) {
        (ValueType::String, ElementChange::Deleted) => SemanticColor::DeletedString,
        (ValueType::String, ElementChange::Inserted) => SemanticColor::InsertedString,
        (ValueType::String, _) => SemanticColor::String,

        (ValueType::Number, ElementChange::Deleted) => SemanticColor::DeletedNumber,
        (ValueType::Number, ElementChange::Inserted) => SemanticColor::InsertedNumber,
        (ValueType::Number, _) => SemanticColor::Number,

        (ValueType::Boolean, ElementChange::Deleted) => SemanticColor::DeletedBoolean,
        (ValueType::Boolean, ElementChange::Inserted) => SemanticColor::InsertedBoolean,
        (ValueType::Boolean, _) => SemanticColor::Boolean,

        (ValueType::Null, ElementChange::Deleted) => SemanticColor::DeletedNull,
        (ValueType::Null, ElementChange::Inserted) => SemanticColor::InsertedNull,
        (ValueType::Null, _) => SemanticColor::Null,

        // Other/unknown types use accent colors
        (ValueType::Other, ElementChange::Deleted) => SemanticColor::Deleted,
        (ValueType::Other, ElementChange::Inserted) => SemanticColor::Inserted,
        (ValueType::Other, ElementChange::MovedFrom) | (ValueType::Other, ElementChange::MovedTo) => SemanticColor::Moved,
        (ValueType::Other, ElementChange::None) => SemanticColor::Unchanged,
    }
}

/// Get semantic color for highlight background (changed values)
fn value_color_highlight(value_type: ValueType, context: ElementChange) -> SemanticColor {
    match (value_type, context) {
        (ValueType::String, ElementChange::Deleted) => SemanticColor::DeletedString,
        (ValueType::String, ElementChange::Inserted) => SemanticColor::InsertedString,

        (ValueType::Number, ElementChange::Deleted) => SemanticColor::DeletedNumber,
        (ValueType::Number, ElementChange::Inserted) => SemanticColor::InsertedNumber,

        (ValueType::Boolean, ElementChange::Deleted) => SemanticColor::DeletedBoolean,
        (ValueType::Boolean, ElementChange::Inserted) => SemanticColor::InsertedBoolean,

        (ValueType::Null, ElementChange::Deleted) => SemanticColor::DeletedNull,
        (ValueType::Null, ElementChange::Inserted) => SemanticColor::InsertedNull,

        // Highlight uses generic highlights for Other/unchanged
        (ValueType::Other, ElementChange::Deleted) | (_, ElementChange::Deleted) => SemanticColor::DeletedHighlight,
        (ValueType::Other, ElementChange::Inserted) | (_, ElementChange::Inserted) => SemanticColor::InsertedHighlight,
        (ValueType::Other, ElementChange::MovedFrom) | (ValueType::Other, ElementChange::MovedTo) => SemanticColor::MovedHighlight,
        _ => SemanticColor::Unchanged,
    }
}
```

### Phase 4: Update Rendering Call Sites

**File:** `src/layout/render.rs`

Update all places where values are rendered to use type-aware colors. Search for:
- `write_styled(w, val, SemanticColor::Deleted)`
- `write_styled(w, val, SemanticColor::Inserted)`
- `write_styled(w, val, SemanticColor::DeletedHighlight)`
- `write_styled(w, val, SemanticColor::InsertedHighlight)`

#### 4.1: Update `render_inline_element`

**Line ~580-640:** In the inline element rendering:

```rust
// Before (line ~588):
opts.backend.write_styled(w, val, SemanticColor::Deleted)?;

// After:
let color = value_color(value.value_type, ElementChange::Deleted);
opts.backend.write_styled(w, val, color)?;

// Before (line ~607 - changed values with highlight):
opts.backend.write_styled(w, val, SemanticColor::DeletedHighlight)?;

// After:
let color = value_color_highlight(old.value_type, ElementChange::Deleted);
opts.backend.write_styled(w, val, color)?;
```

Do the same for:
- Inserted context (~line 695, 714)
- All attribute status cases (Unchanged, Changed, Deleted, Inserted)

#### 4.2: Update `render_changed_group`

**Line ~914-956:** Multi-line changed attribute groups:

```rust
// Before (line ~915):
opts.backend.write_styled(w, old_str, SemanticColor::DeletedHighlight)?;

// After:
let color = value_color_highlight(old.value_type, ElementChange::Deleted);
opts.backend.write_styled(w, old_str, color)?;

// Before (line ~955 - inserted side):
opts.backend.write_styled(w, new_str, SemanticColor::InsertedHighlight)?;

// After:
let color = value_color_highlight(new.value_type, ElementChange::Inserted);
opts.backend.write_styled(w, new_str, color)?;
```

#### 4.3: Update `render_attr_deleted` and `render_attr_inserted`

**Line ~1000-1020:** These functions render entire deleted/inserted attributes.

The challenge here is that `flavor.format_field(name, value_str)` returns the entire field as a single string (e.g., `"debug: true"`), so we can't easily color the value separately.

**Options:**

1. **Short-term:** Keep using `DeletedHighlight`/`InsertedHighlight` for entire fields
2. **Long-term:** Refactor `DiffFlavor` to return structured field formatting:

```rust
pub struct FormattedField {
    pub prefix: String,  // "debug: " or "\"debug\": "
    pub value: String,   // "true"
    pub suffix: String,  // "" or ","
}

pub trait DiffFlavor {
    fn format_field_parts(&self, name: &str, value: &str) -> FormattedField;
}
```

Then render each part with appropriate colors:

```rust
fn render_attr_deleted(..., value: &FormattedValue) -> fmt::Result {
    let value_str = layout.get_string(value.span);
    let field = flavor.format_field_parts(name, value_str);

    // Key uses deleted key color, value uses type-aware deleted color
    opts.backend.write_styled(w, &field.prefix, SemanticColor::DeletedKey)?;
    let value_color = value_color_highlight(value.value_type, ElementChange::Deleted);
    opts.backend.write_styled(w, &field.value, value_color)?;
    opts.backend.write_styled(w, &field.suffix, SemanticColor::DeletedStructure)?;

    Ok(())
}
```

**Recommendation:** Start with option 1 (keep current behavior for entire deleted/inserted fields), then do option 2 as a follow-up enhancement.

#### 4.4: Update Sequence Item Rendering

**Line ~288:** Sequence items (array elements):

```rust
// Before:
opts.backend.write_styled(w, &formatted, semantic)?;

// After:
let semantic = if let Some(item) = items.get(i) {
    value_color(item.value_type, change)
} else {
    element_change_to_semantic(change)
};
opts.backend.write_styled(w, &formatted, semantic)?;
```

Note: You'll need to ensure `FormattedValue` is available at this point. Check if sequence items store their `FormattedValue`.

### Phase 5: Update Tests and Snapshots

1. **Run tests:**
   ```bash
   cargo nextest run -p facet-diff-core -p facet-diff
   ```

2. **Update snapshots:**
   ```bash
   cargo insta test --accept -p facet-diff
   ```

3. **Visual inspection:**
   - Check that strings are now green-tinted (blended with yellow/blue)
   - Check that numbers are orange-tinted (blended with yellow/blue)
   - Check that booleans and null values use their distinct colors
   - Verify the blending creates cohesive warm (deleted) and cool (inserted) tones

### Phase 6: Documentation and Examples

1. **Update `theming-design.md`** to reflect that type-aware coloring is now implemented

2. **Add examples to documentation** showing the color palette:
   ```markdown
   ## Value Type Colors

   | Type    | Base Color | Deleted (Yellow blend) | Inserted (Blue blend) |
   |---------|------------|------------------------|----------------------|
   | String  | Green      | Yellow-green          | Teal                 |
   | Number  | Orange     | Yellow-orange         | Bronze               |
   | Boolean | Orange     | Yellow-orange         | Bronze               |
   | Null    | Cyan       | Cyan-yellow           | Cyan-blue            |
   ```

3. **Add theme customization guide** explaining how users can adjust:
   - Base colors for each value type
   - Blend factors (currently 40% for values)
   - Background brightness

## Testing Strategy

### Unit Tests

Add tests in `arena.rs` for type detection:
- Test all primitive types
- Test Option<T> (both Some and None)
- Test nested structures

### Integration Tests

Add tests in `facet-diff/tests/diff_snapshots.rs`:
- Test string values show green-ish tones
- Test number values show orange-ish tones
- Test boolean values are distinct
- Test null/None values use cyan tones
- Test type colors blend correctly with accent colors

### Visual Testing

Create a visual test file that shows all combinations:

```rust
#[test]
fn visual_test_all_types() {
    #[derive(Facet)]
    struct AllTypes {
        string: &'static str,
        number: i32,
        float: f64,
        boolean: bool,
        null: Option<i32>,
    }

    let before = AllTypes {
        string: "hello",
        number: 42,
        float: 3.14,
        boolean: true,
        null: None,
    };

    let after = AllTypes {
        string: "world",
        number: 99,
        float: 2.71,
        boolean: false,
        null: Some(1),
    };

    let diff = facet_diff::diff(&before, &after);
    println!("{}", diff);
}
```

## Migration Path

This is a **non-breaking change** because:
- New field `value_type` can be added to `FormattedValue` with a default
- Old code will work with `ValueType::Other` as fallback
- Colors gracefully degrade to accent colors for unknown types

## Performance Considerations

- Type determination happens during formatting (once per value)
- Minimal overhead: just pattern matching on `Type` enum
- No additional allocations
- Blending happens at render time using pre-computed blend methods

## Future Enhancements

1. **Custom type colors per theme**
   - Allow themes to override value type colors
   - Support custom blend factors per theme

2. **User-defined type mappings**
   - Let users specify custom types and their colors
   - E.g., "Duration values should be magenta"

3. **Semantic coloring for special values**
   - Empty strings could be dimmed
   - Zero could be distinct from other numbers
   - Default/sentinel values could be muted

4. **Terminal capability detection**
   - Fall back to simpler colors on limited terminals
   - Support both 16-color and 256-color modes

## Implementation Checklist

- [ ] Add `ValueType` enum to `node.rs`
- [ ] Add `value_type` field to `FormattedValue`
- [ ] Implement `determine_value_type` in `arena.rs`
- [ ] Add unit tests for type detection
- [ ] Add `value_color` and `value_color_highlight` helpers to `render.rs`
- [ ] Update `render_inline_element` (both deleted and inserted sides)
- [ ] Update `render_changed_group` (both deleted and inserted sides)
- [ ] Consider refactoring `render_attr_deleted`/`render_attr_inserted`
- [ ] Update sequence item rendering
- [ ] Run tests and update snapshots
- [ ] Visual inspection of color output
- [ ] Update documentation
- [ ] Add integration tests for type-aware coloring

## Estimated Effort

- **Phase 1-2 (Type tracking):** 1-2 hours
- **Phase 3 (Helper functions):** 30 minutes
- **Phase 4 (Update call sites):** 2-3 hours
- **Phase 5 (Testing):** 1-2 hours
- **Phase 6 (Documentation):** 1 hour

**Total: 6-9 hours** of focused development time
