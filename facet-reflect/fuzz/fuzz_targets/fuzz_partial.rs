#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use facet::Facet;
use facet_html_dom::{GlobalAttrs, Html};
use facet_reflect::{Partial, Resolution};
use facet_value::Value;
use libfuzzer_sys::fuzz_target;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// Target types
#[derive(Facet, Debug)]
struct FuzzTarget {
    name: String,
    count: u32,
    nested: NestedStruct,
    items: Vec<String>,
    mapping: HashMap<String, u32>,
    maybe: Option<String>,
    tags: HashSet<String>,
    boxed: Box<i32>,
    shared: Arc<String>,
}

#[derive(Facet, Debug)]
struct NestedStruct {
    x: i32,
    y: i32,
    label: String,
}

// Target with flattened struct - critical for reproducing HTML-style bugs
#[derive(Facet, Debug, Default)]
struct FlattenTarget {
    name: String,
    #[facet(flatten)]
    attrs: FlattenedAttrs,
    maybe_nested: Option<NestedStruct>,
}

// Separate target for Result testing
#[derive(Facet, Debug)]
struct ResultTarget {
    result_field: Result<String, String>,
}

// Enum target for variant operations
#[derive(Facet, Debug)]
#[repr(u8)]
enum FuzzEnum {
    Unit,
    Tuple(String, u32),
    Struct { name: String, value: Option<i32> },
}

// Deeply nested type to stress cleanup paths
#[derive(Facet, Debug, Default)]
struct DeepNested {
    level1: Option<Level1>,
}

// Smart pointer types
#[derive(Facet, Debug)]
struct SmartPtrTarget {
    arc_string: Arc<String>,
    arc_slice: Arc<[u8]>,
    box_option: Box<Option<String>>,
    arc_nested: Arc<NestedStruct>,
}

#[derive(Facet, Debug, Default)]
struct Level1 {
    #[facet(flatten)]
    attrs: FlattenedAttrs,
    level2: Option<Level2>,
}

#[derive(Facet, Debug, Default)]
struct Level2 {
    value: Option<String>,
    items: Vec<Option<String>>,
    nested_map: HashMap<String, Option<u32>>,
}

#[derive(Facet, Debug, Default)]
struct FlattenedAttrs {
    id: Option<String>,
    class: Option<String>,
    title: Option<String>,
    style: Option<String>,
    #[facet(flatten, default)]
    extra: HashMap<String, String>,
}

// Operations we can perform
#[derive(Arbitrary, Debug, Clone)]
enum PartialOp {
    BeginField(FieldChoice),
    BeginNthField(u8),
    SetU32(u32),
    SetI32(i32),
    SetI64(i64),
    SetF64(f64),
    SetBool(bool),
    SetString(SmallString),
    End,
    InitList,
    BeginListItem,
    InitMap,
    BeginKey,
    BeginValue,
    InitSet,
    BeginSetItem,
    BeginSome,
    BeginInner,
    BeginSmartPtr,
    SetDefault,
    SetNthFieldToDefault(u8),
    BeginDeferred,
    FinishDeferred,
    Build,
    // DynamicValue-specific operations
    BeginObjectEntry(SmallString),
    // Explicitly abandon/drop the Partial mid-construction
    Drop,
    // Result operations
    BeginOk,
    BeginErr,
    // Enum operations
    SelectNthVariant(u8),
    SelectVariantNamed(SmallString),
    // Array operations
    InitArray,
    // Parsing operations
    ParseFromStr(SmallString),
    // Custom deserialization
    BeginCustomDeserialization,
}

#[derive(Arbitrary, Debug, Clone)]
enum FieldChoice {
    Name,
    Count,
    Nested,
    Items,
    Mapping,
    Maybe,
    Tags,
    Boxed,
    Shared,
    X,
    Y,
    Label,
    // FlattenTarget fields
    Attrs,
    MaybeNested,
    ResultField,
    // FlattenedAttrs fields
    Id,
    Class,
    Title,
    Style,
    Extra,
    Invalid,
}

impl FieldChoice {
    fn as_str(&self) -> &'static str {
        match self {
            FieldChoice::Name => "name",
            FieldChoice::Count => "count",
            FieldChoice::Nested => "nested",
            FieldChoice::Items => "items",
            FieldChoice::Mapping => "mapping",
            FieldChoice::Maybe => "maybe",
            FieldChoice::Tags => "tags",
            FieldChoice::Boxed => "boxed",
            FieldChoice::Shared => "shared",
            FieldChoice::X => "x",
            FieldChoice::Y => "y",
            FieldChoice::Label => "label",
            FieldChoice::Attrs => "attrs",
            FieldChoice::MaybeNested => "maybe_nested",
            FieldChoice::ResultField => "result_field",
            FieldChoice::Id => "id",
            FieldChoice::Class => "class",
            FieldChoice::Title => "title",
            FieldChoice::Style => "style",
            FieldChoice::Extra => "extra",
            FieldChoice::Invalid => "nonexistent_field",
        }
    }
}

// Small string to avoid huge allocations
#[derive(Debug, Clone)]
struct SmallString(String);

impl<'a> Arbitrary<'a> for SmallString {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let len = u.int_in_range(0..=20)?;
        let bytes: Vec<u8> = (0..len)
            .map(|_| u.int_in_range(b'a'..=b'z'))
            .collect::<Result<_, _>>()?;
        Ok(SmallString(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

/// Applies an operation to the Partial, returning the new Partial on success
/// or None on error (which means the Partial was consumed).
fn apply_op<'a>(partial: Partial<'a>, op: &PartialOp) -> Option<Partial<'a>> {
    match op {
        PartialOp::BeginField(field) => partial.begin_field(field.as_str()).ok(),
        PartialOp::BeginNthField(idx) => partial.begin_nth_field(*idx as usize).ok(),
        PartialOp::SetU32(v) => partial.set(*v).ok(),
        PartialOp::SetI32(v) => partial.set(*v).ok(),
        PartialOp::SetI64(v) => partial.set(*v).ok(),
        PartialOp::SetF64(v) => partial.set(*v).ok(),
        PartialOp::SetBool(v) => partial.set(*v).ok(),
        PartialOp::SetString(s) => partial.set(s.0.clone()).ok(),
        PartialOp::End => partial.end().ok(),
        PartialOp::InitList => partial.init_list().ok(),
        PartialOp::BeginListItem => partial.begin_list_item().ok(),
        PartialOp::InitMap => partial.init_map().ok(),
        PartialOp::BeginKey => partial.begin_key().ok(),
        PartialOp::BeginValue => partial.begin_value().ok(),
        PartialOp::InitSet => partial.init_set().ok(),
        PartialOp::BeginSetItem => partial.begin_set_item().ok(),
        PartialOp::BeginSome => partial.begin_some().ok(),
        PartialOp::BeginInner => partial.begin_inner().ok(),
        PartialOp::BeginSmartPtr => partial.begin_smart_ptr().ok(),
        PartialOp::SetDefault => partial.set_default().ok(),
        PartialOp::SetNthFieldToDefault(idx) => {
            partial.set_nth_field_to_default(*idx as usize).ok()
        }
        PartialOp::BeginDeferred => partial.begin_deferred().ok(),
        PartialOp::FinishDeferred => partial.finish_deferred().ok(),
        PartialOp::Build => {
            // Build consumes the partial and returns a HeapValue
            // We return None because after build, the partial is gone
            let _ = partial.build();
            None
        }
        PartialOp::BeginObjectEntry(key) => partial.begin_object_entry(&key.0).ok(),
        PartialOp::Drop => {
            // Explicitly drop the Partial mid-construction - tests cleanup paths
            drop(partial);
            None
        }
        PartialOp::BeginOk => partial.begin_ok().ok(),
        PartialOp::BeginErr => partial.begin_err().ok(),
        PartialOp::SelectNthVariant(idx) => partial.select_nth_variant(*idx as usize).ok(),
        PartialOp::SelectVariantNamed(name) => partial.select_variant_named(&name.0).ok(),
        PartialOp::InitArray => partial.init_array().ok(),
        PartialOp::ParseFromStr(s) => partial.parse_from_str(&s.0).ok(),
        PartialOp::BeginCustomDeserialization => partial.begin_custom_deserialization().ok(),
    }
}

/// Which target type to fuzz
#[derive(Arbitrary, Debug, Clone)]
enum TargetType {
    FuzzTarget,
    DynamicValue,
    FlattenTarget,
    ResultTarget,
    // Real HTML types that trigger the bug
    HtmlType,
    GlobalAttrsType,
    // Enum target
    EnumType,
    // Deep nested type
    DeepNestedType,
    // Smart pointer types
    SmartPtrType,
}

fuzz_target!(|input: (TargetType, Vec<PartialOp>)| {
    let (target_type, ops) = input;

    // Limit sequence length to avoid timeouts
    let ops = if ops.len() > 100 { &ops[..100] } else { &ops };

    match target_type {
        TargetType::FuzzTarget => {
            if let Ok(partial) = Partial::alloc::<FuzzTarget>() {
                let mut partial = Some(partial);
                for op in ops {
                    if let Some(p) = partial.take() {
                        partial = apply_op(p, op);
                    } else {
                        // Partial was consumed by an error or build, stop
                        break;
                    }
                }
                // Partial is dropped here (if Some) - must not leak or crash
            }
        }
        TargetType::DynamicValue => {
            if let Ok(partial) = Partial::alloc::<Value>() {
                let mut partial = Some(partial);
                for op in ops {
                    if let Some(p) = partial.take() {
                        partial = apply_op(p, op);
                    } else {
                        // Partial was consumed by an error or build, stop
                        break;
                    }
                }
                // Partial is dropped here (if Some) - must not leak or crash
            }
        }
        TargetType::FlattenTarget => {
            if let Ok(partial) = Partial::alloc::<FlattenTarget>() {
                let mut partial = Some(partial);
                for op in ops {
                    if let Some(p) = partial.take() {
                        partial = apply_op(p, op);
                    } else {
                        break;
                    }
                }
            }
        }
        TargetType::ResultTarget => {
            if let Ok(partial) = Partial::alloc::<ResultTarget>() {
                let mut partial = Some(partial);
                for op in ops {
                    if let Some(p) = partial.take() {
                        partial = apply_op(p, op);
                    } else {
                        break;
                    }
                }
            }
        }
        TargetType::HtmlType => {
            if let Ok(partial) = Partial::alloc::<Html>() {
                let mut partial = Some(partial);
                for op in ops {
                    if let Some(p) = partial.take() {
                        partial = apply_op(p, op);
                    } else {
                        break;
                    }
                }
            }
        }
        TargetType::GlobalAttrsType => {
            if let Ok(partial) = Partial::alloc::<GlobalAttrs>() {
                let mut partial = Some(partial);
                for op in ops {
                    if let Some(p) = partial.take() {
                        partial = apply_op(p, op);
                    } else {
                        break;
                    }
                }
            }
        }
        TargetType::EnumType => {
            if let Ok(partial) = Partial::alloc::<FuzzEnum>() {
                let mut partial = Some(partial);
                for op in ops {
                    if let Some(p) = partial.take() {
                        partial = apply_op(p, op);
                    } else {
                        break;
                    }
                }
            }
        }
        TargetType::DeepNestedType => {
            if let Ok(partial) = Partial::alloc::<DeepNested>() {
                let mut partial = Some(partial);
                for op in ops {
                    if let Some(p) = partial.take() {
                        partial = apply_op(p, op);
                    } else {
                        break;
                    }
                }
            }
        }
        TargetType::SmartPtrType => {
            if let Ok(partial) = Partial::alloc::<SmartPtrTarget>() {
                let mut partial = Some(partial);
                for op in ops {
                    if let Some(p) = partial.take() {
                        partial = apply_op(p, op);
                    } else {
                        break;
                    }
                }
            }
        }
    }
});
