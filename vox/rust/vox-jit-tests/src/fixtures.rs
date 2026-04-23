//! Typed test fixtures: structs and enums that cover all translation plan variants.
//!
//! Each type is also usable as a schema-evolution pair (remote/local differ by one field).

use facet::Facet;

// ---------------------------------------------------------------------------
// Scalars and primitives
// ---------------------------------------------------------------------------

#[derive(Facet, Debug, PartialEq, Clone)]
pub struct Scalars {
    pub u8_val: u8,
    pub u16_val: u16,
    pub u32_val: u32,
    pub u64_val: u64,
    pub i8_val: i8,
    pub i16_val: i16,
    pub i32_val: i32,
    pub i64_val: i64,
    pub f32_val: f32,
    pub f64_val: f64,
    pub bool_val: bool,
}

impl Scalars {
    pub fn sample() -> Self {
        Self {
            u8_val: 0xFF,
            u16_val: 1000,
            u32_val: 100_000,
            u64_val: u64::MAX / 2,
            i8_val: -42,
            i16_val: -1000,
            i32_val: -100_000,
            i64_val: i64::MIN / 2,
            f32_val: std::f32::consts::PI,
            f64_val: std::f64::consts::E,
            bool_val: true,
        }
    }
}

// ---------------------------------------------------------------------------
// String and byte containers
// ---------------------------------------------------------------------------

#[derive(Facet, Debug, PartialEq, Clone)]
pub struct StringFields {
    pub name: String,
    pub tag: String,
}

impl StringFields {
    pub fn sample() -> Self {
        Self {
            name: "hello, world".to_string(),
            tag: "test".to_string(),
        }
    }

    pub fn empty() -> Self {
        Self {
            name: String::new(),
            tag: String::new(),
        }
    }
}

#[derive(Facet, Debug, PartialEq, Clone)]
pub struct ByteVec {
    pub data: Vec<u8>,
}

impl ByteVec {
    pub fn sample() -> Self {
        Self {
            data: vec![0x00, 0xFF, 0x42, 0xAB, 0x01],
        }
    }

    pub fn empty() -> Self {
        Self { data: vec![] }
    }
}

// ---------------------------------------------------------------------------
// Nested struct
// ---------------------------------------------------------------------------

#[derive(Facet, Debug, PartialEq, Clone)]
pub struct Inner {
    pub value: u32,
    pub label: String,
}

#[derive(Facet, Debug, PartialEq, Clone)]
pub struct Outer {
    pub name: String,
    pub inner: Inner,
    pub count: u32,
}

impl Outer {
    pub fn sample() -> Self {
        Self {
            name: "outer".to_string(),
            inner: Inner {
                value: 99,
                label: "inner".to_string(),
            },
            count: 7,
        }
    }
}

// ---------------------------------------------------------------------------
// Vec<T> containers
// ---------------------------------------------------------------------------

#[derive(Facet, Debug, PartialEq, Clone)]
pub struct VecU32 {
    pub items: Vec<u32>,
}

impl VecU32 {
    pub fn sample() -> Self {
        Self {
            items: vec![1, 2, 3, 100, u32::MAX],
        }
    }

    pub fn empty() -> Self {
        Self { items: vec![] }
    }

    pub fn large() -> Self {
        Self {
            items: (0u32..256).collect(),
        }
    }
}

#[derive(Facet, Debug, PartialEq, Clone)]
pub struct VecString {
    pub tags: Vec<String>,
}

impl VecString {
    pub fn sample() -> Self {
        Self {
            tags: vec!["alpha".into(), "beta".into(), "gamma".into()],
        }
    }
}

// ---------------------------------------------------------------------------
// Option
// ---------------------------------------------------------------------------

#[derive(Facet, Debug, PartialEq, Clone)]
pub struct WithOption {
    pub maybe: Option<u32>,
    pub name: String,
}

impl WithOption {
    pub fn some() -> Self {
        Self {
            maybe: Some(42),
            name: "present".to_string(),
        }
    }

    pub fn none() -> Self {
        Self {
            maybe: None,
            name: "absent".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Facet, Debug, PartialEq, Clone)]
#[repr(u8)]
pub enum Color {
    Red,
    Green,
    Blue,
}

#[derive(Facet, Debug, PartialEq, Clone)]
#[repr(u8)]
pub enum Shape {
    Circle(f64),
    Rect { w: f64, h: f64 },
    Point,
}

#[derive(Facet, Debug, PartialEq, Clone)]
#[repr(u8)]
pub enum Command {
    Noop,
    Move { x: i32, y: i32 },
    Write(String),
    Batch(Vec<u32>),
}

impl Command {
    pub fn all_variants() -> Vec<Self> {
        vec![
            Self::Noop,
            Self::Move { x: 10, y: -5 },
            Self::Write("hello".to_string()),
            Self::Batch(vec![1, 2, 3]),
        ]
    }
}

// ---------------------------------------------------------------------------
// Fixed-size arrays
// ---------------------------------------------------------------------------

#[derive(Facet, Debug, PartialEq, Clone)]
pub struct WithArray {
    pub data: [u32; 4],
}

impl WithArray {
    pub fn sample() -> Self {
        Self {
            data: [10, 20, 30, 40],
        }
    }
}

// ---------------------------------------------------------------------------
// Schema-evolution pairs: remote has extra field, local doesn't (skip test)
// ---------------------------------------------------------------------------

/// Remote type: has an extra field `extra` that local doesn't know about.
#[derive(Facet, Debug, PartialEq, Clone)]
pub struct RemoteWithExtra {
    pub value: u32,
    pub extra: String,
}

/// Local type: only knows `value`.
#[derive(Facet, Debug, PartialEq, Clone)]
pub struct LocalWithoutExtra {
    pub value: u32,
}
