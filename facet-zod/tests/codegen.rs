#![allow(dead_code)]

use std::collections::HashMap;

use facet::Facet;
use facet_zod::{Config, ZodGenerator, generate, generate_with_config};

#[derive(Facet)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

#[derive(Facet)]
struct Post {
    title: String,
    body: String,
    author: User,
    tags: Vec<String>,
}

#[derive(Facet)]
#[repr(u8)]
enum Status {
    Active,
    Inactive,
    Banned,
}

#[derive(Facet)]
#[repr(C)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
}

#[derive(Facet)]
struct Config2 {
    settings: HashMap<String, String>,
    flags: Vec<bool>,
    matrix: [u32; 3],
}

#[derive(Facet)]
struct Wrapper(String);

#[derive(Facet)]
struct Tree {
    value: i32,
    children: Vec<Tree>,
}

#[derive(Facet)]
struct Node {
    edge: Box<Edge>,
}

#[derive(Facet)]
struct Edge {
    back: Option<Box<Node>>,
}

#[derive(Facet)]
struct Defaulted {
    #[facet(default)]
    count: u32,
    name: String,
}

#[derive(Facet)]
struct Wrap<T> {
    inner: T,
}

#[derive(Facet)]
struct Holder {
    a: Wrap<u32>,
    b: Wrap<String>,
}

#[test]
fn test_simple_struct() {
    let output = generate::<User>();
    insta::assert_snapshot!("simple_struct", output);
}

#[test]
fn test_nested_struct() {
    let mut generator = ZodGenerator::new();
    generator.add::<Post>();
    let output = generator.emit();
    insta::assert_snapshot!("nested_struct", output);
}

#[test]
fn test_unit_enum() {
    let output = generate::<Status>();
    insta::assert_snapshot!("unit_enum", output);
}

#[test]
fn test_data_enum() {
    let output = generate::<Shape>();
    insta::assert_snapshot!("data_enum", output);
}

#[test]
fn test_collections() {
    let output = generate::<Config2>();
    insta::assert_snapshot!("collections", output);
}

#[test]
fn test_newtype() {
    let output = generate::<Wrapper>();
    insta::assert_snapshot!("newtype", output);
}

#[test]
fn test_optional_mode_nullable() {
    let config = Config {
        optional_mode: facet_zod::config::OptionalMode::Nullable,
        ..Config::default()
    };
    let output = generate_with_config::<User>(config);
    insta::assert_snapshot!("optional_nullable", output);
}

#[test]
fn test_recursive_self() {
    // Self-recursion must break the cycle with `z.lazy`, not inline forever.
    let output = generate::<Tree>();
    insta::assert_snapshot!("recursive_self", output);
}

#[test]
fn test_mutual_recursion() {
    // Cross-type references: one direction is a plain ref (already declared),
    // the other a `z.lazy` forward ref. Neither side is inlined.
    let output = generate::<Node>();
    insta::assert_snapshot!("mutual_recursion", output);
}

#[test]
fn test_has_default_is_optional() {
    // A non-Option field with `#[facet(default)]` must emit `.optional()`.
    let output = generate::<Defaulted>();
    insta::assert_snapshot!("has_default", output);
}

#[test]
fn test_export_style_type_only() {
    let config = Config {
        export_style: facet_zod::config::ExportStyle::TypeOnly,
        ..Config::default()
    };
    let output = generate_with_config::<Defaulted>(config);
    insta::assert_snapshot!("type_only", output);
}

#[test]
fn test_generic_instantiations_dont_collide() {
    // `Wrap<u32>` and `Wrap<String>` must get distinct schema names.
    let output = generate::<Holder>();
    insta::assert_snapshot!("generics", output);
}

#[test]
fn test_with_header() {
    let config = Config {
        header: Some("import { z } from 'zod';".into()),
        ..Config::default()
    };
    let output = generate_with_config::<User>(config);
    insta::assert_snapshot!("with_header", output);
}
