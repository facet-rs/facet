#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use facet_value::{VArray, VObject, Value};
use libfuzzer_sys::fuzz_target;

/// Operations on VArray
#[derive(Arbitrary, Debug, Clone)]
enum ArrayOp {
    Push(ValueChoice),
    Pop,
    Insert(u8, ValueChoice),
    Remove(u8),
    SwapRemove(u8),
    Clear,
    Truncate(u8),
    Clone,
    Get(u8),
    ShrinkToFit,
}

/// Operations on VObject
#[derive(Arbitrary, Debug, Clone)]
enum ObjectOp {
    Insert(SmallString, ValueChoice),
    Remove(SmallString),
    Clear,
    Clone,
    Get(SmallString),
}

/// Operations on Value (top-level)
#[derive(Arbitrary, Debug, Clone)]
enum ValueOp {
    // Type checks
    CheckType,
    // Conversions
    AsArray,
    AsObject,
    AsString,
    AsNumber,
    AsBool,
    // Array ops (if it's an array)
    ArrayOp(ArrayOp),
    // Object ops (if it's an object)
    ObjectOp(ObjectOp),
    // Clone the value
    Clone,
    // Replace with new value
    Replace(ValueChoice),
    // Drop and recreate
    DropAndRecreate(ValueChoice),
}

/// What kind of value to create
#[derive(Arbitrary, Debug, Clone)]
enum ValueChoice {
    Null,
    Bool(bool),
    I64(i64),
    F64(f64),
    String(SmallString),
    EmptyArray,
    EmptyObject,
    // Nested structures - these are the tricky ones!
    ArrayWithValues(u8),  // Create array with N simple values
    ObjectWithValues(u8), // Create object with N simple values
    NestedArray(u8),      // Array containing arrays
    NestedObject(u8),     // Object containing objects
    DeeplyNested(u8),     // Alternating array/object nesting
}

/// Small string to avoid huge allocations
#[derive(Debug, Clone)]
struct SmallString(String);

impl<'a> Arbitrary<'a> for SmallString {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let len = u.int_in_range(0..=16)?;
        let bytes: Vec<u8> = (0..len)
            .map(|_| u.int_in_range(b'a'..=b'z'))
            .collect::<Result<_, _>>()?;
        Ok(SmallString(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

fn create_value(choice: &ValueChoice, depth: u8) -> Value {
    // Limit recursion depth
    if depth > 5 {
        return Value::NULL;
    }

    match choice {
        ValueChoice::Null => Value::NULL,
        ValueChoice::Bool(b) => Value::from(*b),
        ValueChoice::I64(n) => Value::from(*n),
        ValueChoice::F64(n) => Value::from(*n),
        ValueChoice::String(s) => Value::from(s.0.as_str()),
        ValueChoice::EmptyArray => {
            let arr = VArray::new();
            arr.into()
        }
        ValueChoice::EmptyObject => {
            let obj = VObject::new();
            obj.into()
        }
        ValueChoice::ArrayWithValues(n) => {
            let mut arr = VArray::new();
            let count = (*n as usize).min(10);
            for i in 0..count {
                arr.push(Value::from(i as i64));
            }
            arr.into()
        }
        ValueChoice::ObjectWithValues(n) => {
            let mut obj = VObject::new();
            let count = (*n as usize).min(10);
            for i in 0..count {
                let key = format!("key{}", i);
                obj.insert(&key, Value::from(i as i64));
            }
            obj.into()
        }
        ValueChoice::NestedArray(n) => {
            let mut arr = VArray::new();
            let count = (*n as usize).min(5);
            for _ in 0..count {
                let inner = VArray::new();
                arr.push(Value::from(inner));
            }
            arr.into()
        }
        ValueChoice::NestedObject(n) => {
            let mut obj = VObject::new();
            let count = (*n as usize).min(5);
            for i in 0..count {
                let key = format!("nested{}", i);
                let inner = VObject::new();
                obj.insert(&key, Value::from(inner));
            }
            obj.into()
        }
        ValueChoice::DeeplyNested(n) => {
            let levels = (*n as usize).min(5);
            let mut val = Value::from(42i64);
            for i in 0..levels {
                if i % 2 == 0 {
                    let mut arr = VArray::new();
                    arr.push(val);
                    val = arr.into();
                } else {
                    let mut obj = VObject::new();
                    obj.insert("inner", val);
                    val = obj.into();
                }
            }
            val
        }
    }
}

fn apply_array_op(arr: &mut VArray, op: &ArrayOp) {
    match op {
        ArrayOp::Push(v) => {
            arr.push(create_value(v, 0));
        }
        ArrayOp::Pop => {
            let _ = arr.pop();
        }
        ArrayOp::Insert(idx, v) => {
            let idx = (*idx as usize).min(arr.len());
            arr.insert(idx, create_value(v, 0));
        }
        ArrayOp::Remove(idx) => {
            let _ = arr.remove(*idx as usize);
        }
        ArrayOp::SwapRemove(idx) => {
            let _ = arr.swap_remove(*idx as usize);
        }
        ArrayOp::Clear => {
            arr.clear();
        }
        ArrayOp::Truncate(len) => {
            arr.truncate(*len as usize);
        }
        ArrayOp::Clone => {
            let _ = arr.clone();
        }
        ArrayOp::Get(idx) => {
            let _ = arr.get(*idx as usize);
        }
        ArrayOp::ShrinkToFit => {
            arr.shrink_to_fit();
        }
    }
}

fn apply_object_op(obj: &mut VObject, op: &ObjectOp) {
    match op {
        ObjectOp::Insert(key, v) => {
            obj.insert(&key.0, create_value(v, 0));
        }
        ObjectOp::Remove(key) => {
            let _ = obj.remove(&key.0);
        }
        ObjectOp::Clear => {
            obj.clear();
        }
        ObjectOp::Clone => {
            let _ = obj.clone();
        }
        ObjectOp::Get(key) => {
            let _ = obj.get(&key.0);
        }
    }
}

fn apply_value_op(val: &mut Value, op: &ValueOp) {
    match op {
        ValueOp::CheckType => {
            let _ = val.value_type();
            let _ = val.is_null();
            let _ = val.is_bool();
            let _ = val.is_number();
            let _ = val.is_string();
            let _ = val.is_array();
            let _ = val.is_object();
        }
        ValueOp::AsArray => {
            let _ = val.as_array();
        }
        ValueOp::AsObject => {
            let _ = val.as_object();
        }
        ValueOp::AsString => {
            let _ = val.as_string();
        }
        ValueOp::AsNumber => {
            let _ = val.as_number();
        }
        ValueOp::AsBool => {
            let _ = val.as_bool();
        }
        ValueOp::ArrayOp(array_op) => {
            if let Some(arr) = val.as_array_mut() {
                apply_array_op(arr, array_op);
            }
        }
        ValueOp::ObjectOp(object_op) => {
            if let Some(obj) = val.as_object_mut() {
                apply_object_op(obj, object_op);
            }
        }
        ValueOp::Clone => {
            let _ = val.clone();
        }
        ValueOp::Replace(choice) => {
            *val = create_value(choice, 0);
        }
        ValueOp::DropAndRecreate(choice) => {
            // Drop current value and create new one
            *val = Value::NULL;
            *val = create_value(choice, 0);
        }
    }
}

/// Test mode
#[derive(Arbitrary, Debug, Clone)]
enum TestMode {
    /// Fuzz a single Value with operations
    SingleValue(ValueChoice, Vec<ValueOp>),
    /// Fuzz VArray directly
    DirectArray(Vec<ArrayOp>),
    /// Fuzz VObject directly
    DirectObject(Vec<ObjectOp>),
    /// Create and immediately drop nested structures
    DropNested(ValueChoice),
    /// Clone nested structures
    CloneNested(ValueChoice),
    /// Multiple values with interleaved operations
    MultiValue(Vec<(ValueChoice, Vec<ValueOp>)>),
}

fuzz_target!(|mode: TestMode| {
    match mode {
        TestMode::SingleValue(choice, ops) => {
            let ops = if ops.len() > 100 {
                &ops[..100]
            } else {
                &ops[..]
            };
            let mut val = create_value(&choice, 0);
            for op in ops {
                apply_value_op(&mut val, op);
            }
            // val is dropped here
        }
        TestMode::DirectArray(ops) => {
            let ops = if ops.len() > 100 {
                &ops[..100]
            } else {
                &ops[..]
            };
            let mut arr = VArray::new();
            for op in ops {
                apply_array_op(&mut arr, op);
            }
            // arr is dropped here
        }
        TestMode::DirectObject(ops) => {
            let ops = if ops.len() > 100 {
                &ops[..100]
            } else {
                &ops[..]
            };
            let mut obj = VObject::new();
            for op in ops {
                apply_object_op(&mut obj, op);
            }
            // obj is dropped here
        }
        TestMode::DropNested(choice) => {
            // Just create and drop - tests drop implementation
            let _val = create_value(&choice, 0);
        }
        TestMode::CloneNested(choice) => {
            let val = create_value(&choice, 0);
            let _cloned = val.clone();
            // Both dropped here
        }
        TestMode::MultiValue(items) => {
            let items: Vec<_> = items.into_iter().take(10).collect();
            let mut values: Vec<Value> = items
                .iter()
                .map(|(choice, _)| create_value(choice, 0))
                .collect();

            for (i, (_, ops)) in items.iter().enumerate() {
                let ops = if ops.len() > 50 { &ops[..50] } else { &ops[..] };
                for op in ops {
                    apply_value_op(&mut values[i], op);
                }
            }
            // All values dropped here
        }
    }
});
