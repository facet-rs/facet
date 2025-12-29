use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Debug, Facet, PartialEq)]
pub struct Document {
    #[facet(default, kdl::children = "trigger")]
    trigger: Vec<Trigger>,
}

#[derive(Debug, Default, Facet, PartialEq)]
pub struct Branch {
    #[facet(kdl::argument)]
    value: String,
}

#[derive(Debug, Facet, PartialEq)]
pub struct Trigger {
    #[facet(kdl::argument)]
    tag: String,

    #[facet(default, kdl::children = "branch")]
    branch: Vec<Branch>,
}

#[test]
fn test_children_with_custom_name() {
    // Bug from uploaded file: kdl::children = "branch" returns empty Vec
    let input = r#"
trigger "git-push" {
    branch "main"
}
"#;

    let doc: Document = facet_kdl_legacy::from_str(input).unwrap();

    println!("Parsed: {:#?}", doc);

    assert_eq!(doc.trigger.len(), 1);
    assert_eq!(doc.trigger[0].tag, "git-push");
    assert_eq!(
        doc.trigger[0].branch.len(),
        1,
        "Expected 1 branch, got {}",
        doc.trigger[0].branch.len()
    );
    assert_eq!(doc.trigger[0].branch[0].value, "main");
}

#[test]
fn test_unrelated_nodes_not_captured() {
    // Bug from user message: job nodes incorrectly parsed as trigger nodes
    let input = r#"
trigger "git-push" {
    branch "main"
}

job "test" {
    task "first" {
        command """
        cargo build
        """
    }
}
"#;

    let doc: Document = facet_kdl_legacy::from_str(input).unwrap();

    println!("Parsed: {:#?}", doc);

    // Should only have ONE trigger, not two
    assert_eq!(
        doc.trigger.len(),
        1,
        "Expected 1 trigger, got {}. Second trigger: {:?}",
        doc.trigger.len(),
        doc.trigger.get(1)
    );
    assert_eq!(doc.trigger[0].tag, "git-push");
    assert_eq!(doc.trigger[0].branch.len(), 1);
    assert_eq!(doc.trigger[0].branch[0].value, "main");
}

// ============================================================================
// Enum variant mapping limitation
// ============================================================================

#[derive(Debug, Facet, PartialEq)]
pub struct EnumDocument {
    #[facet(default, kdl::children)]
    triggers: Vec<EnumTrigger>,
}

#[derive(Debug, Facet, PartialEq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum EnumTrigger {
    GitPush {
        #[facet(default, kdl::children = "branch")]
        branches: Vec<Branch>,
    },
}

#[test]
fn test_enum_variant_with_argument() {
    // This is the user's desired syntax from the uploaded file:
    // trigger "git-push" {
    //     branch "fix/*"
    //     branch "feat/*"
    // }
    //
    // However, there's currently NO way to map the node argument "git-push"
    // to the enum variant GitPush. The enum variant is matched by the NODE NAME,
    // not by an argument.
    //
    // This test documents the CURRENT behavior (matching by node name):

    let input = r#"
git-push {
    branch "fix/*"
    branch "feat/*"
}
"#;

    let doc: EnumDocument = facet_kdl_legacy::from_str(input).unwrap();
    println!("Parsed: {:#?}", doc);

    assert_eq!(doc.triggers.len(), 1);
    match &doc.triggers[0] {
        EnumTrigger::GitPush { branches } => {
            assert_eq!(branches.len(), 2);
            assert_eq!(branches[0].value, "fix/*");
            assert_eq!(branches[1].value, "feat/*");
        }
    }
}

#[test]
fn test_enum_variant_argument_not_supported() {
    // This documents that the user's DESIRED syntax doesn't work:
    let input = r#"
trigger "git-push" {
    branch "fix/*"
}
"#;

    let result: Result<EnumDocument, _> = facet_kdl_legacy::from_str(input);

    // This will fail because:
    // 1. The node name is "trigger" (not "git-push")
    // 2. There's no EnumTrigger variant named "trigger"
    // 3. The argument "git-push" is ignored for enum variant matching

    assert!(
        result.is_err(),
        "Expected error because 'trigger' doesn't match any variant name"
    );

    let err = result.unwrap_err();
    println!("Error message:\n{:?}", err);
}
