use facet::Facet;
use facet_format::DeserializeErrorKind;

#[derive(Debug, Facet, PartialEq, Eq)]
struct Person {
    name: String,
    age: u32,
}

#[derive(Debug, Facet, PartialEq, Eq)]
struct Report {
    title: String,
    scores: Vec<u32>,
    active: Option<bool>,
}

#[derive(Debug, Facet, PartialEq, Eq)]
struct Node {
    id: u32,
    #[facet(recursive_type)]
    child: Option<Box<Node>>,
}

impl Node {
    fn chain(depth: u32) -> Self {
        let mut node = Self {
            id: depth,
            child: None,
        };

        for id in (0..depth).rev() {
            node = Self {
                id,
                child: Some(Box::new(node)),
            };
        }

        node
    }

    fn depth(&self) -> u32 {
        let mut depth = 0;
        let mut current = self;
        while let Some(child) = current.child.as_deref() {
            depth += 1;
            current = child;
        }
        depth
    }
}

fn node_json(depth: u32) -> String {
    let mut json = format!(r#"{{"id":{depth},"child":null}}"#);
    for id in (0..depth).rev() {
        json = format!(r#"{{"id":{id},"child":{json}}}"#);
    }
    json
}

#[test]
fn vm_deserializes_simple_struct_like_default_path() {
    let json = r#"{"name":"Ada","age":37}"#;

    let default: Person = facet_json::from_str(json).unwrap();
    let vm: Person = facet_json::from_str_vm(json).unwrap();

    assert_eq!(vm, default);
}

#[test]
fn reusable_vm_plan_deserializes_like_default_path() {
    let json = r#"{"name":"Ada","age":37}"#;
    let plan = facet_json::JsonVmPlan::<Person>::build().unwrap();

    let default: Person = facet_json::from_str(json).unwrap();
    let vm = plan.from_str(json).unwrap();

    assert_eq!(vm, default);
}

#[test]
fn vm_deserializes_lists_and_options_like_default_path() {
    let json = r#"{"title":"run","scores":[1,2,3,5],"active":true}"#;

    let default: Report = facet_json::from_str(json).unwrap();
    let vm: Report = facet_json::from_str_vm(json).unwrap();

    assert_eq!(vm, default);
}

#[test]
fn vm_deserializes_recursive_owned_chain() {
    let depth = 96;
    let json = node_json(depth);

    let (vm, stats): (Node, _) = facet_json::from_str_vm_with_stats(&json).unwrap();

    assert_eq!(vm.depth(), depth);
    assert_eq!(vm, Node::chain(depth));
    assert!(stats.max_vm_frames >= depth as usize);
    assert!(stats.max_partial_frames >= depth as usize);
    #[cfg(feature = "stacker")]
    {
        assert!(stats.native_stack_samples > depth as usize);
        assert!(
            stats.native_stack_bytes <= 256 * 1024,
            "VM used {} native stack bytes for depth {depth}",
            stats.native_stack_bytes
        );
    }
}

#[test]
fn vm_skips_unknown_fields_when_the_type_allows_it() {
    let json = r#"{"unknown":{"nested":[1,2,3]},"name":"Ada","age":37}"#;

    let vm: Person = facet_json::from_str_vm(json).unwrap();

    assert_eq!(
        vm,
        Person {
            name: "Ada".to_owned(),
            age: 37
        }
    );
}

#[test]
fn vm_rejects_duplicate_struct_fields() {
    let json = r#"{"name":"Ada","name":"Lovelace","age":37}"#;

    let err = facet_json::from_str_vm::<Person>(json).unwrap_err();

    assert!(matches!(
        err.kind,
        DeserializeErrorKind::DuplicateField { ref field, .. } if field.as_ref() == "name"
    ));
}
