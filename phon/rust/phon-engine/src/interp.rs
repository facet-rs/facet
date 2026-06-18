//! The IR interpreter: run a lowered [`Program`] against a reader to produce a
//! [`Value`]. The reference semantics a JIT must match exactly
//! (`r[exec.interpreter-baseline]`).
//!
//! This is a small stack machine. Leaf ops decode from the wire and push a
//! value; container ops pop their children's values and push the assembled one.
//! The invariant from `phon_ir::ir` holds throughout: running a complete lowered
//! subtree nets exactly one value on the stack, so the whole program leaves a
//! single result.
//!
//! It is the flat counterpart to `plan::exec` (which walks the `Node` tree
//! recursively). The two must agree value-for-value; the planner's compat tests
//! assert exactly that, with `exec` as the oracle.
//!
//! Spec: `r[exec.interpreter-baseline]`, `r[ir.total]`.

use std::collections::HashSet;

use facet_value::{VArray, VObject, VString, Value};
use phon_ir::ir::{Op, Program, ValueProgram};
use phon_schema::bytes::Reader;
use phon_schema::{DecodeError, read_value};

use crate::compact::{self, CompactError, Registry};

type Result<T> = core::result::Result<T, CompactError>;

/// Run a lowered program against `bytes`, producing the decoded value and
/// rejecting trailing bytes.
///
/// # Errors
/// [`CompactError`] for malformed input or a writer-only enum variant.
// r[impl exec.interpreter-baseline]
// r[impl ir.total]
pub fn run(program: &Program, bytes: &[u8], reg: &Registry) -> Result<Value> {
    run_program(program, bytes, reg, &Default::default())
}

/// Run a lowered dynamic-value program with its recursive block registry.
///
/// # Errors
/// [`CompactError`] for malformed input, missing recursion blocks, or a
/// writer-only enum variant.
pub fn run_lowered(lowered: &ValueProgram, bytes: &[u8], reg: &Registry) -> Result<Value> {
    run_program(&lowered.program, bytes, reg, &lowered.blocks)
}

fn run_program(
    program: &Program,
    bytes: &[u8],
    reg: &Registry,
    blocks: &std::collections::BTreeMap<phon_schema::SchemaId, Program>,
) -> Result<Value> {
    let mut r = Reader::new(bytes);
    let mut stack: Vec<Value> = Vec::new();
    exec_ops(program, &mut r, reg, blocks, &mut stack)?;
    if r.remaining() != 0 {
        return Err(CompactError::Decode(DecodeError::TrailingBytes(
            r.remaining(),
        )));
    }
    stack
        .pop()
        .ok_or(CompactError::Decode(DecodeError::Malformed(
            "program produced no value",
        )))
}

fn exec_ops(
    ops: &[Op],
    r: &mut Reader,
    reg: &Registry,
    blocks: &std::collections::BTreeMap<phon_schema::SchemaId, Program>,
    stack: &mut Vec<Value>,
) -> Result<()> {
    for op in ops {
        exec_op(op, r, reg, blocks, stack)?;
    }
    Ok(())
}

fn exec_op(
    op: &Op,
    r: &mut Reader,
    reg: &Registry,
    blocks: &std::collections::BTreeMap<phon_schema::SchemaId, Program>,
    stack: &mut Vec<Value>,
) -> Result<()> {
    match op {
        Op::Scalar(p) => stack.push(compact::decode_primitive(r, *p)?),
        Op::Dynamic => stack.push(read_value(r)?),
        Op::CallBlock { schema } => {
            let block = blocks
                .get(schema)
                .ok_or(CompactError::Decode(DecodeError::Malformed(
                    "missing recursion block",
                )))?;
            exec_ops(block, r, reg, blocks, stack)?;
        }
        Op::Null => stack.push(Value::NULL),
        Op::Skip(writer_ref) => {
            // Walk the writer-only field by its own schema and drop it.
            compact::decode_ref(r, writer_ref, reg, 0)?;
        }
        Op::Object { keys } => {
            let vals = stack.split_off(stack.len() - keys.len());
            let mut obj = VObject::new();
            for (k, v) in keys.iter().zip(vals) {
                obj.insert(VString::new(k), v);
            }
            stack.push(obj.into());
        }
        Op::Array { count } => {
            let vals = stack.split_off(stack.len() - count);
            let mut arr = VArray::new();
            for v in vals {
                arr.push(v);
            }
            stack.push(arr.into());
        }
        Op::Seq {
            set,
            min_wire,
            body,
        } => {
            let n = r.read_len(*min_wire)?;
            let mut arr = VArray::new();
            let mut seen = if *set { Some(HashSet::new()) } else { None };
            for _ in 0..n {
                exec_ops(body, r, reg, blocks, stack)?;
                let v = stack.pop().expect("seq body nets one value");
                if let Some(s) = &mut seen
                    && !s.insert(v.clone())
                {
                    return Err(CompactError::Decode(DecodeError::DuplicateElement));
                }
                arr.push(v);
            }
            stack.push(arr.into());
        }
        Op::Map { key, value } => {
            let n = r.read_len(1)?;
            let mut obj = VObject::new();
            for _ in 0..n {
                exec_ops(key, r, reg, blocks, stack)?;
                let k = stack.pop().expect("map key nets one value");
                exec_ops(value, r, reg, blocks, stack)?;
                let v = stack.pop().expect("map value nets one value");
                let ks = k
                    .as_string()
                    .ok_or(CompactError::Unsupported("map with non-string keys"))?;
                if obj.insert(VString::new(ks.as_str()), v).is_some() {
                    return Err(CompactError::Decode(DecodeError::DuplicateKey));
                }
            }
            stack.push(obj.into());
        }
        Op::FixedArray {
            dimensions,
            min_wire,
            body,
        } => {
            let count = compact::product(dimensions)?;
            compact::check_fixed_count(count, *min_wire, r.remaining())?;
            let mut arr = VArray::new();
            for _ in 0..count {
                exec_ops(body, r, reg, blocks, stack)?;
                arr.push(stack.pop().expect("array body nets one value"));
            }
            stack.push(arr.into());
        }
        Op::Option { some } => match r.read_u8()? {
            0 => stack.push(Value::NULL),
            1 => exec_ops(some, r, reg, blocks, stack)?,
            b => return Err(CompactError::Decode(DecodeError::InvalidBool(b))),
        },
        Op::Enum { arms } => {
            let idx = r.read_u32()?;
            let arm = arms
                .iter()
                .find(|a| a.writer_index == idx)
                .ok_or(CompactError::WriterOnlyVariant(idx))?;
            exec_ops(&arm.payload, r, reg, blocks, stack)?;
            let payload = stack.pop().expect("variant payload nets one value");
            let mut obj = VObject::new();
            obj.insert(VString::new(&arm.reader_name), payload);
            stack.push(obj.into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use phon_schema::{Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef, primitive_id};

    fn prim(p: Primitive) -> SchemaRef {
        SchemaRef::concrete(primitive_id(p))
    }

    fn schema(id: u64, kind: SchemaKind) -> Schema {
        Schema {
            id: SchemaId(id),
            type_params: Vec::new(),
            kind,
        }
    }

    /// Same-schema roundtrip (`writer == reader`): encode with the compact codec,
    /// decode through the lowered IR, and check the value survives and that the
    /// IR agrees with the recursive compact decoder.
    fn rt_ir(value: Value, root: SchemaId, reg: &Registry) {
        let bytes = compact::to_bytes(&value, root, reg).unwrap();
        let got = crate::plan::decode_via_ir(&bytes, root, root, reg).unwrap();
        assert_eq!(got, value, "IR roundtrip changed the value");
        assert_eq!(
            got,
            compact::from_bytes(&bytes, root, reg).unwrap(),
            "IR disagreed with the recursive compact decoder"
        );
    }

    // r[verify exec.interpreter-baseline]
    // r[verify ir.total]
    #[test]
    fn ir_roundtrips_containers() {
        let reg = Registry::new([
            schema(
                1,
                SchemaKind::List {
                    element: prim(Primitive::U64),
                },
            ),
            schema(
                2,
                SchemaKind::Set {
                    element: prim(Primitive::U32),
                },
            ),
            schema(
                3,
                SchemaKind::Map {
                    key: prim(Primitive::String),
                    value: prim(Primitive::U32),
                },
            ),
            schema(
                4,
                SchemaKind::Option {
                    element: prim(Primitive::U32),
                },
            ),
            schema(
                5,
                SchemaKind::Array {
                    element: prim(Primitive::U16),
                    dimensions: vec![3],
                },
            ),
            schema(
                6,
                SchemaKind::Tuple {
                    elements: vec![prim(Primitive::U8), prim(Primitive::String)],
                },
            ),
            schema(7, SchemaKind::Dynamic),
            schema(
                8,
                SchemaKind::Struct {
                    name: "Holder".to_string(),
                    fields: vec![
                        Field {
                            name: "items".to_string(),
                            schema: SchemaRef::concrete(SchemaId(1)),
                            required: true,
                        },
                        Field {
                            name: "label".to_string(),
                            schema: prim(Primitive::String),
                            required: true,
                        },
                    ],
                },
            ),
        ]);

        // list<u64> — Op::Seq
        let mut list = VArray::new();
        list.push(Value::from(1u64));
        list.push(Value::from(2u64));
        list.push(Value::from(3u64));
        rt_ir(Value::from(list), SchemaId(1), &reg);

        // set<u32> — Op::Seq { set: true }
        let mut set = VArray::new();
        set.push(Value::from(10u32));
        set.push(Value::from(20u32));
        rt_ir(Value::from(set), SchemaId(2), &reg);

        // map<string,u32> — Op::Map
        let mut map = VObject::new();
        map.insert(VString::new("a"), Value::from(1u32));
        map.insert(VString::new("b"), Value::from(2u32));
        rt_ir(Value::from(map), SchemaId(3), &reg);

        // option<u32> — Op::Option, both arms
        rt_ir(Value::from(42u32), SchemaId(4), &reg);
        rt_ir(Value::NULL, SchemaId(4), &reg);

        // array<u16, 3> — Op::FixedArray
        let mut arr = VArray::new();
        arr.push(Value::from(7u16));
        arr.push(Value::from(8u16));
        arr.push(Value::from(9u16));
        rt_ir(Value::from(arr), SchemaId(5), &reg);

        // tuple(u8, string) — Op::Array over inline heterogeneous elements
        let mut tup = VArray::new();
        tup.push(Value::from(5u8));
        tup.push(Value::from(VString::new("hi")));
        rt_ir(Value::from(tup), SchemaId(6), &reg);

        // dynamic — Op::Dynamic
        rt_ir(Value::from("free-form"), SchemaId(7), &reg);

        // struct holding a list — Op::Object with a nested Op::Seq child
        let mut items = VArray::new();
        items.push(Value::from(100u64));
        items.push(Value::from(200u64));
        let mut holder = VObject::new();
        holder.insert(VString::new("items"), Value::from(items));
        holder.insert(VString::new("label"), Value::from(VString::new("L")));
        rt_ir(Value::from(holder), SchemaId(8), &reg);
    }
}
