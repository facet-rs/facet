use rapace_codegen::targets;
use rapace_schema::{ArgDetail, MethodDetail, TypeDetail};

fn fixture_methods() -> Vec<MethodDetail> {
    vec![
        MethodDetail {
            service_name: "TemplateHost".into(),
            method_name: "load_template".into(),
            args: vec![
                ArgDetail {
                    name: "context_id".into(),
                    type_info: TypeDetail::U64,
                },
                ArgDetail {
                    name: "name".into(),
                    type_info: TypeDetail::String,
                },
            ],
            return_type: TypeDetail::Bytes,
        },
        MethodDetail {
            service_name: "TemplateHost".into(),
            method_name: "loadTemplate".into(), // should normalize to same kebab as load_template
            args: vec![ArgDetail {
                name: "id".into(),
                type_info: TypeDetail::U64,
            }],
            return_type: TypeDetail::Unit,
        },
    ]
}

#[test]
fn typescript_contains_hex_bigint_ids() {
    let methods = fixture_methods();
    let out = targets::typescript::generate_method_ids(&methods);
    assert!(out.contains("export const METHOD_ID"));
    // ensure we emit hex + bigint literal
    assert!(out.contains("0x"));
    assert!(out.contains("n,"));
}

#[test]
fn swift_contains_uint64_literals() {
    let methods = fixture_methods();
    let out = targets::swift::generate_method_ids(&methods);
    assert!(out.contains("UInt64"));
    assert!(out.contains("0x"));
}

#[test]
fn go_contains_uint64_map() {
    let methods = fixture_methods();
    let out = targets::go::generate_method_ids(&methods);
    assert!(out.contains("map[string]uint64"));
    assert!(out.contains("0x"));
}

#[test]
fn java_contains_map_entries() {
    let methods = fixture_methods();
    let out = targets::java::generate_method_ids(&methods);
    assert!(out.contains("Map.entry("));
    assert!(out.contains("0x"));
    assert!(out.contains("L)"));
}
