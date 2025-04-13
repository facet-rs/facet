<h1>
<picture>
<source srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/logo-only.webp">
<img src="https://github.com/facet-rs/facet/raw/main/static/logo-v2/logo-only.png" height="35" alt="Facet logo - a reflection library for Rust">
</picture> &nbsp; facet-reflect
</h1>

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![free of syn](https://img.shields.io/badge/free%20of-syn-hotpink)](https://github.com/fasterthanlime/free-of-syn)
[![crates.io](https://img.shields.io/crates/v/facet-reflect.svg)](https://crates.io/crates/facet-reflect)
[![documentation](https://docs.rs/facet-reflect/badge.svg)](https://docs.rs/facet-reflect)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-reflect.svg)](./LICENSE)

_Logo by [Misiasart](https://misiasart.com/)_

Thanks to all individual and corporate sponsors, without whom this work could not exist:

<p> <a href="https://ko-fi.com/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/ko-fi-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/ko-fi-light.svg" height="40" alt="Ko-fi">
</picture>
</a> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/patreon-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/patreon-light.svg" height="40" alt="Patreon">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
    <img src="https://depot.dev/badges/built-with-depot.svg" alt="built with depot">
</a> </p>


Allows reading (peek) and constructing/initializing/mutating (poke) arbitrary
values without knowing their concrete type until runtime.

## Overview

facet-reflect provides two main APIs:

- **Peek**: Read and inspect arbitrary values at runtime
- **Poke**: Create, initialize, and mutate arbitrary values at runtime

## Struct Initialization API

The Poke API provides a powerful way to create and initialize structs through reflection, with a carefully designed system for tracking initialization state.

### Basic Usage

```rust
use facet::Facet;
use facet_reflect::PokeValueUninit;
use std::string::String;

// Define a struct to work with
#[derive(Facet, Debug, PartialEq)]
struct Person {
    age: u64,
    name: String,
}

// Allocate memory for a struct
let (poke, guard) = PokeValueUninit::alloc::<Person>();

// Convert to a struct handler
let poke = poke.into_struct().unwrap();

// Set field values by name
let poke = poke.field_by_name("age").unwrap().set(42u64).unwrap().into_struct_uninit();
let poke = poke
    .field_by_name("name")
    .unwrap()
    .set(String::from("Joan Watson"))
    .unwrap()
    .into_struct_uninit();

// Build the final struct
let person: Person = poke.build(Some(guard)).unwrap();
assert_eq!(person.age, 42);
assert_eq!(person.name, "Joan Watson");
```

### How It Works

1. **Allocation and Creation**:
   - Use `PokeValueUninit::alloc<T>()` to allocate memory for type T
   - Returns a `(PokeValueUninit, Guard)` pair where Guard handles deallocation

2. **Converting to a Struct Context**:
   - Call `into_struct()` to get a `PokeStructUninit` for working with struct fields
   - The struct handler tracks which fields have been initialized

3. **Working with Fields via Slots**:
   - Access fields with `field_by_name(name)` or `field(index)` to get a `Slot`
   - A `Slot` represents a field and maintains a connection to its parent
   - Set a field value with `slot.set(value)`, which returns the parent context

4. **Nested Struct Initialization**:
   - For fields that are themselves structs, call `slot.into_struct()`
   - Initialize nested fields, then call `finish()` to return to the parent

5. **Building the Final Struct**:
   - Call `build<T>(guard)` to finalize when all fields are initialized
   - The system validates that all fields are set and any invariants are satisfied

### Type Safety

The API maintains type safety by:
- Validating that field values match the expected type
- Ensuring all fields are initialized before building the final struct
- Checking any struct invariants during the build process

### Navigation Methods

The API provides several methods for navigating the initialization hierarchy:
- `into_struct_uninit()` - Navigate from a parent context to the struct context
- `into_struct_slot()` - Navigate from a parent context to a struct slot
- `finish()` - Complete a nested struct and return to its parent



