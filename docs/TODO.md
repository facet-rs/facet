# Documentation Improvement Plan

This document tracks planned improvements to the facet documentation website at `docs/`.

## Table of Contents

0. [Audience-Based Documentation Structure](#0-audience-based-documentation-structure) ← **START HERE**
1. [Clarity Issues](#1-clarity-issues)
2. [Correctness Issues](#2-correctness-issues)
3. [Structure Issues](#3-structure-issues)
4. [Tone Issues](#4-tone-issues)
5. [Navigation Issues](#5-navigation-issues)

---

## 0. Audience-Based Documentation Structure

The most impactful improvement is reorganizing documentation by **audience** rather than by topic. Facet serves three distinct audiences with very different needs:

### The Three Audiences

#### 1. **Learn** — "I want to serialize my types"

**Who they are:**
- Application developers using `facet-json`, `facet-yaml`, `facet-kdl`, etc.
- Coming from serde, want to know how to migrate
- Don't care about internals, just want things to work

**What they need:**
- Installation instructions
- Derive macro usage (`#[derive(Facet)]`)
- Attribute reference (`#[facet(rename = "...")]`, `#[facet(default)]`, etc.)
- Format-specific guides (JSON gotchas, KDL node structure, YAML anchors)
- Error message explanations
- Showcases with copy-paste examples

**What they DON'T need:**
- `Shape`, `Def`, `Peek`, `Partial` details
- VTable internals
- How to build a format crate

#### 2. **Developers** — "I want to build on facet"

**Who they are:**
- Building a new format crate (`facet-xml`, `facet-protobuf`, `facet-avro`)
- Building tools using reflection (`facet-pretty`, `facet-diff`, schema generators)
- Creating custom `Facet` implementations for special types

**What they need:**
- Understanding `Shape` and what information it provides
- How to use `Peek` for reading values
- How to use `Partial` for building values
- Extension attributes system (defining `xml::` namespace)
- The `Facet` trait contract
- How existing format crates are structured (as reference)
- Testing strategies for format crates

**What they DON'T need:**
- Proc macro internals
- How vtables are constructed
- Memory layout details

#### 3. **Contribute** — "I want to contribute to facet-core"

**Who they are:**
- Contributors to `facet-core`, `facet-reflect`, `facet-derive`
- People debugging weird edge cases
- Researchers exploring the design space

**What they need:**
- Architecture overview (crate graph, responsibilities)
- Proc macro implementation (`facet-derive`)
- VTable construction and safety invariants
- `Characteristic` and `ValueVTable` design
- Memory layout and `unsafe` patterns
- Design documents and rationale
- How to add new built-in types
- Test infrastructure

### Proposed Site Structure

```
/                                   # Landing page (routes to appropriate guide)
│
├── /learn/                          # LEARN GUIDE
│   ├── why/                        # **WHY FACET?** (read this first!)
│   ├── getting-started/            # Installation, first example
│   ├── attributes/                 # Complete attribute reference
│   │   ├── container/              # #[facet(deny_unknown_fields)], etc.
│   │   ├── field/                  # #[facet(default)], #[facet(skip)], etc.
│   │   └── variant/                # Enum variant attributes
│   ├── formats/                    # Per-format guides
│   │   ├── json/                   # facet-json specifics
│   │   ├── yaml/                   # facet-yaml specifics
│   │   ├── kdl/                    # facet-kdl + kdl:: attributes
│   │   ├── toml/                   # facet-toml specifics
│   │   └── ...
│   ├── migration/                  # Coming from serde
│   ├── errors/                     # Understanding error messages
│   ├── showcases/                  # Copy-paste examples (existing)
│   └── faq/                        # Common questions
│
├── /extend/                           # EXTEND GUIDE
│   ├── overview/                   # What facet provides for tool builders
│   ├── shape/                      # Understanding Shape, Def, fields, variants
│   ├── peek/                       # Reading values with Peek
│   ├── partial/                    # Building values with Partial
│   ├── extension-attrs/            # Creating custom attribute namespaces
│   ├── format-crate/               # How to build a format crate
│   │   ├── architecture/           # Deserializer/Serializer patterns
│   │   ├── error-handling/         # Spans, diagnostics, miette
│   │   └── testing/                # Test patterns, snapshot testing
│   └── examples/                   # Annotated existing crate walkthroughs
│
├── /contribute/                        # CONTRIBUTE GUIDE
│   ├── architecture/               # Crate graph, design philosophy
│   ├── derive-macro/               # facet-derive internals
│   ├── vtables/                    # ValueVTable, Characteristic
│   ├── memory/                     # Layout, unsafe patterns, invariants
│   ├── design/                     # Design documents (existing docs/design/)
│   ├── adding-types/               # How to add support for new std types
│   └── contributing/               # Dev setup, testing, PR process
│
└── /reference/                     # REFERENCE (audience-neutral)
    ├── format-matrix/              # Feature support table (existing)
    └── changelog/                  # Version history
```

### Navigation Design

The top navigation should make audience selection obvious:

```
┌─────────────────────────────────────────────────────────────────┐
│  🔷 facet       [Learn ▾] [Extend ▾] [Contribute ▾]  🔍  │
└─────────────────────────────────────────────────────────────────┘
```

Or with tabs on the landing page:

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│                     Welcome to facet                            │
│         Runtime reflection for Rust                             │
│                                                                 │
│   ┌──────────────┐ ┌──────────────┐ ┌──────────────┐           │
│   │  📖 Learn    │ │  🔧 Extend   │ │  🔬 Contribute │         │
│   │              │ │              │ │                │         │
│   │ Serialize    │ │ Build tools  │ │ Work on        │         │
│   │ your types   │ │ with facet   │ │ facet itself   │         │
│   └──────────────┘ └──────────────┘ └──────────────┘           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Content Migration Plan

| Current Location | New Location | Audience |
|------------------|--------------|----------|
| `_index.md` (crash course) | `/learn/getting-started/` | Learn |
| `_index.md` (Shape, Peek, Partial) | `/extend/overview/` | Extend |
| `_index.md` (use cases) | `/` (landing) | All |
| `serde-comparison.md` | `/learn/migration/` | Learn |
| `extension-attributes.md` | `/extend/extension-attrs/` | Extend |
| `format-crate-matrix.md` | `/reference/format-matrix/` | All |
| `showcases/*` | `/learn/showcases/` | Learn |
| `design/*` | `/contribute/design/` | Contribute |

### What Needs to be Written (New Content)

#### For Learn Guide:
- [ ] **"Why Facet?" page** (see detailed spec below - this is CRITICAL)
- [ ] Per-format guides (currently only showcases exist)
- [ ] Complete attribute reference (extracted from serde-comparison + extension-attrs)
- [ ] Error message guide
- [ ] FAQ

#### For Extend Guide:
- [ ] Shape deep-dive (currently only brief mention in _index.md)
- [ ] Peek tutorial with examples
- [ ] Partial tutorial with examples
- [ ] "Build a format crate" tutorial
- [ ] Annotated walkthrough of facet-json or facet-yaml

#### For Contribute Guide:
- [ ] Architecture overview
- [ ] Derive macro walkthrough
- [ ] VTable design and safety
- [ ] "Adding a new type" guide
- [ ] Contributing guide (expanded from one-liner)

### Implementation Phases

**Phase 1: Restructure Navigation**
- Create `/learn/`, `/extend/`, `/contribute/` directories
- Create index pages for each
- Update navigation to show three tracks
- Move existing content to appropriate locations

**Phase 2: Fill Learn Guide Gaps**
- Write getting-started (high priority)
- Write per-format guides
- Write attribute reference
- Polish showcases

**Phase 3: Fill Extend Guide Gaps**
- Write Shape/Peek/Partial tutorials
- Write "build a format crate" guide
- Add annotated examples

**Phase 4: Fill Contribute Guide Gaps**
- Write architecture overview
- Document derive macro
- Expand contributing guide

---

### The "Why Facet?" Page — Detailed Spec

This is the most important page in the Learn Guide. It must be linked prominently from the landing page and appear before "Getting Started". Users need to understand **why** before investing time in **how**.

#### The Tagline

> **A fresh take on serialization**

But it's not *just* serialization. The tagline is the hook, but the page needs to immediately expand on it:

- Yes, serialization (facet-json, facet-yaml, facet-kdl, facet-toml, facet-msgpack...)
- But also pretty-printing (facet-pretty)
- And structural diffing (facet-diff)
- And better test assertions (facet-assert)
- And runtime introspection
- And a better intermediate value type (facet-value)

**The core message:**

> **Facet's goal is not speed. It's not code size. It's not even compile time.**
>
> **Facet's goal is features. Great diagnostics. Multiple tools built from one derive.**

This must be stated clearly and early. Users expecting a "faster serde" will be disappointed. Users wanting "better errors" or "one derive for everything" will be excited.

#### Implementation: Generated Comparison Showcase

**This page should be auto-generated from actual code**, just like the format showcases. Users must be able to verify every claim by running the code themselves.

Create a new comparison example crate (e.g., `facet-comparison` or in `samples/comparison/`) that generates:

##### 1. Binary Size Comparison

**The core insight: monomorphization**

```
┌─────────────────────────────────────────────────────────────┐
│ Why Serde Binaries Grow: Monomorphization                   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  When you call serde_json::to_string(&my_struct), Rust      │
│  generates a concrete copy of that function for MyStruct.   │
│                                                             │
│  serde_json::to_string::<Person>                            │
│  serde_json::to_string::<Config>                            │
│  serde_json::to_string::<Vec<Person>>   ← another copy      │
│  serde_json::to_string::<Option<Config>> ← another copy     │
│                                                             │
│  Now add serde_yaml:                                        │
│                                                             │
│  serde_yaml::to_string::<Person>        ← duplicate logic   │
│  serde_yaml::to_string::<Config>        ← duplicate logic   │
│  serde_yaml::to_string::<Vec<Person>>   ← duplicate logic   │
│  ...                                                        │
│                                                             │
│  Result: O(types × formats) code in your binary             │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ Why Facet Binaries Don't: Reflection                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  facet_json::to_string() is ONE function. Not generic.      │
│  It reads the Shape at runtime and serializes accordingly.  │
│                                                             │
│  Your types contribute:                                     │
│  • Static Shape data (field names, offsets, types)          │
│  • That's it. No code generation per type.                  │
│                                                             │
│  Adding facet_yaml? It uses the SAME Shape data.            │
│                                                             │
│  Result: O(types) data + O(formats) code                    │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ The Tradeoff                                                │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Serde's monomorphization = faster (compiler optimizes)     │
│  Facet's reflection = smaller (no duplication)              │
│                                                             │
│  Small program: facet's base overhead dominates → serde wins│
│  Large program: serde's multiplication dominates → facet wins│
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**What we'll measure:**

Rather than invent numbers, we'll build real scenarios and measure them:

```
samples/comparison/
├── scenario-a/          # 1 struct, 1 format (worst case for facet)
├── scenario-b/          # 10 structs, 1 format
├── scenario-c/          # 50 structs, 2 formats
├── scenario-d/          # 100+ structs, 3 formats
└── measure-sizes.sh     # Builds all, outputs comparison table
```

Each scenario builds both serde and facet versions. The script:
1. Builds with `--release`, LTO, stripped
2. Measures binary sizes
3. Outputs a table with actual numbers
4. Links to source code

**The page will show real measurements**, whatever they turn out to be.
We're confident in the O() model — the measurements will confirm it.

```bash
cd samples/comparison
./measure-sizes.sh  # See for yourself
```

##### 2. Diagnostics Comparison

**The Code:**
```rust
// Same malformed JSON input for both:
let input = r#"{
    "name": "Alice",
    "agge": 30,
    "email": "alice@example.com"
}"#;

// Try to deserialize into:
struct User {
    name: String,
    age: u32,  // Note: input has "agge" typo
    email: String,
}
```

**The Output (auto-generated, side-by-side):**

```
┌─────────────────────────────────────────────────────────────┐
│ Error Output: Unknown Field "agge" (typo for "age")         │
├────────────────────────────┬────────────────────────────────┤
│ serde_json                 │ facet_json                     │
├────────────────────────────┼────────────────────────────────┤
│                            │                                │
│ Error: unknown field       │   × unknown field `agge`       │
│ `agge`, expected one of    │     ╭─[input.json:3:5]         │
│ `name`, `age`, `email`     │   2 │     "name": "Alice",     │
│                            │   3 │     "agge": 30,          │
│                            │     ·     ──┬──                │
│                            │     ·       ╰── did you mean   │
│                            │     ·           `age`?         │
│                            │   4 │     "email": "..."       │
│                            │     ╰────                      │
│                            │                                │
├────────────────────────────┴────────────────────────────────┤
│ [View source: samples/comparison/src/diagnostics.rs]        │
│ [Reproduce: cargo run --example diagnostics]                │
└─────────────────────────────────────────────────────────────┘
```

Show multiple error scenarios:
- Unknown field with typo suggestion
- Type mismatch (string vs number)
- Missing required field
- Nested object errors (where in the path?)
- Array index errors

##### 3. Flexibility Comparison

**Scenario: Same types, multiple uses**

```
┌─────────────────────────────────────────────────────────────┐
│ One Type, Many Uses                                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ Given this type:                                            │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ #[derive(Facet)]  // Just one derive                    │ │
│ │ struct Config {                                         │ │
│ │     name: String,                                       │ │
│ │     port: u16,                                          │ │
│ │     #[facet(sensitive)]                                 │ │
│ │     api_key: String,                                    │ │
│ │ }                                                       │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ You get:                                                    │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ ▸ JSON serialization         facet_json::to_string(&c)     │
│ ▸ YAML serialization         facet_yaml::to_string(&c)     │
│ ▸ KDL serialization          facet_kdl::to_string(&c)      │
│ ▸ Pretty debug output        facet_pretty::to_string(&c)   │
│   (with api_key REDACTED)                                   │
│ ▸ Structural diffing         facet_diff::diff(&a, &b)      │
│ ▸ Better assertions          facet_assert::eq!(a, b)       │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ With serde, you would need:                                 │
│                                                             │
│ #[derive(Serialize, Deserialize)]  // For JSON/YAML/etc    │
│ #[derive(Debug)]                   // For debug output      │
│ // Custom Debug impl for redaction                          │
│ // Different crate for diffing                              │
│ // Different crate for assertions                           │
│ // Each one: separate code generation, separate attributes  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

##### 4. Extension Attributes Comparison

```
┌─────────────────────────────────────────────────────────────┐
│ Format-Specific Attributes                                  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ KDL has arguments (positional) and properties (named).      │
│ Serde's data model doesn't distinguish them.                │
│                                                             │
│ With facet:                                                 │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ #[derive(Facet)]                                        │ │
│ │ struct Dependency {                                     │ │
│ │     #[facet(kdl::node_name)]                            │ │
│ │     name: String,           // node name = crate name   │ │
│ │                                                         │ │
│ │     #[facet(kdl::argument)]                             │ │
│ │     version: String,        // positional argument      │ │
│ │                                                         │ │
│ │     #[facet(kdl::property)]                             │ │
│ │     features: Vec<String>,  // named property           │ │
│ │ }                                                       │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ Parses:                                                     │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ serde "1.0" features=["derive", "std"]                  │ │
│ │ tokio "1.28" features=["full"]                          │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ With serde: You'd need a custom deserializer or wrapper     │
│ types. The derive macro can't express "this is a KDL        │
│ argument, not a property."                                  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

##### 5. The facet-value Crate

**A better intermediate representation than `serde_json::Value`.**

serde_json's `Value` type is JSON-shaped: strings, numbers, bools, arrays, objects, null.
That's fine for JSON, but it's the lowest common denominator. When you convert
YAML → Value → your types, you lose information.

facet-value's `Value` type is richer:

```
┌─────────────────────────────────────────────────────────────┐
│ facet_value::Value vs serde_json::Value                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Feature              serde_json::Value    facet::Value     │
│  ─────────────────────────────────────────────────────────  │
│  Strings              ✓                    ✓                │
│  Numbers              ✓ (f64 or i64)       ✓ (precise)      │
│  Booleans             ✓                    ✓                │
│  Arrays               ✓                    ✓                │
│  Objects              ✓                    ✓                │
│  Null                 ✓                    ✓                │
│  Bytes (Vec<u8>)      ✗ (encode as array)  ✓ native         │
│  DateTime             ✗ (encode as string) ✓ native         │
│  Source spans         ✗                    ✓ optional       │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Why it matters:                                            │
│                                                             │
│  • Binary data doesn't need base64 encoding/decoding        │
│  • DateTime values preserve type information                │
│  • Error messages can point to source locations             │
│  • Round-tripping through Value loses less information      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

Show example:
```rust
// With serde: bytes become array of numbers, datetime becomes string
// With facet: bytes stay bytes, datetime stays datetime

let value = facet_yaml::from_str::<facet_value::Value>(input)?;
// value preserves rich type information
let config: Config = facet_value::from_value(value)?;
// errors point back to original YAML source locations
```

##### 6. What Facet is NOT For

**Be explicit about non-goals and limitations.**

```
┌─────────────────────────────────────────────────────────────┐
│ What Facet is NOT                                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ ✗ NOT for maximum serialization speed                       │
│   If you're serializing millions of objects/second in a     │
│   hot loop, serde's generated code will be faster.          │
│                                                             │
│ ✗ NOT for minimum binary size (in small programs)           │
│   Facet has fixed overhead. For tiny binaries, serde wins.  │
│                                                             │
│ ✗ NOT for dynamic/plugin shape loading                      │
│   Shapes are STATIC. They're compiled into your binary.     │
│   You cannot load a Shape from a shared library or define   │
│   types at runtime. This is a fundamental design choice.    │
│                                                             │
│ ✗ NOT a replacement for runtime schema validation           │
│   Facet reflects Rust types. It doesn't validate arbitrary  │
│   data against external schemas (JSON Schema, etc.)         │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ Why shapes are static:                                      │
│                                                             │
│ Facet's power comes from compile-time knowledge. The Shape  │
│ knows field offsets, type layouts, vtable pointers — all    │
│ things that require the type to exist at compile time.      │
│                                                             │
│ This means:                                                 │
│ • No loading shapes from plugins/dlls                       │
│ • No defining types at runtime                              │
│ • No deserializing into "whatever the JSON says"            │
│   (use facet_value::Value for that)                         │
│                                                             │
│ This is a feature, not a bug. Static shapes enable safety   │
│ guarantees that dynamic reflection cannot provide.          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

##### 7. Beyond Serialization: The Ecosystem

Show that facet is more than just "another serde":

```
┌─────────────────────────────────────────────────────────────┐
│ The Facet Ecosystem                                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ SERIALIZATION FORMATS                                       │
│ ├── facet-json      JSON (text)                             │
│ ├── facet-yaml      YAML (text)                             │
│ ├── facet-toml      TOML (text)                             │
│ ├── facet-kdl       KDL (text, node-based)                  │
│ ├── facet-msgpack   MessagePack (binary)                    │
│ ├── facet-csv       CSV (text, tabular)                     │
│ └── ...more coming                                          │
│                                                             │
│ DEVELOPER TOOLS                                             │
│ ├── facet-pretty    Colored, formatted debug output         │
│ │                   • Redacts #[facet(sensitive)] fields    │
│ │                   • Works without Debug trait             │
│ │                   • Handles unprintable fields gracefully │
│ │                                                           │
│ ├── facet-diff      Structural diffing                      │
│ │                   • Compare values of DIFFERENT types     │
│ │                   • Meaningful diffs, not string diffs    │
│ │                   • Useful for snapshot testing           │
│ │                                                           │
│ ├── facet-assert    Better test assertions                  │
│ │                   • Shows exactly which fields differ     │
│ │                   • Colored, structured output            │
│ │                   • No Debug requirement                  │
│ │                                                           │
│ └── facet-value     Rich intermediate representation        │
│                     • Bytes, DateTime as first-class types  │
│                     • Source span tracking                  │
│                     • Better than serde_json::Value         │
│                                                             │
│ CORE                                                        │
│ ├── facet           The derive macro                        │
│ ├── facet-core      Shape, Def, traits (no_std)             │
│ └── facet-reflect   Peek, Partial (safe reflection API)     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

Each tool should have a mini-showcase demonstrating its value:

**facet-pretty example:**
```rust
#[derive(Facet)]
struct Config {
    name: String,
    #[facet(sensitive)]
    api_key: String,
    port: u16,
}

// Output:
// Config {
//   name: "my-app",
//   api_key: [REDACTED],
//   port: 8080,
// }
```

**facet-diff example:**
```rust
let old = Config { name: "v1", port: 8080 };
let new = Config { name: "v2", port: 9090 };

// Shows:
// Config {
//   name: "v1" → "v2",
//   port: 8080 → 9090,
// }
```

**facet-assert example:**
```rust
facet_assert::eq!(expected, actual);

// On failure:
// assertion failed: expected != actual
//
// Differences:
//   user.email: "old@example.com" → "new@example.com"
//   user.updated_at: 2024-01-01 → 2024-01-02
```

#### Build System Integration

Add to `build-website.rs`:

```rust
// Discover and run comparison examples
fn generate_comparison_showcase() {
    // 1. Build both binaries in release mode
    // 2. Measure binary sizes
    // 3. Run diagnostic examples, capture stderr
    // 4. Format into markdown/HTML
    // 5. Write to content/learn/why.md
}
```

The comparison crate should have:
```
samples/comparison/
├── Cargo.toml           # Dependencies on both serde_json AND facet_json
├── src/
│   ├── bin/
│   │   ├── serde_hello.rs
│   │   └── facet_hello.rs
│   └── lib.rs
├── examples/
│   ├── diagnostics.rs   # Side-by-side error comparison
│   ├── flexibility.rs   # One derive, many uses
│   └── extensions.rs    # KDL attribute showcase
└── README.md            # How to reproduce locally
```

#### Page Structure (Generated)

```markdown
# Why Facet?

> **A fresh take on serialization** — and much more.

Facet is a reflection-based alternative to serde. It makes different tradeoffs.
This page shows real comparisons you can verify yourself.

[All examples link to source code. All numbers are reproducible.]

---

## What Facet Is

More than just serialization:

- **Serialization**: JSON, YAML, TOML, KDL, MessagePack, and more
- **Pretty-printing**: Colored output with sensitive field redaction
- **Diffing**: Structural comparison between values
- **Assertions**: Clear test failure messages showing exactly what differs
- **Introspection**: Query type information at runtime

All from a single `#[derive(Facet)]`.

[Auto-generated ecosystem diagram]

---

## Where Facet Loses

### Binary Size (Scenario Comparison)

[Auto-generated: 4 scenarios from hello-world to 100+ types]

**Summary:**
- Small programs: ~2-3x larger (fixed overhead)
- Medium programs: ~1.5-2x larger (overhead amortizes)
- Large programs with multiple formats: comparable or smaller

### Execution Speed

[Auto-generated benchmark comparison]

Facet is slower. Runtime reflection can't match compile-time code generation.
If you're serializing millions of objects per second in a hot loop, use serde.

---

## Where Facet Wins

### Error Messages

[Auto-generated: side-by-side error comparison, multiple scenarios]

- Unknown field with typo suggestion
- Type mismatch with source location
- Nested object path in error
- Array index errors

### One Derive, Many Tools

[Auto-generated: same type used with 6 different facet crates]

### Extensible Attributes

[Auto-generated: KDL example with kdl::argument, kdl::property]

### A Better Value Type

[Auto-generated: facet_value::Value vs serde_json::Value comparison]

- Native bytes support (no base64)
- Native datetime support
- Source span tracking for errors

---

## What Facet is NOT For

- **Maximum speed**: Use serde for hot loops
- **Minimum size** (small programs): Use serde for tiny binaries
- **Dynamic shapes**: Shapes are static, compiled in
- **Plugin/DLL shape loading**: Not supported by design
- **External schema validation**: Facet reflects Rust types, not JSON Schema

[Explanation of why shapes are static and why that's a feature]

---

## The Philosophy

Facet bets that for most applications:

1. **Features matter more than microseconds**
2. **Flexibility matters more than micro-optimization**
3. **Data beats code** — introspectable shapes enable tools we haven't imagined

---

## Try It Yourself

\`\`\`bash
git clone https://github.com/facet-rs/facet
cd facet/samples/comparison
./measure-sizes.sh        # Binary size comparison
cargo run --example diagnostics  # Error message comparison
\`\`\`

Every comparison on this page can be reproduced locally.
```

#### Key Principles

1. **Losses first, wins second** — Lead with honesty. Users who need speed/size will self-select out. Users who stay will trust you.

2. **Everything verifiable** — Every number links to code. Every example can be reproduced with a shell script.

3. **Multiple scenarios, not cherry-picking** — Don't show one "2.9x larger" number. Show the full picture:
   - Hello world (worst case for facet)
   - Medium app (realistic)
   - Large app with multiple formats (where facet shines)
   - Let users interpolate their own situation

4. **No spin, but full context** — Show real numbers, but explain what they mean. "2.9x larger" sounds bad until you realize it's the absolute worst case and the overhead is constant.

5. **Source links everywhere** — `[View source]` `[Reproduce locally]` on every comparison.

6. **The philosophy comes last** — After seeing the data, users understand *why* these tradeoffs were made.

7. **Be explicit about non-goals** — Static shapes, no plugins, no runtime type definition. These are features, not limitations. Explain why.

#### What This Replaces

This generated comparison page replaces the hand-written "Why Facet?" prose. The prose was good for explaining concepts, but concrete, verifiable comparisons are more convincing.

Keep some prose for:
- The tagline and intro ("A fresh take on serialization")
- The ecosystem overview (what facet offers beyond serialization)
- The "what facet is NOT for" section
- The philosophy section at the end
- Transitions between comparisons

But the meat of the page should be generated data.

---

## 1. Clarity Issues

### 1.1 Homepage is Scattered

**Location:** `content/_index.md`

**Problem:** The homepage jumps between concepts without a clear narrative flow. It goes from "what is facet" → "crash course" → "reflection abstractions" → "use cases" → "serde comparison" → "code generation" → "specialization" → random feature ideas. The note on line 20 explicitly admits this.

**Current flow:**
1. One-sentence definition
2. List of use cases
3. "Note: documentation is being restructured" warning
4. Crash course (derive macro)
5. Shape type explanation
6. Reflection (Peek/Partial)
7. Use cases expanded (Debug, assert, serde comparison)
8. Code generation
9. Specialization
10. Future ideas (debuggers, diffing, XML/KDL, JSON schemas, Error derive)
11. Contributing one-liner

**Proposed flow:**
1. **Hero section**: Clear value proposition in 2-3 sentences
2. **Quick start**: `cargo add facet facet-json` + minimal example
3. **Core concept**: What is `Shape` and why it matters (brief)
4. **Use cases**: Organized cards/sections for each major use case
5. **Why not serde?**: Compile time benefits, feature comparison
6. **Ecosystem**: Links to format crates, tools
7. **Learn more**: Links to guides, API docs, showcases
8. **Contributing**: Expanded section

**Specific changes needed:**
- Remove the "Note: documentation is being restructured" warning or replace with something actionable
- Add a proper "Installation" section at the top with:
  ```toml
  [dependencies]
  facet = "0.x"
  facet-json = "0.x"  # or whichever format crate
  ```
- Move the "Crash course" to be the first major section
- Group the scattered "ideas" (debuggers, diffing, XML/KDL) into a "Future directions" or "Research" section
- Add clear section headers with consistent hierarchy

---

### 1.2 No Clear "Getting Started" Path

**Location:** Missing page, should be `content/getting-started.md`

**Problem:** A new user landing on the site has no clear path to "I want to try this right now." The homepage assumes familiarity with serde concepts and jumps into technical details.

**Proposed new page structure:**

```markdown
+++
title = "Getting Started"
+++

## Installation

Add facet to your project:

\`\`\`toml
[dependencies]
facet = "0.x"
\`\`\`

For JSON serialization/deserialization:

\`\`\`toml
[dependencies]
facet = "0.x"
facet-json = "0.x"
\`\`\`

## Your First Facet Type

\`\`\`rust
use facet::Facet;

#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
}
\`\`\`

## Serializing to JSON

\`\`\`rust
use facet_json::to_string;

let person = Person {
    name: "Alice".into(),
    age: 30,
};

let json = to_string(&person)?;
// {"name":"Alice","age":30}
\`\`\`

## Deserializing from JSON

\`\`\`rust
use facet_json::from_str;

let json = r#"{"name":"Bob","age":25}"#;
let person: Person = from_str(json)?;
\`\`\`

## Next Steps

- [Explore format showcases](/showcases/) - See examples for JSON, YAML, KDL, and more
- [Compare with serde](/serde-comparison/) - If you're migrating from serde
- [Format support matrix](/format-crate-matrix/) - See what's supported in each format
```

**Additional considerations:**
- Should include feature flags explanation if relevant
- Should mention `no_std` support early for embedded users
- Should link to docs.rs for API reference

---

### 1.3 Missing Cargo Dependency Examples

**Locations:**
- `content/_index.md` - no cargo snippets
- `content/extension-attributes.md` - mentions `use facet_kdl as kdl` but no cargo dep

**Problem:** Users see code examples but don't know what crates to add to Cargo.toml.

**Specific additions needed:**

In `_index.md`, after the crash course section, add:
```markdown
### Adding facet to your project

\`\`\`toml
[dependencies]
facet = "0.x"

# Add format crates as needed:
facet-json = "0.x"
facet-yaml = "0.x"
facet-toml = "0.x"
facet-kdl = "0.x"
\`\`\`
```

In `extension-attributes.md`, at the start of the KDL section, add:
```markdown
First, add facet-kdl to your dependencies:

\`\`\`toml
[dependencies]
facet-kdl = "0.x"
\`\`\`

Then import it with an alias to enable the `kdl::` attribute namespace:

\`\`\`rust
use facet_kdl as kdl;
\`\`\`
```

---

### 1.4 Typo in serde-comparison.md

**Location:** `content/serde-comparison.md:252-253`

**Problem:** Extra closing parenthesis in code example.

**Current:**
```rust
    #[facet(default = 42))]
```

**Should be:**
```rust
    #[facet(default = 42)]
```

---

### 1.5 Empty "Target Type" Sections in Showcases

**Location:** `content/showcases/json.md` lines 103-105, 143-145, 183-185, 227-229, 255-257, 283-285, and similar

**Problem:** Several showcase scenarios have empty `<pre>` blocks for the "Target Type" section. This happens when the example code doesn't include the struct definition inline (probably because it's defined elsewhere in the example).

**Examples of empty sections:**
- "Externally Tagged Enum (default)" - line 103
- "Internally Tagged Enum" - line 143
- "Adjacently Tagged Enum" - line 183
- "Untagged Enum" - line 227
- "Maps with String Keys" - line 255
- "Maps with Integer Keys" - line 283

**Solution options:**
1. **Fix the showcase generator** (`build-website.rs`) to always include type definitions
2. **Manually add missing type definitions** to showcase output
3. **Hide the "Target Type" section** when empty (CSS or template change)

**Recommended approach:** Fix the showcase generator to include all relevant type definitions. This may require changes to how examples emit their showcase output.

---

### 1.6 Showcases Lack Contextual Explanation

**Location:** All showcase files in `content/showcases/`

**Problem:** Showcases show input/output but don't explain:
- When you'd use this pattern
- What the attributes mean
- Common gotchas

**Example - json.md "Internally Tagged Enum":**

Current description (line 140):
> Enum with internal tagging using `#[facet(tag = "type")]` - variant name becomes a field.

Could be expanded to:
> **When to use:** Internal tagging is ideal for APIs where the type discriminator should be a regular field in the JSON object, commonly used in JSON-RPC and similar protocols.
>
> **Attribute:** `#[facet(tag = "type")]` places the variant name in a field called "type". You can use any field name.
>
> **Note:** This only works with struct variants - tuple variants would conflict with the tag field.

**Recommendation:** Add a "Context" or "When to use" paragraph to each major showcase scenario. This could be done in the example source files that generate the showcases.

---

## 2. Correctness Issues

### 2.1 Outdated "Being Restructured" Note

**Location:** `content/_index.md:20`

**Current text:**
> Note: This documentation is being restructured. If it feels a bit scattered, that's because it is! We're working on it.

**Problem:** This note has no date and gives an impression of abandonment. It doesn't tell users what to do or when to expect improvements.

**Options:**
1. **Remove entirely** - Just fix the docs and don't apologize
2. **Replace with versioned note** - "Documentation for facet v0.x. See [changelog] for updates."
3. **Replace with contribution CTA** - "Help us improve these docs! [Contribute on GitHub]"

**Recommendation:** Remove the note entirely. Apologetic notes don't help users and suggest the project is unstable.

---

### 2.2 Verify GitHub Issue Links

**Location:** `content/_index.md` lines 145, 149-151, 156

**Links to verify:**
- Line 145: `https://github.com/facet-rs/facet/issues/102` - "Better debuggers"
- Line 149: `https://github.com/facet-rs/facet/issues/145` - "Diffing"
- Line 150: `https://github.com/facet-rs/facet/issues/150` - "XML issue"
- Line 151: `https://github.com/facet-rs/facet/issues/151` - "KDL issue"

**Verification needed:**
- [ ] Are these issues still open?
- [ ] Are the issue titles/descriptions still accurate?
- [ ] Should any be updated to link to PRs or completed features?

**If issues are closed:** Update text to reflect current state (e.g., "KDL support is now available via facet-kdl")

---

### 2.3 No Version Information

**Location:** Site-wide issue

**Problem:** Users don't know which version of facet the documentation covers. This is especially important for a rapidly evolving project.

**Solutions:**
1. Add version to `config.toml` and display in footer/header
2. Add version badge to homepage (linking to crates.io)
3. Add "Last updated" date to pages

**Implementation:**

In `config.toml`, add:
```toml
[extra]
facet_version = "0.x.x"
```

In `templates/base.html`, add to nav or footer:
```html
<span class="version">v{{ config.extra.facet_version }}</span>
```

Or add a crates.io badge to `_index.md`:
```markdown
[![Crates.io](https://img.shields.io/crates/v/facet.svg)](https://crates.io/crates/facet)
```

---

### 2.4 Format Matrix May Be Stale

**Location:** `content/format-crate-matrix.md`

**Problem:** Many cells show 🟡 (partial/untested). These may now be fully tested. The matrix should be regenerated or manually verified.

**Cells marked 🟡 that need verification:**
- u128/i128 support across all formats (line 39)
- NonZero integers in most formats (line 42)
- Various KDL features (HashMap, BTreeMap, etc.)
- Most features in asn1, xdr, args, urlenc, csv

**Process to verify:**
1. Run the test suite for each format crate
2. Check if tests exist for 🟡 features
3. Update to ✅ if tests pass, 🚫 if explicitly unsupported

**Automation idea:** Generate this matrix from test results or feature flags rather than maintaining manually.

---

### 2.5 Showcase "Target Type" Shows Incomplete Types

**Location:** `content/showcases/json.md:700-701`

**Problem:** Some type displays are mangled:
```
struct (…)(i32, i32);
```

Should probably be:
```rust
#[derive(Facet)]
struct Point(i32, i32);
```

This suggests the showcase generator has a bug with tuple struct names.

---

## 3. Structure Issues

### 3.1 No Clear Content Hierarchy

> **Note:** This section is superseded by [Section 0: Audience-Based Documentation Structure](#0-audience-based-documentation-structure), which proposes organizing by Learn/Extend/Contribute audiences rather than by topic.

**Current structure (flat):**
```
/                           # Home
/serde-comparison/          # Comparison guide
/extension-attributes/      # Guide
/format-crate-matrix/       # Reference
/showcases/json/            # Example
/showcases/yaml/            # Example
/showcases/kdl/             # Example
...
```

**Problem:** Content is organized by what it is (guide, reference, example) rather than by who needs it. A user looking to serialize JSON doesn't need to see extension attribute internals. A format crate developer doesn't need the serde migration guide.

**See Section 0 for the proposed `/learn/`, `/extend/`, `/contribute/` structure.**

---

### 3.2 Missing Showcase Index Page

**Location:** Need to create `content/showcases/_index.md`

**Problem:** Navigation links directly to `/showcases/kdl/` - users can't discover other showcases without knowing URLs.

**Proposed content:**
```markdown
+++
title = "Showcases"
template = "section.html"
+++

Interactive examples demonstrating facet's serialization and deserialization capabilities.

## Format Showcases

- [JSON](/showcases/json/) - facet-json serialization and deserialization with error examples
- [YAML](/showcases/yaml/) - facet-yaml examples
- [KDL](/showcases/kdl/) - facet-kdl with KDL-specific attributes

## Feature Showcases

- [Assertions](/showcases/assert/) - Structural diffing with facet-assert
- [Value Deserialization](/showcases/from-value/) - Converting facet-value to typed structs
- [Error Diagnostics](/showcases/diagnostics/) - Rich error messages with source spans
- [Span Highlighting](/showcases/spans/) - How errors point to source locations
```

**Also need to update navigation** in `templates/base.html:21`:
```html
<a href="/showcases/">Showcases</a>
```
(Currently links to `/showcases/kdl/`)

---

### 3.3 Extension Attributes Not in Navigation

**Location:** `templates/base.html:19-24`

**Current navigation:**
```html
<a href="/">Home</a>
<a href="/showcases/kdl/">Showcases</a>
<a href="/format-crate-matrix/">Formats</a>
<a href="/serde-comparison/">Serde</a>
```

**Problem:** Extension attributes guide exists but isn't discoverable from navigation.

**Options:**
1. Add as top-level nav item (clutters nav)
2. Add dropdown for "Guides" containing serde-comparison and extension-attributes
3. Add to a "Guides" section page that's linked from nav

**Recommended:** Create a "Guides" dropdown or section:
```html
<div class="nav-dropdown">
    <a href="/guides/">Guides</a>
    <div class="dropdown-content">
        <a href="/serde-comparison/">Serde Comparison</a>
        <a href="/extension-attributes/">Extension Attributes</a>
    </div>
</div>
```

---

### 3.4 Design Documents Not Linked

**Location:** `docs/design/value-error-diagnostics.md` exists but isn't linked anywhere

**Problem:** Valuable design context exists but is undiscoverable.

**Options:**
1. Link from relevant documentation pages (e.g., link from error showcase)
2. Create a "Design Documents" or "Architecture" section
3. Add to Contributing guide

**Recommendation:** Add links from relevant pages. For example, in the error diagnostics showcase or from-value showcase:
```markdown
> **Design note:** See [Value Error Diagnostics Design Doc](/design/value-error-diagnostics/) for the rationale behind this error reporting approach.
```

---

### 3.5 Missing Pages

**Pages that should exist:**

#### 3.5.1 Migration from Serde Guide
**Location:** `content/guides/serde-migration.md` (or expand `serde-comparison.md`)

**Content outline:**
1. Why migrate?
2. Step-by-step migration process
3. Attribute mapping table (already exists)
4. Common migration patterns
5. Handling unsupported serde features
6. Testing your migration

#### 3.5.2 Troubleshooting / FAQ
**Location:** `content/troubleshooting.md`

**Content outline:**
1. Common error messages and solutions
2. "Why doesn't X work?" for common issues
3. Performance considerations
4. no_std troubleshooting

#### 3.5.3 Contributing Guide
**Location:** `content/contributing.md`

**Current state:** One line in `_index.md`:
> Contributions are welcome! Check out the [GitHub repository](https://github.com/facet-rs/facet) to get started.

**Should include:**
1. Development setup
2. Running tests (`cargo nextest run` per CLAUDE.md)
3. Documentation contribution process
4. Code style guidelines
5. PR process

---

## 4. Tone Issues

### 4.1 Inconsistent Formality

**Problem:** Some sections are conversational/tutorial-style, others are terse reference-style.

**Examples of conversational (good for tutorials):**
- `_index.md:64`: "What can you build with it?"
- `_index.md:86`: "Wouldn't it be better to have access to..."
- `_index.md:170`: "We still haven't figured everything facet can do. Come do research with us"

**Examples of terse/reference (good for matrices):**
- `format-crate-matrix.md`: Just tables with no prose
- `extension-attributes.md:272-278`: Technical limitation explanation

**Recommendation:**
- Guides/tutorials: Conversational, explain "why", use second person ("you")
- Reference pages: Concise, factual, use third person
- Showcases: Minimal prose, let code speak

**Specific tone improvements:**

`format-crate-matrix.md` could use introductory paragraphs for each section:
```markdown
## Scalar Types

All facet format crates support the basic Rust scalar types. The main variation
is in `u128`/`i128` support, which depends on the underlying format's capabilities.

| Type | json | kdl | ...
```

---

### 4.2 Missing Calls-to-Action

**Problem:** Pages often end abruptly without telling users what to do next.

**Locations needing "Next Steps" sections:**

1. **_index.md** - Ends with one-line Contributing section

   Add:
   ```markdown
   ## Next Steps

   - [Get started](/getting-started/) - Install facet and write your first example
   - [Browse showcases](/showcases/) - See facet in action
   - [Format comparison](/format-crate-matrix/) - Choose the right format crate
   ```

2. **serde-comparison.md** - Ends mid-table

   Add:
   ```markdown
   ## Ready to Migrate?

   See the [migration guide](/guides/serde-migration/) for step-by-step instructions.
   ```

3. **extension-attributes.md** - Ends with limitations section

   Add:
   ```markdown
   ## Next Steps

   - [KDL showcase](/showcases/kdl/) - See KDL attributes in action
   - [Create your own](/guides/custom-attributes/) - Build custom attribute namespaces
   ```

4. **Showcases** - End after last example

   Add footer to each:
   ```markdown
   ---

   ## More Examples

   - [Other format showcases](/showcases/)
   - [Full API documentation](https://docs.rs/facet-json)
   ```

---

### 4.3 Passive Voice in Technical Explanations

**Problem:** Some explanations use passive voice where active would be clearer.

**Example from extension-attributes.md:272:**
> Extension attributes are validated at **runtime**, not compile time.

Better:
> Facet validates extension attributes at **runtime**, not compile time.

**Example from _index.md:106:**
> With `facet`, serialization and deserialization is implemented:

Better:
> Facet implements serialization and deserialization:

**Recommendation:** Do a pass through all documentation converting passive to active voice where it improves clarity.

---

## 5. Navigation Issues

### 5.1 No Sidebar for Long Pages

**Location:** `content/format-crate-matrix.md` (157 lines, many tables)

**Problem:** The format matrix page is very long with many sections. Users must scroll to find what they need.

**Solution:** Add a sticky sidebar TOC.

**Implementation in `templates/page.html`:**
```html
{% if page.extra.show_toc | default(value=false) %}
<aside class="page-toc">
    <nav>
        <h4>On this page</h4>
        <ul>
        {% for h2 in page.toc %}
            <li><a href="#{{ h2.id }}">{{ h2.title }}</a></li>
        {% endfor %}
        </ul>
    </nav>
</aside>
{% endif %}
```

Then in `format-crate-matrix.md` frontmatter:
```toml
+++
title = "Format crates comparison"
[extra]
show_toc = true
+++
```

**CSS needed in `sass/main.scss`:**
```scss
.page-toc {
    position: sticky;
    top: 1rem;
    max-height: calc(100vh - 2rem);
    overflow-y: auto;
    // ... styling
}
```

---

### 5.2 No Breadcrumbs

**Problem:** Users navigating deep into showcases have no visual indication of where they are in the site hierarchy.

**Solution:** Add breadcrumb component.

**Implementation in `templates/page.html`:**
```html
<nav class="breadcrumbs" aria-label="Breadcrumb">
    <ol>
        <li><a href="/">Home</a></li>
        {% if page.ancestors %}
            {% for ancestor in page.ancestors %}
                {% set section = get_section(path=ancestor) %}
                <li><a href="{{ section.permalink }}">{{ section.title }}</a></li>
            {% endfor %}
        {% endif %}
        <li aria-current="page">{{ page.title }}</li>
    </ol>
</nav>
```

---

### 5.3 Showcase Navigation Goes to Single Page

**Location:** `templates/base.html:21`

**Current:**
```html
<a href="/showcases/kdl/">Showcases</a>
```

**Problem:** Links to KDL showcase specifically, not a showcase index.

**Fix:** Change to:
```html
<a href="/showcases/">Showcases</a>
```

And create `content/showcases/_index.md` (see 3.2).

---

### 5.4 TOC Only Works for Showcases

**Location:** `templates/base.html:38-51`

**Current behavior:** JavaScript builds TOC from `.showcase h3[id]` elements only.

**Problem:** Regular pages with `## Headings` don't get a TOC.

**Solution:** Extend the TOC logic or use Zola's built-in TOC:

Option 1 - Use Zola's TOC (recommended):
```html
{% if page.toc %}
<aside class="toc">
    <h4>On this page</h4>
    <ul>
    {% for h1 in page.toc %}
        <li>
            <a href="#{{ h1.id }}">{{ h1.title }}</a>
            {% if h1.children %}
            <ul>
                {% for h2 in h1.children %}
                <li><a href="#{{ h2.id }}">{{ h2.title }}</a></li>
                {% endfor %}
            </ul>
            {% endif %}
        </li>
    {% endfor %}
    </ul>
</aside>
{% endif %}
```

Option 2 - Extend JavaScript:
```javascript
// Also handle regular markdown headings
const content = document.querySelector('.content');
if (toc && content && toc.children.length === 0) {
    const headers = content.querySelectorAll('h2[id], h3[id]');
    // ... build TOC
}
```

---

### 5.5 Search Keyboard Shortcut Not Visible

**Location:** `templates/base.html:58-59`

**Current:** Placeholder shows "Search ⌘K" but:
1. Only shows ⌘K, not Ctrl+K for non-Mac users
2. Slash (/) shortcut not mentioned

**Improvement:**
```javascript
const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
const shortcut = isMac ? '⌘K' : 'Ctrl+K';

new PagefindUI({
    element: "#search",
    translations: {
        placeholder: `Search (${shortcut} or /)`
    }
});
```

---

### 5.6 No Footer

**Location:** `templates/base.html` - no footer element

**Problem:** No footer means no:
- Sitemap/quick links
- Version info
- Last updated date
- License info
- Social/community links

**Proposed footer:**
```html
<footer class="site-footer">
    <div class="footer-content">
        <div class="footer-section">
            <h4>Documentation</h4>
            <ul>
                <li><a href="/getting-started/">Getting Started</a></li>
                <li><a href="/showcases/">Showcases</a></li>
                <li><a href="/format-crate-matrix/">Format Support</a></li>
            </ul>
        </div>
        <div class="footer-section">
            <h4>Community</h4>
            <ul>
                <li><a href="https://github.com/facet-rs/facet">GitHub</a></li>
                <li><a href="https://crates.io/crates/facet">crates.io</a></li>
                <li><a href="https://docs.rs/facet">docs.rs</a></li>
            </ul>
        </div>
        <div class="footer-section">
            <p>facet v{{ config.extra.facet_version }}</p>
            <p>Licensed under MIT/Apache-2.0</p>
        </div>
    </div>
</footer>
```

---

## Implementation Priority

### Strategic Priority: Audience-Based Restructure

The **Section 0** restructure (Learn/Extend/Contribute guides) should be the primary initiative. However, it can proceed in phases alongside quick fixes.

**Recommended approach:**
1. Do P0 quick fixes immediately (they're independent)
2. Start Phase 1 of Section 0 (create structure, move content)
3. Weave in P1-P2 items as part of filling guide content
4. P3-P4 items fold into the new structure naturally

---

### P0 - Quick Wins (< 1 hour each)

Do these immediately, independent of restructure:

- [ ] 1.4 Fix typo in serde-comparison.md
- [ ] 2.1 Remove "being restructured" note
- [ ] 5.3 Fix showcase nav link
- [ ] 5.5 Improve search placeholder

### Phase 1 - Structure Foundation (1-2 days)

Set up the audience-based structure:

- [ ] Create `/learn/`, `/extend/`, `/contribute/` directory structure
- [ ] Create `_index.md` landing pages for each guide
- [ ] Update navigation to show three tracks
- [ ] Move existing content to appropriate locations (per migration table in Section 0)
- [ ] Set up redirects for old URLs

### Phase 2 - Learn Guide (High Priority)

Fill the Learn Guide - this is what most visitors need:

- [ ] **`/learn/why/` - "Why Facet?" (WRITE THIS FIRST - see detailed spec in Section 0)**
- [ ] `/learn/getting-started/` - Installation, first example (was 1.2)
- [ ] `/learn/attributes/` - Complete attribute reference
- [ ] `/learn/migration/` - Serde comparison + migration steps (was 3.5.1)
- [ ] `/learn/formats/json/` - JSON-specific guide
- [ ] `/learn/formats/kdl/` - KDL-specific guide (incl. kdl:: attributes)
- [ ] `/learn/showcases/` - Polish and add index (was 3.2)
- [ ] `/learn/errors/` - Error message explanations
- [ ] `/learn/faq/` - Common questions (was 3.5.2)

### Phase 3 - Extend Guide

Fill the Developer Guide - for tool/format builders:

- [ ] `/extend/overview/` - What facet provides
- [ ] `/extend/shape/` - Shape, Def, fields, variants deep-dive
- [ ] `/extend/peek/` - Reading values tutorial
- [ ] `/extend/partial/` - Building values tutorial
- [ ] `/extend/extension-attrs/` - Creating custom namespaces (move existing)
- [ ] `/extend/format-crate/` - "Build a format crate" guide
- [ ] `/extend/examples/` - Annotated facet-json walkthrough

### Phase 4 - Contribute Guide

Fill the Contribute Guide - for contributors:

- [ ] `/contribute/architecture/` - Crate graph, design philosophy
- [ ] `/contribute/derive-macro/` - facet-derive internals
- [ ] `/contribute/vtables/` - ValueVTable, Characteristic design
- [ ] `/contribute/memory/` - Layout, unsafe patterns
- [ ] `/contribute/design/` - Move existing design docs
- [ ] `/contribute/adding-types/` - How to add new std types
- [ ] `/contribute/contributing/` - Dev setup, testing, PR process (was 3.5.3)

### Phase 5 - Polish

After structure is in place:

- [ ] 5.1 Add sidebar TOC for long pages
- [ ] 5.6 Add footer
- [ ] 5.2 Add breadcrumbs
- [ ] 4.2 Add "Next Steps" sections to all pages
- [ ] 4.1 Tone consistency pass
- [ ] 4.3 Active voice pass

### Backlog - As Needed

- [ ] 2.2 Verify all GitHub issue links
- [ ] 2.4 Audit format matrix accuracy
- [ ] 1.5 Fix empty Target Type sections (requires build-website.rs changes)
- [ ] 1.6 Add context to showcase examples
