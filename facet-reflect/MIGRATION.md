# Facet Reflect API Migration Guide

## Quick Reference: Most Common Changes

| Old Method | New Method | Notes |
|------------|------------|-------|
| `Wip::new::<T>()` | `Partial::alloc::<T>()` | Returns `TypedPartial<T>` |
| `wip.put(value)` | `partial.set(value)` | |
| `wip.put_default()` | `partial.set_default()` | |
| `wip.field(idx)` | `partial.begin_nth_field(idx)` | |
| `wip.variant(disc)` | `partial.select_variant(disc)` | |
| `wip.variant_named(name)` | `partial.select_variant_named(name)` | |
| `wip.begin_map_insert()` | Just use `partial.begin_map()` | No separate insert method |
| `wip.put_empty_list()` | Just use `partial.begin_list()` | Don't add items for empty |
| `wip.put_empty_map()` | Just use `partial.begin_map()` | Don't add items for empty |

## Major API Changes

### 1. Wip → Partial
The `Wip` type has been renamed to `Partial` throughout the codebase.

### 2. Constructor Methods
- `Wip::new::<T>()` → `Partial::alloc::<T>()` (for simple cases)
- `Wip::alloc::<T>()` → `Partial::alloc_shape(T::SHAPE)` (preferred for explicit shape usage)

### 3. Navigation Methods
The API uses descriptive method names for different operations:

#### Struct Fields
- `begin_field(field_name)` - Select a struct field by name
- `begin_nth_field(idx)` - Select a struct field by index
- `field(idx)` - No longer exists, use `begin_nth_field(idx)` instead

#### Arrays and Tuples
- `begin_nth_element(idx)` - Select an array or tuple element by index

#### Enums
- `select_variant(discriminant)` - Select enum variant by discriminant
- `select_variant_named(variant_name)` - Select enum variant by name
- `begin_nth_enum_field(idx)` - Select enum variant field by index

#### Collections
- `begin_list()` - Initialize a list/vector for adding elements
- `begin_list_item()` - Add an item to a list
- `begin_map()` - Initialize a map for adding key-value pairs
- `begin_key()` / `push_map_key()` - Push a frame for setting the key of a map entry (push_map_key is an alias)
- `begin_value()` / `push_map_value()` - Push a frame for setting the value of a map entry (push_map_value is an alias)

Note: Methods like `put_empty_list()` and `put_empty_map()` no longer exist. To create empty collections, just call `begin_list()`/`begin_map()` without adding any items.

#### Smart Pointers
- `begin_smart_ptr()` - Navigate into smart pointer contents (Box, Arc, etc.)

### 4. Setting Values
- `put()` → `set()` - Set a value at the current position
- `put_default()` → `set_default()` - Set the default value at the current position
- `put_from_fn()` → `set_from_function()` - Set a value using a function
- `parse()` - Method removed, use type conversions before calling `set()`

### 5. Other Method Changes
- `variant(discriminant)` → `select_variant(discriminant)` 
- `variant_named(name)` → `select_variant_named(name)`
- `begin_map_insert()` - Removed, just use `begin_map()` followed by `begin_key()`
- `frames_count()` - Removed, internal implementation detail

### 6. Single End Method
All navigation operations use a single `end()` method to pop the current frame, regardless of the type being constructed.

## Migration Examples

### Basic Struct Construction

#### Before
```rust
let mut wip = Wip::new::<MyStruct>();
wip = wip.push_struct_field("field_name")?;
wip = wip.put(value)?;
wip = wip.pop_struct_field()?;
```

#### After
```rust
let mut partial = Partial::alloc::<MyStruct>()?;
partial.begin_field("field_name")?;
partial.set(value)?;
partial.end()?;
```

### List Construction

#### Before
```rust
let mut wip = Wip::new::<Vec<i32>>();
wip = wip.push_list_element()?;
wip = wip.put(42)?;
wip = wip.pop_list_element()?;
```

#### After
```rust
let mut partial = Partial::alloc::<Vec<i32>>()?;
partial.begin_list()?;
partial.begin_list_item()?;
partial.set(42)?;
partial.end()?;
```

### Map Construction

#### Before
```rust
let mut wip = Wip::new::<HashMap<String, i32>>();
wip = wip.push_map_entry()?;
// ... set key and value
wip = wip.pop_map_entry()?;
```

#### After
```rust
let mut partial = Partial::alloc::<HashMap<String, i32>>()?;
partial.begin_map()?;
partial.begin_key()?;   // or push_map_key()
partial.set("key".to_string())?;
partial.end()?;
partial.begin_value()?; // or push_map_value()
partial.set(42)?;
partial.end()?;
```

### Array Construction

#### Before
```rust
let mut wip = Wip::new::<[i32; 3]>();
wip = wip.push_array_element(0)?;
wip = wip.put(value)?;
wip = wip.pop_array_element()?;
```

#### After
```rust
let mut partial = Partial::alloc::<[i32; 3]>()?;
partial.begin_nth_element(0)?;
partial.set(value)?;
partial.end()?;
```

### Option Construction

#### Before
```rust
// The old API supported implicit conversion from inner value to Some
let mut wip = Wip::new::<Option<String>>();
wip = wip.put("hello")?;  // Implicitly creates Some("hello")
```

#### After
```rust
// The new API requires explicit Option values
let mut partial = Partial::alloc::<Option<String>>()?;
partial.set(Some("hello".to_string()))?;  // Explicit Some

// Or for None:
partial.set(None)?;
```

**Note**: The implicit conversion from inner value to `Some` has been removed for clarity and consistency. You must now explicitly provide `Some(value)` or `None`.

## Key Differences

1. **Single end method**: Use `end()` for all types instead of type-specific pop methods
2. **Descriptive naming**: Method names clearly indicate their purpose (e.g., `begin_field` vs generic `push`)
3. **Separate map operations**: Map construction uses distinct `begin_key()` and `begin_value()` methods (no `begin_insert()`)
4. **List initialization**: Lists require `begin_list()` before adding items with `begin_list_item()`
5. **Convenience methods**: Use `set_field(name, value)` as a shorthand for `begin_field(name)?.set(value)?.end()?`
6. **No implicit Option conversion**: Must explicitly use `Some(value)` or `None` when setting Option types
7. **Mutable reference API**: All navigation methods return `&mut self`, so you don't need to reassign the variable

## Partial vs TypedPartial

The new API has two related types for building values:

1. **`Partial`** - Type-erased builder that works with shapes
   - Created with `Partial::alloc_shape(shape)` when you have a shape but not a concrete type
   - `build()` returns a `HeapValue` which must be materialized to get the concrete type
   - Use pattern: `partial.build()?.materialize::<T>()?`
   - Used internally by deserializers that work with shapes

2. **`TypedPartial<T>`** - Typed wrapper that knows the concrete type at compile time
   - Created with `Partial::alloc::<T>()` when you know the type
   - `build()` returns `Box<T>` directly (no materialize needed)
   - More convenient when type is known at compile time
   - Can be converted to `Partial` via `as_partial()` or `as_partial_mut()` methods

### Example

```rust
// Using Partial (type-erased)
let partial = Partial::alloc_shape(MyStruct::SHAPE)?;
// ... build the value ...
let heap_value = partial.build()?;
let value: MyStruct = heap_value.materialize()?;

// Using TypedPartial (typed)
let partial = Partial::alloc::<MyStruct>()?;
// ... build the value ...
let value: Box<MyStruct> = partial.build()?;

// Converting TypedPartial to Partial (for passing to functions)
let mut typed_partial = Partial::alloc::<MyStruct>()?;
let partial_ref: &mut Partial = typed_partial.as_partial_mut();
```
