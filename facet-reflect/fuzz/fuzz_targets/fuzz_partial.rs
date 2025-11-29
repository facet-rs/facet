#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use facet::Facet;
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
    BeginList,
    BeginListItem,
    BeginMap,
    BeginKey,
    BeginValue,
    BeginSet,
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
fn apply_op(partial: Partial<'_>, op: &PartialOp) -> Option<Partial<'_>> {
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
        PartialOp::BeginList => partial.begin_list().ok(),
        PartialOp::BeginListItem => partial.begin_list_item().ok(),
        PartialOp::BeginMap => partial.begin_map().ok(),
        PartialOp::BeginKey => partial.begin_key().ok(),
        PartialOp::BeginValue => partial.begin_value().ok(),
        PartialOp::BeginSet => partial.begin_set().ok(),
        PartialOp::BeginSetItem => partial.begin_set_item().ok(),
        PartialOp::BeginSome => partial.begin_some().ok(),
        PartialOp::BeginInner => partial.begin_inner().ok(),
        PartialOp::BeginSmartPtr => partial.begin_smart_ptr().ok(),
        PartialOp::SetDefault => partial.set_default().ok(),
        PartialOp::SetNthFieldToDefault(idx) => {
            partial.set_nth_field_to_default(*idx as usize).ok()
        }
        PartialOp::BeginDeferred => {
            let resolution = Resolution::new();
            partial.begin_deferred(resolution).ok()
        }
        PartialOp::FinishDeferred => partial.finish_deferred().ok(),
        PartialOp::Build => {
            // Build consumes the partial and returns a HeapValue
            // We return None because after build, the partial is gone
            let _ = partial.build();
            None
        }
        PartialOp::BeginObjectEntry(key) => partial.begin_object_entry(&key.0).ok(),
    }
}

/// Which target type to fuzz
#[derive(Arbitrary, Debug, Clone)]
enum TargetType {
    FuzzTarget,
    DynamicValue,
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
    }
});
