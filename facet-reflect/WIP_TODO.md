# facet-reflect Wip Reimplementation TODO

This document tracks the remaining work for the facet-reflect Wip (Work-in-progress) pattern reimplementation.

## ✅ Completed
- [x] Basic struct initialization with `push_nth_field()`
- [x] Array initialization with `push_nth_element()` (up to 63 elements)
- [x] Enum support with `push_variant()` and `push_nth_enum_field()`
- [x] Box<T> support with `push_box()`
- [x] Arc<T> support with `push_arc()`
- [x] Field re-initialization with proper drop handling
- [x] `set_default()` and `set_from_function()` methods
- [x] ISet for tracking partial initialization
- [x] Memory safety verified with Miri

## 🚧 Major Missing Features

### Container Types
- [x] **Map/HashMap Support**
  - [x] `begin_map()` method to initialize maps
  - [x] `begin_insert()`, `push_key()` and `push_value()` methods
  - [x] Memory safety verified with Miri
  - [ ] Support for BTreeMap and other map types

- [x] **List/Vec Support**
  - [x] `begin_pushback()` method
  - [x] `push()` for dynamic list building
  - [x] Memory leak tests for partial list initialization

- [ ] **Set Support**
  - [ ] HashSet initialization
  - [ ] BTreeSet initialization
  - [ ] Set-specific methods

### Type Support
- [ ] **Union Support** (currently has `todo!()`)
- [ ] **Tuple Support**
  - [ ] Direct tuple initialization
  - [ ] Tuple struct patterns
- [ ] **Additional Smart Pointers**
  - [ ] Rc<T> support
  - [ ] Cow<T> support
  - [ ] Other smart pointer types
- [ ] **Opaque Type Handling** (currently has `todo!()`)

## 🔧 API Improvements

### Convenience Methods
- [ ] `variant_named()` - select enum variant by name instead of discriminant
- [ ] `field_named()` - select struct field by name
- [ ] Fluent/chaining API that returns new Wip instances

### Field Access
- [ ] ~~Re-access already initialized fields~~ **NOT PLANNED**
- [ ] ~~Partial updates to initialized structures~~ **NOT PLANNED**
- [x] Array element re-initialization support

## 🧪 Testing & Safety

### Compile-time Safety
- [ ] Variance tests (covariant, contravariant, invariant)
- [ ] Lifetime safety verification
- [ ] Compile test framework integration

### Runtime Testing
- [ ] Comprehensive memory leak tests
- [ ] Drop behavior in error cases
- [ ] Partial initialization edge cases

## 🔗 Integration

### Peek Integration
- [ ] Bidirectional conversion between Peek and Wip
- [ ] Unified API for reading and writing

### Type System
- [ ] Facts collection for trait discovery
- [ ] Automatic type conversion patterns
- [ ] `try_from` integration

## 📝 Notes

- Current implementation focuses on core functionality (structs, arrays, enums, Box/Arc)
- ISet limits tracking to 63 fields/elements
- All existing functionality passes Miri memory safety checks
- Priority should be on completing container types (Map, List, Set) as they're commonly used