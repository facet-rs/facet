# Facet Reflect API Migration Guide

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
- `begin_insert()` - Begin inserting a key-value pair into map
- `begin_key()` - Set the key of a map entry
- `begin_value()` - Set the value of a map entry

#### Smart Pointers
- `begin_smart_ptr()` - Navigate into smart pointer contents (Box, Arc, etc.)

### 4. Single End Method
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
partial = partial.begin_field("field_name")?;
partial = partial.set(value)?;
partial = partial.end()?;
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
partial = partial.begin_pushback()?;
partial = partial.begin_list_item()?;
partial = partial.set(42)?;
partial = partial.end()?;
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
partial = partial.begin_map()?;
partial = partial.begin_insert()?;
partial = partial.begin_key()?;
partial = partial.set("key".to_string())?;
partial = partial.end()?;
partial = partial.begin_value()?;
partial = partial.set(42)?;
partial = partial.end()?;
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
partial = partial.begin_nth_element(0)?;
partial = partial.set(value)?;
partial = partial.end()?;
```

## Key Differences

1. **Single end method**: Use `end()` for all types instead of type-specific pop methods
2. **Descriptive naming**: Method names clearly indicate their purpose (e.g., `begin_field` vs generic `push`)
3. **Separate map operations**: Map construction uses distinct `begin_key()` and `begin_value()` methods
4. **List initialization**: Lists require `begin_pushback()` before adding items with `begin_list_item()`
5. **Convenience methods**: Use `set_field(name, value)` as a shorthand for `begin_field(name)?.set(value)?.end()?`
