//! Snapshot tests for path formatting (non-pretty version)

use facet::Facet;
use facet_path::{Path, PathStep};

#[test]
fn test_simple_field_path() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Config {
        name: String,
        max_retries: u8,
        enabled: bool,
    }

    let mut path = Path::new(Config::SHAPE);
    path.push(PathStep::Field(1)); // max_retries

    let formatted = path.format();
    insta::assert_snapshot!(formatted);
}

#[test]
fn test_nested_struct_path() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Inner {
        value: i32,
    }

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Outer {
        label: String,
        inner: Inner,
    }

    let mut path = Path::new(Outer::SHAPE);
    path.push(PathStep::Field(1)); // inner
    path.push(PathStep::Field(0)); // value

    let formatted = path.format();
    insta::assert_snapshot!(formatted);
}

#[test]
fn test_vec_index_path() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Item {
        id: u32,
    }

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Container {
        items: Vec<Item>,
    }

    let mut path = Path::new(Container::SHAPE);
    path.push(PathStep::Field(0)); // items
    path.push(PathStep::Index(2)); // [2]
    path.push(PathStep::Field(0)); // id

    let formatted = path.format();
    insta::assert_snapshot!(formatted);
}

#[test]
fn test_option_path() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Config {
        name: String,
        debug_info: Option<String>,
    }

    let mut path = Path::new(Config::SHAPE);
    path.push(PathStep::Field(1)); // debug_info
    path.push(PathStep::OptionSome);

    let formatted = path.format();
    insta::assert_snapshot!(formatted);
}

#[test]
fn test_enum_variant_path() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[repr(C)]
    #[allow(dead_code)]
    enum Message {
        Simple,
        WithData { value: u32, name: String },
    }

    let mut path = Path::new(Message::SHAPE);
    path.push(PathStep::Variant(1)); // WithData
    path.push(PathStep::Field(1)); // name

    let formatted = path.format();
    insta::assert_snapshot!(formatted);
}

#[test]
fn test_map_path() {
    facet_testhelpers::setup();

    use std::collections::HashMap;

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Config {
        settings: HashMap<String, u32>,
    }

    let mut path = Path::new(Config::SHAPE);
    path.push(PathStep::Field(0)); // settings
    path.push(PathStep::MapKey);

    let formatted = path.format();
    insta::assert_snapshot!("map_key_path", formatted);

    let mut path = Path::new(Config::SHAPE);
    path.push(PathStep::Field(0)); // settings
    path.push(PathStep::MapValue);

    let formatted = path.format();
    insta::assert_snapshot!("map_value_path", formatted);
}

#[test]
fn test_empty_path() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Config {
        name: String,
    }

    let path = Path::new(Config::SHAPE);
    let formatted = path.format();
    insta::assert_snapshot!(formatted);
}
