# RFC: TypePlan Precomputation for Deserialization

**Issue**: #1951
**Status**: Draft
**Author**: Claude (with Amos)

## Summary

Precompute a "TypePlan" tree at `Partial` construction time to eliminate repeated runtime lookups during deserialization. This turns runtime interpretation of `Shape` into execution of a precompiled plan.

## Motivation

Callgrind profiling of `citm_catalog.json` deserialization shows significant overhead in runtime lookups that produce the same answers every time for a given type:

| Function | Instructions | % of Total |
|----------|-------------|------------|
| `begin_custom_deserialization_from_shape_with_format` | 1.6M | 1.32% |
| `end()` | 3.4M | 2.75% |
| `begin_nth_field` | 1.5M | 1.25% |
| `fill_defaults` | 1.2M | 0.94% |

Most of this answers the same questions repeatedly:
- Does this type have a proxy? (almost always no)
- Is this an Option/Result/List/Map/Struct/Enum?
- What fields does this struct have?
- Does this field have a default?

## Current Architecture

### Deserialization Flow

```
deserialize_into(wip: Partial)
  ├─ Check for raw_capture_shape()
  ├─ Check container-level proxy (begin_custom_deserialization_from_shape_with_format)
  ├─ Check field-level proxy (wip.parent_field()?.effective_proxy(format_ns))
  ├─ Check Def::Option, Def::Result
  ├─ Check shape.builder_shape, Def::Pointer, shape.inner
  ├─ Check metadata_container
  └─ Match shape.ty / shape.def → dispatch to appropriate deserializer
```

### Hot Paths Identified

1. **Struct field lookup** (`struct_simple.rs:114-118`)
   - Current: O(n) linear scan per input field
   - `struct_def.fields.iter().find(|f| field_matches(f, key_name))`

2. **Deferred mode path building** (`fields.rs:239`)
   - Current: O(n) scan to get field name from index
   - `get_field_name_for_path(idx)`

3. **Proxy resolution** (`entry.rs:52-55`)
   - Current: O(m) where m = number of format proxies
   - `field.effective_proxy(format_ns)`

4. **Default field detection** (`mod.rs:952-973`)
   - Scan field attributes at runtime

## Proposed Solution

### TypePlan Data Structures

```rust
/// Precomputed deserialization plan for a type
pub struct TypePlan {
    pub shape: &'static Shape,
    pub kind: TypePlanKind,
    pub proxy: Option<&'static ProxyDef>,
    pub has_default: bool,
}

pub enum TypePlanKind {
    Scalar {
        scalar_def: &'static ScalarDef,
    },
    Struct(StructPlan),
    Enum(EnumPlan),
    Option {
        inner: Box<TypePlan>,
    },
    Result {
        ok: Box<TypePlan>,
        err: Box<TypePlan>,
    },
    List {
        item: Box<TypePlan>,
    },
    Map {
        key: Box<TypePlan>,
        value: Box<TypePlan>,
    },
    Set {
        item: Box<TypePlan>,
    },
    Array {
        item: Box<TypePlan>,
        len: usize,
    },
    Pointer {
        pointee: Box<TypePlan>,
    },
    Transparent {
        inner: Box<TypePlan>,
    },
}

pub struct StructPlan {
    pub struct_def: &'static StructType,
    pub fields: Vec<FieldPlan>,
    pub field_lookup: FieldLookup,  // name/alias → index
    pub has_flatten: bool,
    pub required_fields: ISet,  // fields without defaults
}

pub struct FieldPlan {
    pub field: &'static Field,
    pub name: &'static str,
    pub effective_name: &'static str,
    pub alias: Option<&'static str>,
    pub has_default: bool,
    pub is_required: bool,
    pub proxy: Option<&'static ProxyDef>,
    pub child_plan: Box<TypePlan>,  // recursive
}

pub struct EnumPlan {
    pub enum_def: &'static EnumType,
    pub variants: Vec<VariantPlan>,
    pub variant_lookup: VariantLookup,  // name → index
}

pub struct VariantPlan {
    pub variant: &'static Variant,
    pub fields: Vec<FieldPlan>,
    pub field_lookup: FieldLookup,
}
```

### Field Lookup Optimization

Replace O(n) linear scan with O(1) or O(log n):

```rust
pub enum FieldLookup {
    /// For small structs (≤8 fields): linear scan is faster than hashing
    Small {
        names: Vec<(&'static str, usize)>,  // (effective_name, index)
        aliases: Vec<(&'static str, usize)>,
    },
    /// For larger structs: use a hash map
    Hash {
        by_name: HashMap<&'static str, usize>,
        by_alias: HashMap<&'static str, usize>,
    },
}

impl FieldLookup {
    pub fn find(&self, name: &str) -> Option<usize> {
        match self {
            Self::Small { names, aliases } => {
                names.iter().find(|(n, _)| *n == name).map(|(_, i)| *i)
                    .or_else(|| aliases.iter().find(|(n, _)| *n == name).map(|(_, i)| *i))
            }
            Self::Hash { by_name, by_alias } => {
                by_name.get(name).or_else(|| by_alias.get(name)).copied()
            }
        }
    }
}
```

### Plan Construction

Build plan eagerly when `Partial` is created:

```rust
impl TypePlan {
    pub fn build(shape: &'static Shape, format_ns: Option<&str>) -> Self {
        let proxy = shape.effective_proxy(format_ns);
        let has_default = shape.is(Characteristic::Default);

        let kind = match &shape.def {
            Def::Scalar => TypePlanKind::Scalar {
                scalar_def: /* ... */
            },
            Def::Option(opt) => TypePlanKind::Option {
                inner: Box::new(Self::build(opt.t(), format_ns)),
            },
            // ... etc
        };

        TypePlan { shape, kind, proxy, has_default }
    }
}
```

### Integration Points

1. **In `Partial::alloc`**: Build TypePlan alongside allocation
2. **In `deserialize_into`**: Use plan dispatch instead of sequential checks
3. **In `deserialize_struct_simple`**: Use plan's field lookup table
4. **In `begin_nth_field` (deferred mode)**: Use plan's field index→name map

## Implementation Plan

### Phase 1: Infrastructure
- [ ] Create `facet-reflect/src/partial/typeplan.rs` module
- [ ] Define core TypePlan structures
- [ ] Implement `TypePlan::build()` for basic types

### Phase 2: Field Lookup
- [ ] Implement `FieldLookup` with small/hash strategies
- [ ] Integrate into `deserialize_struct_simple`
- [ ] Benchmark improvement

### Phase 3: Proxy Resolution
- [ ] Precompute proxies per (shape, format) pair
- [ ] Update `deserialize_into` to use precomputed proxies

### Phase 4: Full Integration
- [ ] Store TypePlan in Partial or Deserializer
- [ ] Update all dispatch paths
- [ ] Add caching for repeated deserializations

### Phase 5: Optimization
- [ ] Profile and tune field lookup threshold
- [ ] Consider compile-time generation in derive macro

## Baseline Measurements (2026-01-28)

### Test Files
- `citm_catalog.json` (1.7MB, deeply nested structs, many HashMaps)
- `twitter.json` (632KB, many small objects with lots of optional fields)

### Callgrind Results (Instructions)

| File | Total Instructions |
|------|-------------------|
| citm_catalog.json | 123,367,403 |
| twitter.json | 63,316,459 |

### Perf Stats (10 runs)

| File | Time | Cycles | Instructions | IPC | Branch Misses |
|------|------|--------|--------------|-----|---------------|
| citm_catalog.json | 16.55ms ± 1.49% | 59.4M | 127.6M | 2.15 | 0.77% |
| twitter.json | 8.71ms ± 1.15% | 30.2M | 66.2M | 2.19 | 1.41% |

### Heaptrack Results (Allocations)

| File | Total Allocations | Temporary | Leaked |
|------|-------------------|-----------|--------|
| citm_catalog.json | 3,905 | 175 | 1 |
| twitter.json | 5,820 | 79 | 1 |

### Top Functions by Instructions (citm_catalog.json)

| Function | Instructions | % |
|----------|-------------|---|
| `Scanner::next_token` | 12,490,592 | 10.12% |
| `JsonParser::consume_token` | 6,222,053 | 5.04% |
| `JsonParser::produce_event` | 5,070,583 | 4.11% |
| `Scanner::scan_string_content` | 4,412,458 | 3.58% |
| `deserialize_into` | 4,396,058 | 3.56% |
| `deserialize_struct` | 4,379,850 | 3.55% |
| **`Partial::end`** | 3,434,909 | **2.78%** |
| `expect_event` | 3,231,930 | 2.62% |
| **`begin_custom_deserialization_from_shape_with_format`** | 1,633,140 | **1.32%** |
| **`begin_nth_field`** | 1,545,427 | **1.25%** |
| **`fill_defaults`** | 1,165,033 | **0.94%** |
| `begin_list_item` | 1,001,130 | 0.81% |

**Bold = TypePlan optimization targets**

### Reflection Overhead Summary

Total reflection overhead (end + begin_custom_deser + begin_nth_field + fill_defaults):
- **7,778,509 instructions (6.3% of total)**

This is the primary target for TypePlan optimization.

## Expected Impact

| Operation | Current | Expected |
|-----------|---------|----------|
| Struct field lookup | O(n) | O(1) or O(log n) |
| Proxy resolution | O(m) | O(1) |
| Field name lookup (deferred) | O(n) | O(1) |
| Type dispatch | O(checks) | O(1) enum match |

Target: **Reduce reflection overhead by 30-50%**

## Risks and Mitigations

1. **Memory overhead**: TypePlan trees take memory
   - Mitigation: Lazy construction, caching, Arc sharing

2. **Complexity**: More code paths to maintain
   - Mitigation: Clear separation, comprehensive tests

3. **Recursive types**: Infinite loops during plan construction
   - Mitigation: Track visited shapes, use weak references

## Alternatives Considered

1. **JIT compilation**: More aggressive but higher complexity
2. **Compile-time generation**: Best performance but breaks incremental compilation
3. **No change**: Accept current overhead

## References

- Issue #1951: Original proposal
- PR #1950: Recent allocation optimizations (reduced 5.9M → 3.9K allocations)
