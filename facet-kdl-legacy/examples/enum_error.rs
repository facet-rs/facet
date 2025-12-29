use facet::Facet;
use facet_kdl_legacy as kdl;

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

#[derive(Debug, Default, Facet, PartialEq)]
pub struct Branch {
    #[facet(kdl::argument)]
    value: String,
}

fn main() {
    let input = r#"
trigger "git-push" {
    branch "fix/*"
}
"#;

    match facet_kdl_legacy::from_str::<EnumDocument>(input) {
        Ok(doc) => {
            println!("Unexpected success: {:#?}", doc);
        }
        Err(err) => {
            println!("Error (as expected):\n{}", err);
        }
    }
}
