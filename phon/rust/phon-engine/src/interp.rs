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

use std::collections::{BTreeMap, HashSet};

use facet_value::{VArray, VObject, VString, Value};
use phon_ir::ir::{Op, Program, ValueProgram};
use phon_schema::SchemaId;
use phon_schema::bytes::Reader;
use phon_schema::{DecodeError, read_value};
use weavy::{Control, RunError, RunStats, Step};

use crate::compact::{self, CompactError, Registry};

type Result<T> = core::result::Result<T, CompactError>;

/// Decoded value plus the generic Weavy runner counters that produced it.
///
/// These counters are diagnostics only. The default [`run`] and [`run_lowered`]
/// paths do not collect them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunReport {
    pub value: Value,
    pub stats: RunStats,
}

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

/// Run a lowered program and return generic Weavy runner counters.
///
/// # Errors
/// [`CompactError`] for malformed input or a writer-only enum variant.
pub fn run_with_stats(program: &Program, bytes: &[u8], reg: &Registry) -> Result<RunReport> {
    run_program_with_stats(program, bytes, reg, &Default::default())
}

/// Run a lowered dynamic-value program with its recursive block registry.
///
/// # Errors
/// [`CompactError`] for malformed input, missing recursion blocks, or a
/// writer-only enum variant.
pub fn run_lowered(lowered: &ValueProgram, bytes: &[u8], reg: &Registry) -> Result<Value> {
    run_program(&lowered.program, bytes, reg, &lowered.blocks)
}

/// Run a lowered dynamic-value program and return generic Weavy runner counters.
///
/// # Errors
/// [`CompactError`] for malformed input, missing recursion blocks, or a
/// writer-only enum variant.
pub fn run_lowered_with_stats(
    lowered: &ValueProgram,
    bytes: &[u8],
    reg: &Registry,
) -> Result<RunReport> {
    run_program_with_stats(&lowered.program, bytes, reg, &lowered.blocks)
}

fn run_program(
    program: &Program,
    bytes: &[u8],
    reg: &Registry,
    blocks: &BTreeMap<SchemaId, Program>,
) -> Result<Value> {
    let mut interp = Interp {
        reader: Reader::new(bytes),
        reg,
        stack: Vec::new(),
    };
    weavy::run_program(program, blocks, &mut interp).map_err(|err| match err {
        RunError::Step(err) => err,
        RunError::MissingBlock(_) => {
            CompactError::Decode(DecodeError::Malformed("missing recursion block"))
        }
    })?;
    finish_interp(interp)
}

fn run_program_with_stats(
    program: &Program,
    bytes: &[u8],
    reg: &Registry,
    blocks: &BTreeMap<SchemaId, Program>,
) -> Result<RunReport> {
    let mut interp = Interp {
        reader: Reader::new(bytes),
        reg,
        stack: Vec::new(),
    };
    let stats =
        weavy::run_program_with_stats(program, blocks, &mut interp).map_err(|err| match err {
            RunError::Step(err) => err,
            RunError::MissingBlock(_) => {
                CompactError::Decode(DecodeError::Malformed("missing recursion block"))
            }
        })?;
    Ok(RunReport {
        value: finish_interp(interp)?,
        stats,
    })
}

fn finish_interp(mut interp: Interp<'_, '_>) -> Result<Value> {
    if interp.reader.remaining() != 0 {
        return Err(CompactError::Decode(DecodeError::TrailingBytes(
            interp.reader.remaining(),
        )));
    }
    interp
        .stack
        .pop()
        .ok_or(CompactError::Decode(DecodeError::Malformed(
            "program produced no value",
        )))
}

struct Interp<'bytes, 'reg> {
    reader: Reader<'bytes>,
    reg: &'reg Registry,
    stack: Vec<Value>,
}

enum Continuation<'program> {
    Seq {
        remaining: usize,
        set: bool,
        body: &'program Program,
        values: VArray,
        seen: Option<HashSet<Value>>,
    },
    FixedArray {
        remaining: u64,
        body: &'program Program,
        values: VArray,
    },
    MapKey {
        remaining: usize,
        key: &'program Program,
        value: &'program Program,
        object: VObject,
    },
    MapValue {
        remaining: usize,
        key: &'program Program,
        value: &'program Program,
        object: VObject,
        pending_key: VString,
    },
    EnumPayload {
        reader_name: &'program str,
    },
}

impl<'program> Step<'program, SchemaId, Op> for Interp<'_, '_> {
    type Error = CompactError;
    type Continuation = Continuation<'program>;

    fn step(
        &mut self,
        op: &'program Op,
    ) -> Result<Control<'program, SchemaId, Op, Self::Continuation>> {
        Ok(match op {
            Op::Scalar(p) => {
                self.stack
                    .push(compact::decode_primitive(&mut self.reader, *p)?);
                Control::Continue
            }
            Op::Dynamic => {
                self.stack.push(read_value(&mut self.reader)?);
                Control::Continue
            }
            Op::CallBlock { schema } => Control::CallBlock(*schema),
            Op::Null => {
                self.stack.push(Value::NULL);
                Control::Continue
            }
            Op::Skip(writer_ref) => {
                // Walk the writer-only field by its own schema and drop it.
                compact::decode_ref(&mut self.reader, writer_ref, self.reg, 0)?;
                Control::Continue
            }
            Op::Object { keys } => {
                let vals = self.stack.split_off(self.stack.len() - keys.len());
                let mut obj = VObject::new();
                for (k, v) in keys.iter().zip(vals) {
                    obj.insert(VString::new(k), v);
                }
                self.stack.push(obj.into());
                Control::Continue
            }
            Op::Array { count } => {
                let vals = self.stack.split_off(self.stack.len() - count);
                let mut arr = VArray::new();
                for v in vals {
                    arr.push(v);
                }
                self.stack.push(arr.into());
                Control::Continue
            }
            Op::Seq {
                set,
                min_wire,
                body,
            } => {
                let remaining = self.reader.read_len(*min_wire)?;
                let continuation = Continuation::Seq {
                    remaining,
                    set: *set,
                    body,
                    values: VArray::new(),
                    seen: if *set { Some(HashSet::new()) } else { None },
                };
                self.call_repeated(body, continuation, remaining)
            }
            Op::Map { key, value } => {
                let remaining = self.reader.read_len(1)?;
                let continuation = Continuation::MapKey {
                    remaining,
                    key,
                    value,
                    object: VObject::new(),
                };
                self.call_repeated(key, continuation, remaining)
            }
            Op::FixedArray {
                dimensions,
                min_wire,
                body,
            } => {
                let count = compact::product(dimensions)?;
                compact::check_fixed_count(count, *min_wire, self.reader.remaining())?;
                let continuation = Continuation::FixedArray {
                    remaining: count,
                    body,
                    values: VArray::new(),
                };
                self.call_repeated(body, continuation, count)
            }
            Op::Option { some } => match self.reader.read_u8()? {
                0 => {
                    self.stack.push(Value::NULL);
                    Control::Continue
                }
                1 => Control::CallProgram(some),
                b => return Err(CompactError::Decode(DecodeError::InvalidBool(b))),
            },
            Op::Enum { arms } => {
                let idx = self.reader.read_u32()?;
                let arm = arms
                    .iter()
                    .find(|a| a.writer_index == idx)
                    .ok_or(CompactError::WriterOnlyVariant(idx))?;
                Control::CallProgramThen(
                    &arm.payload,
                    Continuation::EnumPayload {
                        reader_name: &arm.reader_name,
                    },
                )
            }
        })
    }

    fn after_return(
        &mut self,
        continuation: Self::Continuation,
    ) -> Result<Control<'program, SchemaId, Op, Self::Continuation>> {
        match continuation {
            Continuation::Seq {
                mut remaining,
                set,
                body,
                mut values,
                mut seen,
            } => {
                let v = self.pop_value("seq body produced no value")?;
                if let Some(s) = &mut seen
                    && !s.insert(v.clone())
                {
                    return Err(CompactError::Decode(DecodeError::DuplicateElement));
                }
                values.push(v);
                remaining -= 1;
                if remaining == 0 {
                    self.stack.push(values.into());
                    Ok(Control::Continue)
                } else {
                    Ok(Control::CallProgramThen(
                        body,
                        Continuation::Seq {
                            remaining,
                            set,
                            body,
                            values,
                            seen,
                        },
                    ))
                }
            }
            Continuation::FixedArray {
                mut remaining,
                body,
                mut values,
            } => {
                values.push(self.pop_value("array body produced no value")?);
                remaining -= 1;
                if remaining == 0 {
                    self.stack.push(values.into());
                    Ok(Control::Continue)
                } else {
                    Ok(Control::CallProgramThen(
                        body,
                        Continuation::FixedArray {
                            remaining,
                            body,
                            values,
                        },
                    ))
                }
            }
            Continuation::MapKey {
                remaining,
                key,
                value,
                object,
            } => {
                let k = self.pop_value("map key produced no value")?;
                let ks = k
                    .as_string()
                    .ok_or(CompactError::Unsupported("map with non-string keys"))?;
                Ok(Control::CallProgramThen(
                    value,
                    Continuation::MapValue {
                        remaining,
                        key,
                        value,
                        object,
                        pending_key: VString::new(ks.as_str()),
                    },
                ))
            }
            Continuation::MapValue {
                mut remaining,
                key,
                value,
                mut object,
                pending_key,
            } => {
                let v = self.pop_value("map value produced no value")?;
                if object.insert(pending_key, v).is_some() {
                    return Err(CompactError::Decode(DecodeError::DuplicateKey));
                }
                remaining -= 1;
                if remaining == 0 {
                    self.stack.push(object.into());
                    Ok(Control::Continue)
                } else {
                    Ok(Control::CallProgramThen(
                        key,
                        Continuation::MapKey {
                            remaining,
                            key,
                            value,
                            object,
                        },
                    ))
                }
            }
            Continuation::EnumPayload { reader_name } => {
                let payload = self.pop_value("variant payload produced no value")?;
                let mut obj = VObject::new();
                obj.insert(VString::new(reader_name), payload);
                self.stack.push(obj.into());
                Ok(Control::Continue)
            }
        }
    }
}

impl Interp<'_, '_> {
    fn pop_value(&mut self, message: &'static str) -> Result<Value> {
        self.stack
            .pop()
            .ok_or(CompactError::Decode(DecodeError::Malformed(message)))
    }

    fn call_repeated<'program, N>(
        &mut self,
        body: &'program Program,
        continuation: Continuation<'program>,
        remaining: N,
    ) -> Control<'program, SchemaId, Op, Continuation<'program>>
    where
        N: PartialEq + From<u8>,
    {
        if remaining == N::from(0) {
            match continuation {
                Continuation::Seq { values, .. } | Continuation::FixedArray { values, .. } => {
                    self.stack.push(values.into());
                }
                Continuation::MapKey { object, .. } => {
                    self.stack.push(object.into());
                }
                Continuation::MapValue { .. } | Continuation::EnumPayload { .. } => {
                    unreachable!("value-producing continuations cannot start empty")
                }
            }
            Control::Continue
        } else {
            Control::CallProgramThen(body, continuation)
        }
    }
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

    #[test]
    fn run_with_stats_reports_weavy_runner_counters() {
        let reg = Registry::new([]);
        let program = vec![Op::Scalar(Primitive::U8)];

        let report = run_with_stats(&program, &[7], &reg).unwrap();

        assert_eq!(report.value, Value::from(7u8));
        assert_eq!(report.stats.step_count, 1);
        assert_eq!(report.stats.inline_call_count, 0);
        assert_eq!(report.stats.block_call_count, 0);
        assert_eq!(report.stats.return_count, 1);
        assert_eq!(report.stats.continuation_resume_count, 0);
        assert_eq!(report.stats.max_frame_depth, 1);
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
