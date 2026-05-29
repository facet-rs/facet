//! Oracle test: facet-json output must validate against the Zod schema
//! facet-zod generates for the same type.
//!
//! For each value we serialize it with `facet-json`, generate the schema with
//! `facet-zod`, and run the schema against the JSON with a real Zod runtime
//! (via `deno` + `npm:zod`). This pins the wire format end to end — a schema
//! that inlines, mis-tags, or drops a field fails here.
//!
//! Requires `deno` on PATH; skipped (not failed) if absent so offline builds
//! stay green.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::Write;
use std::process::Command;

use facet::Facet;
use facet_zod::mapping::schema_name;

fn deno_available() -> bool {
    Command::new("deno")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Serialize `value` with facet-json, generate its Zod schema, and assert the
/// JSON validates against the schema using a real Zod runtime.
#[track_caller]
fn oracle<'a, T: Facet<'a>>(value: &'a T) {
    if !deno_available() {
        eprintln!("oracle: `deno` not found on PATH — skipping Zod validation");
        return;
    }

    let json = facet_json::to_string(value).expect("facet-json serialization failed");
    let schema = facet_zod::generate::<T>();
    let root = format!("{}Schema", schema_name(T::SHAPE));

    let script = format!(
        r#"import {{ z }} from "npm:zod@^3";
{schema}
const __data = {json};
const __r = {root}.safeParse(__data);
if (!__r.success) {{
  console.error(JSON.stringify(__r.error.issues, null, 2));
  Deno.exit(1);
}}
"#
    );

    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "facet_zod_oracle_{}_{:p}.ts",
        std::process::id(),
        value as *const _
    ));
    {
        let mut f = std::fs::File::create(&path).expect("create temp script");
        f.write_all(script.as_bytes()).expect("write temp script");
    }

    let out = Command::new("deno")
        .args(["run", "--quiet", "--no-lock"])
        .arg(&path)
        .env("DENO_NO_UPDATE_CHECK", "1")
        .current_dir(&dir)
        .output()
        .expect("spawn deno");

    let _ = std::fs::remove_file(&path);

    assert!(
        out.status.success(),
        "Zod rejected facet-json output.\n--- JSON ---\n{json}\n--- SCHEMA ---\n{schema}\n--- ZOD ISSUES ---\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

// ---------------------------------------------------------------------------
// Types under test
// ---------------------------------------------------------------------------

#[derive(Facet)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

#[derive(Facet)]
struct Post {
    title: String,
    author: User,
    tags: Vec<String>,
}

#[derive(Facet)]
struct Tree {
    value: i32,
    children: Vec<Tree>,
}

#[derive(Facet)]
struct Plain(String);

#[derive(Facet)]
#[facet(transparent)]
struct Transparent(String);

#[derive(Facet)]
struct TwoField(u32, String);

#[derive(Facet)]
struct WithDefault {
    #[facet(default)]
    count: u32,
    name: String,
}

#[derive(Facet)]
struct WithSkip {
    keep: u32,
    #[facet(skip_serializing)]
    drop: u32,
}

#[derive(Facet)]
struct Maps {
    by_int: BTreeMap<u32, String>,
    by_str: BTreeMap<String, i32>,
}

#[derive(Facet)]
struct Results {
    ok: Result<u32, String>,
    err: Result<u32, String>,
}

#[derive(Facet)]
#[repr(u8)]
enum ExternalEnum {
    Unit,
    One(u32),
    Pair(u32, String),
    Named { x: i32, y: i32 },
}

#[derive(Facet)]
#[repr(u8)]
enum AllUnit {
    A,
    B,
    C,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(tag = "type")]
enum InternalEnum {
    Unit,
    Named { x: i32, y: i32 },
}

#[derive(Facet)]
#[repr(u8)]
#[facet(tag = "t", content = "c")]
enum AdjacentEnum {
    Unit,
    One(u32),
    Pair(u32, String),
    Named { x: i32 },
}

#[derive(Facet)]
#[repr(u8)]
#[facet(untagged)]
enum UntaggedEnum {
    TheUnit,
    AsNum(u32),
    AsObj { x: i32 },
}

#[derive(Facet)]
struct Holder {
    ext: ExternalEnum,
    adj: AdjacentEnum,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_simple_struct() {
    oracle(&User {
        name: "Ada".into(),
        age: 36,
        email: None,
    });
    oracle(&User {
        name: "Bo".into(),
        age: 1,
        email: Some("bo@example.com".into()),
    });
}

#[test]
fn oracle_nested_struct_ref() {
    oracle(&Post {
        title: "Hi".into(),
        author: User {
            name: "Ada".into(),
            age: 36,
            email: None,
        },
        tags: vec!["a".into(), "b".into()],
    });
}

#[test]
fn oracle_recursive() {
    oracle(&Tree {
        value: 1,
        children: vec![
            Tree {
                value: 2,
                children: vec![],
            },
            Tree {
                value: 3,
                children: vec![Tree {
                    value: 4,
                    children: vec![],
                }],
            },
        ],
    });
}

#[test]
fn oracle_tuple_structs() {
    oracle(&Plain("w".into()));
    oracle(&Transparent("w".into()));
    oracle(&TwoField(1, "a".into()));
}

#[test]
fn oracle_default_and_skip() {
    oracle(&WithDefault {
        count: 0,
        name: "n".into(),
    });
    oracle(&WithSkip { keep: 1, drop: 9 });
}

#[test]
fn oracle_maps() {
    let mut by_int = BTreeMap::new();
    by_int.insert(1u32, "one".to_string());
    by_int.insert(2u32, "two".to_string());
    let mut by_str = BTreeMap::new();
    by_str.insert("k".to_string(), 7i32);
    oracle(&Maps { by_int, by_str });
}

#[test]
fn oracle_result() {
    oracle(&Results {
        ok: Ok(3),
        err: Err("bad".into()),
    });
}

#[test]
fn oracle_external_enum() {
    oracle(&ExternalEnum::Unit);
    oracle(&ExternalEnum::One(7));
    oracle(&ExternalEnum::Pair(7, "a".into()));
    oracle(&ExternalEnum::Named { x: 1, y: 2 });
}

#[test]
fn oracle_all_unit_enum() {
    oracle(&AllUnit::A);
    oracle(&AllUnit::C);
}

#[test]
fn oracle_internal_enum() {
    oracle(&InternalEnum::Unit);
    oracle(&InternalEnum::Named { x: 1, y: 2 });
}

#[test]
fn oracle_adjacent_enum() {
    oracle(&AdjacentEnum::Unit);
    oracle(&AdjacentEnum::One(7));
    oracle(&AdjacentEnum::Pair(7, "a".into()));
    oracle(&AdjacentEnum::Named { x: 1 });
}

#[test]
fn oracle_untagged_enum() {
    oracle(&UntaggedEnum::TheUnit);
    oracle(&UntaggedEnum::AsNum(7));
    oracle(&UntaggedEnum::AsObj { x: 1 });
}

#[test]
fn oracle_enum_fields_in_struct() {
    oracle(&Holder {
        ext: ExternalEnum::Pair(1, "z".into()),
        adj: AdjacentEnum::Named { x: 9 },
    });
}
