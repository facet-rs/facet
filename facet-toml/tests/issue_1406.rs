use facet::Facet;

#[derive(Facet, Debug, Clone, PartialEq)]
struct DetailedDep {
    version: Option<String>,
    features: Option<Vec<String>>,
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct WorkspaceDep {
    workspace: bool,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
#[facet(untagged)]
enum Dep {
    Simple(String),
    Workspace(WorkspaceDep),
    Detailed(DetailedDep),
}

#[derive(Facet, Debug, PartialEq)]
struct Config {
    dep: Dep,
}

#[test]
fn test_untagged_enum_with_struct_variants() {
    // Simple variant - works
    let toml1 = r#"dep = "1.0""#;
    let c1: Config = facet_toml::from_str(toml1).unwrap();
    assert_eq!(c1.dep, Dep::Simple("1.0".to_string()));
    println!("✓ Simple: {:?}", c1);

    // Workspace variant - works
    let toml2 = r#"dep = { workspace = true }"#;
    let c2: Config = facet_toml::from_str(toml2).unwrap();
    assert_eq!(c2.dep, Dep::Workspace(WorkspaceDep { workspace: true }));
    println!("✓ Workspace: {:?}", c2);

    // Detailed variant - FAILS (regression from PR #1405)
    let toml3 = r#"dep = { version = "2.0", features = ["foo"] }"#;
    let c3: Config = facet_toml::from_str(toml3).unwrap();
    assert_eq!(
        c3.dep,
        Dep::Detailed(DetailedDep {
            version: Some("2.0".to_string()),
            features: Some(vec!["foo".to_string()])
        })
    );
    println!("✓ Detailed: {:?}", c3);
}
