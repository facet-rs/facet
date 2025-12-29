use facet::Facet;
use facet_kdl_legacy as kdl;
use indoc::indoc;

/// Test that enum children can be deserialized using node name as variant discriminant.
/// This is useful for DSLs where the node name indicates the type of action/widget/etc.
#[test]
fn enum_child_by_variant_name() {
    #[derive(Facet, PartialEq, Debug)]
    struct Step {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::child)]
        action: Action,
    }

    #[derive(Facet, PartialEq, Debug)]
    #[repr(u8)]
    enum Action {
        Print {
            #[facet(kdl::property)]
            message: String,
            #[facet(kdl::property)]
            level: Option<String>,
        },
        Write {
            #[facet(kdl::property)]
            path: String,
            #[facet(kdl::property)]
            content: Option<String>,
        },
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Pipeline {
        #[facet(kdl::children)]
        steps: Vec<Step>,
    }

    let kdl = indoc! {r#"
        step "greeting" {
            Print message="hello" level="info"
        }
        step "save-output" {
            Write path="/tmp/output.txt" content="done"
        }
    "#};

    let pipeline: Pipeline = facet_kdl_legacy::from_str(kdl).unwrap();

    assert_eq!(pipeline.steps.len(), 2);

    assert_eq!(pipeline.steps[0].name, "greeting");
    assert_eq!(
        pipeline.steps[0].action,
        Action::Print {
            message: "hello".to_string(),
            level: Some("info".to_string()),
        }
    );

    assert_eq!(pipeline.steps[1].name, "save-output");
    assert_eq!(
        pipeline.steps[1].action,
        Action::Write {
            path: "/tmp/output.txt".to_string(),
            content: Some("done".to_string()),
        }
    );
}

/// Test Vec<enum> where variants have same fields (issue reproduction)
/// Node name should be used as the discriminator.
#[test]
fn vec_enum_children_same_fields_kebab_case() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[facet(rename_all = "kebab-case")]
    pub enum Command {
        SaveScreenshot {
            #[facet(kdl::property)]
            keys: String,
        },
        CopyToClipboard {
            #[facet(kdl::property)]
            keys: String,
        },
        SelectRegion {
            #[facet(kdl::argument)]
            selection: String,
            #[facet(kdl::property)]
            keys: String,
        },
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct KeyMap {
        #[facet(kdl::children)]
        keymap: Vec<Command>,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct Config {
        #[facet(kdl::child)]
        keymap: KeyMap,
    }

    let kdl = indoc! {r#"
        keymap {
            save-screenshot keys=s
            select-region "full" keys=<f11>
            copy-to-clipboard keys=<enter>
        }
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();

    assert_eq!(config.keymap.keymap.len(), 3);
    assert_eq!(
        config.keymap.keymap[0],
        Command::SaveScreenshot {
            keys: "s".to_string()
        }
    );
    assert_eq!(
        config.keymap.keymap[1],
        Command::SelectRegion {
            selection: "full".to_string(),
            keys: "<f11>".to_string()
        }
    );
    assert_eq!(
        config.keymap.keymap[2],
        Command::CopyToClipboard {
            keys: "<enter>".to_string()
        }
    );
}

/// Test enum child with rename_all to use kebab-case node names.
#[test]
fn enum_child_with_rename_all() {
    #[derive(Facet, PartialEq, Debug)]
    struct Container {
        #[facet(kdl::child)]
        event: Event,
    }

    #[derive(Facet, PartialEq, Debug)]
    #[facet(rename_all = "kebab-case")]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Event {
        UserCreated {
            #[facet(kdl::property)]
            username: String,
        },
        FileUploaded {
            #[facet(kdl::property)]
            path: String,
        },
    }

    let kdl = indoc! {r#"
        user-created username="alice"
    "#};

    let container: Container = facet_kdl_legacy::from_str(kdl).unwrap();

    assert_eq!(
        container.event,
        Event::UserCreated {
            username: "alice".to_string(),
        }
    );
}
