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
fn test_with_header() {
    let config = Config {
        header: Some("import { z } from 'zod';".into()),
        ..Config::default()
    };
    let output = generate_with_config::<User>(config);
    insta::assert_snapshot!("with_header", output);
}
