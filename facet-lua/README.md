# facet-lua

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

Generate LuaLS type annotations, serialize values to Lua table syntax,
and parse Lua table syntax back into Rust values.

This crate provides three complementary features:
- **Annotations**: Generate LuaLS `---@class` / `---@alias` annotations from type metadata
- **Serialization**: Serialize Rust values to Lua table constructor syntax
- **Deserialization**: Parse Lua table constructor syntax back into Rust values

## Example

```rust
use facet::Facet;
use facet_lua::{to_lua_annotations, to_string_pretty, to_lua_annotated, from_str};

#[derive(Facet, Debug, PartialEq)]
struct User {
    name: String,
    age: u32,
}

// Generate just the annotations
let annotations = to_lua_annotations::<User>();
assert!(annotations.contains("---@class User"));

// Serialize a value
let user = User { name: "Alice".into(), age: 30 };
let lua = to_string_pretty(&user).unwrap();
assert!(lua.contains("name"));

// Parse it back
let parsed: User = from_str(&lua).unwrap();
assert_eq!(parsed.name, "Alice");
assert_eq!(parsed.age, 30);

// Combined: annotations + typed local variable
let annotated = to_lua_annotated(&user, "user").unwrap();
assert!(annotated.contains("---@class User"));
assert!(annotated.contains("---@type User"));
assert!(annotated.contains("local user ="));
```

## Lua syntax coverage

The parser accepts a broad subset of Lua table syntax:
- Table constructors with `,` or `;` separators
- Bare identifier keys, string bracket keys (`["key"]`, `[ [[key]] ]`),
  and integer bracket keys (`[1]`, `[-2]`, `[2.0]`)
- Explicit-index arrays (`{[1]="a", [2]="b"}`) deserialize into sequences
  when the target expects one; indices must be contiguous from 1 and in
  order
- Double-quoted, single-quoted, and long-bracket strings (`[[...]]`, `[=[...]=]`)
- All standard string escapes: `\n`, `\t`, `\r`, `\a`, `\b`, `\f`, `\v`, `\\`, `\"`, `\'`,
  and backslash-newline
- Extended escapes: `\xNN` (hex), `\u{XXXX}` (Unicode), `\z` (whitespace skip), `\ddd` (decimal)
- Decimal and hex integer literals (`0xFF`, wrapping modulo 2^64 like Lua 5.4)
- Decimal floats (`1.5e3`, `.5`, `3.`) and hex floats (`0x1.8p1`)
- Special floats: `math.huge`, `-math.huge`, `0/0`, `-0/0`
- Line comments (`--`) and block comments (`--[[ ]]`, `--[=[ ]=]`)
- Function values are rejected

Lua 5.4 lexical rules are enforced and applied: unescaped line breaks in
quoted strings are rejected, and line breaks inside long-bracket strings
are normalized to `\n`.

## Integer range

Lua 5.4 integers are signed 64-bit. Integers above `i64::MAX` serialize
as decimal strings by default so no consumer silently reads a rounded
float; deserialization parses them back. See [`BigIntEncoding`](https://docs.rs/facet-lua/latest/facet_lua/serializer/enum.BigIntEncoding.html) to opt
into bare numerals instead.

<!-- cargo-reedme: end -->
