use facet::Facet;
use facet_kdl as kdl;

#[derive(Debug, Facet, PartialEq)]
struct Authors {
    #[facet(kdl::argument)]
    value: String,
}

#[derive(Debug, Facet, PartialEq)]
struct Repo {
    #[facet(kdl::argument)]
    value: String,
}

#[derive(Debug, Facet, PartialEq)]
#[facet(rename_all = "kebab-case")]
struct CrateConfig {
    #[facet(kdl::child)]
    repo: Repo,
    #[facet(kdl::child)]
    #[facet(default)]
    authors: Option<Authors>,
}

fn main() {
    // Test WITHOUT authors
    let kdl = r#"repo "https://example.com""#;

    match facet_kdl::from_str::<CrateConfig>(kdl) {
        Ok(config) => {
            println!("Success! {:?}", config);
            assert!(config.authors.is_none());
        }
        Err(e) => {
            println!("Error: {}", e);
            std::process::exit(1);
        }
    }

    println!("Test passed!");
}
