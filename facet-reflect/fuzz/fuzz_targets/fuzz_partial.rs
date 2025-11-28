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

fn apply_op(partial: &mut Partial<'_>, op: &PartialOp) {
    match op {
        PartialOp::BeginField(field) => {
            let _ = partial.begin_field(field.as_str());
        }
        PartialOp::BeginNthField(idx) => {
            let _ = partial.begin_nth_field(*idx as usize);
        }
        PartialOp::SetU32(v) => {
            let _ = partial.set(*v);
        }
        PartialOp::SetI32(v) => {
            let _ = partial.set(*v);
        }
        PartialOp::SetI64(v) => {
            let _ = partial.set(*v);
        }
        PartialOp::SetF64(v) => {
            let _ = partial.set(*v);
        }
        PartialOp::SetBool(v) => {
            let _ = partial.set(*v);
        }
        PartialOp::SetString(s) => {
            let _ = partial.set(s.0.clone());
        }
        PartialOp::End => {
            let _ = partial.end();
        }
        PartialOp::BeginList => {
            let _ = partial.begin_list();
        }
        PartialOp::BeginListItem => {
            let _ = partial.begin_list_item();
        }
        PartialOp::BeginMap => {
            let _ = partial.begin_map();
        }
        PartialOp::BeginKey => {
            let _ = partial.begin_key();
        }
        PartialOp::BeginValue => {
            let _ = partial.begin_value();
        }
        PartialOp::BeginSet => {
            let _ = partial.begin_set();
        }
        PartialOp::BeginSetItem => {
            let _ = partial.begin_set_item();
        }
        PartialOp::BeginSome => {
            let _ = partial.begin_some();
        }
        PartialOp::BeginInner => {
            let _ = partial.begin_inner();
        }
        PartialOp::BeginSmartPtr => {
            let _ = partial.begin_smart_ptr();
        }
        PartialOp::SetDefault => {
            let _ = partial.set_default();
        }
        PartialOp::SetNthFieldToDefault(idx) => {
            let _ = partial.set_nth_field_to_default(*idx as usize);
        }
        PartialOp::BeginDeferred => {
            let resolution = Resolution::new();
            let _ = partial.begin_deferred(resolution);
        }
        PartialOp::FinishDeferred => {
            let _ = partial.finish_deferred();
        }
        PartialOp::Build => {
            let _ = partial.build();
        }
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
            if let Ok(mut typed_partial) = Partial::alloc::<FuzzTarget>() {
                let partial = typed_partial.inner_mut();
                for op in ops {
                    apply_op(partial, op);
                }
                // Partial is dropped here - must not leak or crash
            }
        }
        TargetType::DynamicValue => {
            if let Ok(mut typed_partial) = Partial::alloc::<Value>() {
                let partial = typed_partial.inner_mut();
                for op in ops {
                    apply_op(partial, op);
                }
                // Partial is dropped here - must not leak or crash
            }
        }
    }
});
