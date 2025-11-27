#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use facet::Facet;
use facet_reflect::{Partial, Resolution};
use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;

// Target types
#[derive(Facet, Debug)]
struct FuzzTarget {
    name: String,
    count: u32,
    nested: NestedStruct,
    items: Vec<String>,
    mapping: HashMap<String, u32>,
    maybe: Option<String>,
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
    SetString(SmallString),
    End,
    BeginList,
    BeginListItem,
    BeginMap,
    BeginKey,
    BeginValue,
    BeginSome,
    BeginInner,
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
        PartialOp::BeginSome => {
            let _ = partial.begin_some();
        }
        PartialOp::BeginInner => {
            let _ = partial.begin_inner();
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

fuzz_target!(|ops: Vec<PartialOp>| {
    // Limit sequence length to avoid timeouts
    let ops = if ops.len() > 100 { &ops[..100] } else { &ops };

    if let Ok(mut typed_partial) = Partial::alloc::<FuzzTarget>() {
        let partial = typed_partial.inner_mut();
        for op in ops {
            apply_op(partial, op);
        }
        // Partial is dropped here - must not leak or crash
    }
});
