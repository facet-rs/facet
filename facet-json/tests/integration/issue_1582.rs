use facet::Facet;
use facet_reflect::Partial;

#[derive(Facet, Debug, Clone, PartialEq, Eq)]
struct Document {
    root: FlowContent,
}

#[derive(Facet, Debug, Clone, PartialEq, Eq)]
struct ElementNode {
    child: Box<FlowContent>,
}

#[derive(Facet, Debug, Clone, PartialEq, Eq)]
#[facet(tag = "kind", content = "content")]
#[repr(C)]
enum FlowContent {
    Element(ElementNode),
    Text(String),
    V01(u8),
    V02(u8),
    V03(u8),
    V04(u8),
    V05(u8),
    V06(u8),
    V07(u8),
    V08(u8),
    V09(u8),
    V10(u8),
    V11(u8),
    V12(u8),
    V13(u8),
    V14(u8),
    V15(u8),
    V16(u8),
    V17(u8),
    V18(u8),
    V19(u8),
    V20(u8),
    V21(u8),
    V22(u8),
    V23(u8),
    V24(u8),
    V25(u8),
    V26(u8),
    V27(u8),
    V28(u8),
    V29(u8),
    V30(u8),
    V31(u8),
    V32(u8),
    V33(u8),
    V34(u8),
    V35(u8),
    V36(u8),
    V37(u8),
    V38(u8),
    V39(u8),
    V40(u8),
    V41(u8),
    V42(u8),
    V43(u8),
    V44(u8),
    V45(u8),
    V46(u8),
    V47(u8),
    V48(u8),
}

fn deep_payload(depth: usize) -> String {
    let mut payload = r#"{"kind":"Text","content":"leaf"}"#.to_owned();
    for _ in 0..depth {
        payload = format!(r#"{{"kind":"Element","content":{{"child":{payload}}}}}"#);
    }
    payload
}

#[test]
fn deeply_nested_large_enum_chain_deserializes() {
    let depth = std::env::var("FACET_ISSUE_1582_DEPTH")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(16);
    let json = format!(r#"{{"root":{}}}"#, deep_payload(depth));

    let partial = Partial::alloc_owned::<Document>().expect("allocate partial");
    let partial = facet_json::from_str_into(&json, partial)
        .expect("deep enum/struct chain should deserialize");
    let value: Document = partial
        .build()
        .expect("partial should build")
        .materialize()
        .expect("materialize document");

    let mut current = &value.root;
    let mut actual_depth = 0usize;
    loop {
        match current {
            FlowContent::Element(node) => {
                actual_depth += 1;
                current = node.child.as_ref();
            }
            FlowContent::Text(text) => {
                assert_eq!(text, "leaf");
                break;
            }
            other => panic!("unexpected terminal variant: {other:?}"),
        }
    }

    assert_eq!(actual_depth, depth);
}
