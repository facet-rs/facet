//! Test for https://github.com/facet-rs/facet/issues/1852
//!
//! CRLF line endings combined with scientific notation in nested structures
//! caused parsing to fail, while the same JSON with LF line endings works.

use std::collections::HashMap;
use std::sync::Arc;

use facet::Facet;
use facet_testhelpers::test;

#[derive(Clone, Debug, PartialEq, Facet)]
pub struct TwoVecs {
    pub x: Vec<f32>,
    pub y: Vec<f32>,
}

#[test]
fn test_two_arrays_scientific_crlf() {
    let json_crlf = concat!(
        "{\r\n",
        "  \"x\": [\r\n",
        "    0.0\r\n",
        "  ],\r\n",
        "  \"y\": [\r\n",
        "    5e-05\r\n",
        "  ]\r\n",
        "}"
    );
    let json_lf = json_crlf.replace("\r\n", "\n");

    let result_lf: Result<TwoVecs, _> = facet_json::from_str(&json_lf);
    assert!(result_lf.is_ok(), "LF version should work");

    let result_crlf: Result<TwoVecs, _> = facet_json::from_str(json_crlf);
    assert!(
        result_crlf.is_ok(),
        "CRLF version should also work: {:?}",
        result_crlf.err()
    );
}

#[derive(Clone, Facet)]
pub struct VecPair {
    pub x: Vec<f32>,
    pub y: Vec<f32>,
}

#[derive(Clone, Facet)]
#[repr(C)]
#[allow(dead_code)]
pub enum TypeA {
    VariantA { inner: Arc<VecPair> },
}

#[derive(Clone, Facet)]
pub struct TypeB {
    pub nested: TypeA,
    pub value: f64,
}

#[derive(Clone, Facet)]
pub struct Wrapper {
    pub items: HashMap<String, TypeB>,
}

#[derive(Clone, Facet)]
pub struct Container {
    pub data: Wrapper,
}

/// Original test case from the issue report
#[test]
fn test_facet_crlf_scientific_notation_bug() {
    let json_crlf = concat!(
        "{\r\n",
        "  \"data\": {\r\n",
        "    \"items\": {\r\n",
        "      \"key1\": {\r\n",
        "        \"nested\": {\r\n",
        "          \"VariantA\": {\r\n",
        "            \"inner\": {\r\n",
        "              \"x\": [\r\n",
        "                0.0\r\n",
        "              ],\r\n",
        "              \"y\": [\r\n",
        "                5e-05\r\n",
        "              ]\r\n",
        "            }\r\n",
        "          }\r\n",
        "        },\r\n",
        "        \"value\": 10\r\n",
        "      }\r\n",
        "    }\r\n",
        "  }\r\n",
        "}"
    );
    let json_lf = json_crlf.replace("\r\n", "\n");

    let result_lf: Result<Container, _> = facet_json::from_str(&json_lf);
    assert!(result_lf.is_ok(), "LF version should work");

    let result_crlf: Result<Container, _> = facet_json::from_str(json_crlf);
    assert!(
        result_crlf.is_ok(),
        "CRLF version should also work: {:?}",
        result_crlf.err()
    );
}
