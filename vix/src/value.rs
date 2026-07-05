use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::ast::Expr;

#[derive(facet::Facet, Debug, Clone)]
#[repr(u8)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Blob(Vec<u8>),
    Str(String),
    Path(String),
    Flag(String),
    Tuple(Vec<Value>),
    Array(Vec<Value>),
    Map(BTreeMap<Value, Value>),
    Struct {
        name: String,
        fields: Vec<(String, Value)>,
    },
    Variant {
        enum_name: String,
        index: usize,
        name: String,
        payload: Payload,
    },
    Fn {
        name: String,
        hash: u64,
    },
    Closure {
        hash: u64,
        params: Vec<String>,
        body: Box<Expr>,
        env: Vec<(String, Value)>,
    },
    Partial {
        func: String,
        given: Vec<(String, Value)>,
    },
    Tree(crate::exec::Tree),
    Sealed {
        ciphertext: Vec<u8>,
        taint: String,
        recipient: String,
        identity_hash: Vec<u8>,
        content_tag: Option<String>,
    },
}

#[derive(facet::Facet, Debug, Clone)]
#[repr(u8)]
pub enum Payload {
    Unit,
    Tuple(Vec<Value>),
    Record(Vec<(String, Value)>),
}

impl Value {
    fn rank(&self) -> u8 {
        match self {
            Value::Int(_) => 0,
            Value::Float(_) => 1,
            Value::Bool(_) => 2,
            Value::Str(_) => 3,
            Value::Path(_) => 4,
            Value::Flag(_) => 5,
            Value::Tuple(_) => 6,
            Value::Array(_) => 7,
            Value::Map(_) => 8,
            Value::Struct { .. } => 9,
            Value::Variant { .. } => 10,
            Value::Fn { .. } => 11,
            Value::Closure { .. } => 12,
            Value::Partial { .. } => 13,
            Value::Tree(_) => 14,
            Value::Blob(_) => 15,
            Value::Sealed { .. } => 16,
        }
    }

    fn forced_tree(&self) -> Option<crate::exec::Tree> {
        match self {
            Value::Tree(t) => Some(t.clone()),
            _ => None,
        }
    }

    pub fn short(&self) -> String {
        match self {
            Value::Int(v) => v.to_string(),
            Value::Float(v) => v.to_string(),
            Value::Bool(v) => v.to_string(),
            Value::Blob(v) => format!("blob({} bytes)", v.len()),
            Value::Str(v) => format!("{v:?}"),
            Value::Path(v) => v.clone(),
            Value::Flag(v) => v.clone(),
            Value::Tuple(vs) => format!(
                "({})",
                vs.iter().map(|v| v.short()).collect::<Vec<_>>().join(", ")
            ),
            Value::Array(vs) => format!(
                "[{}]",
                vs.iter().map(|v| v.short()).collect::<Vec<_>>().join(", ")
            ),
            Value::Map(entries) => format!("{{…{} entries}}", entries.len()),
            Value::Struct { name, fields } => format!("{name}{{…{}}}", fields.len()),
            Value::Variant {
                enum_name, name, ..
            } => format!("{enum_name}::{name}"),
            Value::Fn { name, .. } => format!("fn {name}"),
            Value::Closure { .. } => "closure".to_string(),
            Value::Partial { func, .. } => format!("partial {func}"),
            Value::Tree(t) => {
                let mut h = DefaultHasher::new();
                self.hash_into(&mut h);
                format!(
                    "tree({:08x}, {} paths)",
                    h.finish() as u32,
                    t.entries.len() + t.blobs.len()
                )
            }
            Value::Sealed {
                taint,
                recipient,
                identity_hash,
                ..
            } => format!(
                "sealed({taint}, {recipient}, {:08x})",
                short_hash(identity_hash)
            ),
        }
    }

    pub fn hash_into(&self, h: &mut DefaultHasher) {
        self.rank().hash(h);
        match self {
            Value::Int(v) => v.hash(h),
            Value::Float(v) => normalize_float(*v).to_bits().hash(h),
            Value::Bool(v) => v.hash(h),
            Value::Blob(v) => v.hash(h),
            Value::Str(v) | Value::Path(v) | Value::Flag(v) => v.hash(h),
            Value::Tuple(vs) | Value::Array(vs) => {
                vs.len().hash(h);
                for v in vs {
                    v.hash_into(h);
                }
            }
            Value::Map(m) => {
                m.len().hash(h);
                for (k, v) in m {
                    k.hash_into(h);
                    v.hash_into(h);
                }
            }
            Value::Struct { name, fields } => {
                name.hash(h);
                for (fname, v) in fields {
                    fname.hash(h);
                    v.hash_into(h);
                }
            }
            Value::Variant {
                enum_name,
                index,
                payload,
                ..
            } => {
                enum_name.hash(h);
                index.hash(h);
                match payload {
                    Payload::Unit => {}
                    Payload::Tuple(vs) => {
                        for v in vs {
                            v.hash_into(h);
                        }
                    }
                    Payload::Record(fs) => {
                        for (n, v) in fs {
                            n.hash(h);
                            v.hash_into(h);
                        }
                    }
                }
            }
            Value::Fn { hash, .. } => hash.hash(h),
            Value::Closure { hash, env, .. } => {
                hash.hash(h);
                for (n, v) in env {
                    n.hash(h);
                    v.hash_into(h);
                }
            }
            Value::Partial { func, given } => {
                func.hash(h);
                for (n, v) in given {
                    n.hash(h);
                    v.hash_into(h);
                }
            }
            Value::Tree(_) => self.forced_tree().expect("tree rank").fingerprint().hash(h),
            Value::Sealed {
                ciphertext,
                taint,
                recipient,
                identity_hash,
                content_tag,
            } => {
                ciphertext.hash(h);
                taint.hash(h);
                recipient.hash(h);
                identity_hash.hash(h);
                content_tag.hash(h);
            }
        }
    }

    pub fn canon_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.hash_into(&mut h);
        h.finish()
    }
}

fn normalize_float(v: f64) -> f64 {
    if v.is_nan() {
        f64::NAN
    } else if v == 0.0 {
        0.0
    } else {
        v
    }
}

fn float_cmp(a: f64, b: f64) -> std::cmp::Ordering {
    normalize_float(a).total_cmp(&normalize_float(b))
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}
impl Eq for Value {}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => float_cmp(*a, *b),
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Blob(a), Value::Blob(b)) => a.cmp(b),
            (Value::Str(a), Value::Str(b))
            | (Value::Path(a), Value::Path(b))
            | (Value::Flag(a), Value::Flag(b)) => a.cmp(b),
            (Value::Tuple(a), Value::Tuple(b)) | (Value::Array(a), Value::Array(b)) => a.cmp(b),
            (Value::Map(a), Value::Map(b)) => a.cmp(b),
            (
                Value::Struct {
                    name: an,
                    fields: af,
                },
                Value::Struct {
                    name: bn,
                    fields: bf,
                },
            ) => an.cmp(bn).then_with(|| af.cmp(bf)),
            (
                Value::Variant {
                    enum_name: ae,
                    index: ai,
                    payload: ap,
                    ..
                },
                Value::Variant {
                    enum_name: be,
                    index: bi,
                    payload: bp,
                    ..
                },
            ) => ae
                .cmp(be)
                .then_with(|| ai.cmp(bi))
                .then_with(|| match (ap, bp) {
                    (Payload::Unit, Payload::Unit) => Ordering::Equal,
                    (Payload::Tuple(a), Payload::Tuple(b)) => a.cmp(b),
                    (Payload::Record(a), Payload::Record(b)) => a.cmp(b),
                    _ => Ordering::Equal,
                }),
            (Value::Fn { hash: a, .. }, Value::Fn { hash: b, .. }) => a.cmp(b),
            (Value::Closure { .. }, Value::Closure { .. })
            | (Value::Partial { .. }, Value::Partial { .. }) => {
                self.canon_hash().cmp(&other.canon_hash())
            }
            (Value::Tree(_), Value::Tree(_)) => {
                let a = self.forced_tree().expect("tree rank");
                let b = other.forced_tree().expect("tree rank");
                a.entries
                    .cmp(&b.entries)
                    .then_with(|| a.blobs.cmp(&b.blobs))
            }
            (
                Value::Sealed {
                    ciphertext: ac,
                    taint: at,
                    recipient: ar,
                    identity_hash: ah,
                    content_tag: ag,
                },
                Value::Sealed {
                    ciphertext: bc,
                    taint: bt,
                    recipient: br,
                    identity_hash: bh,
                    content_tag: bg,
                },
            ) => ac
                .cmp(bc)
                .then_with(|| at.cmp(bt))
                .then_with(|| ar.cmp(br))
                .then_with(|| ah.cmp(bh))
                .then_with(|| ag.cmp(bg)),
            _ => self.rank().cmp(&other.rank()),
        }
    }
}

fn short_hash(bytes: &[u8]) -> u32 {
    let mut out = [0u8; 4];
    let len = bytes.len().min(out.len());
    out[..len].copy_from_slice(&bytes[..len]);
    u32::from_be_bytes(out)
}
