#[derive(Facet, Debug)]
struct DependencySpec {
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::argument)]
    version: String,
    #[facet(kdl::property, default)]
    features: Option<Vec<String>>,
}

#[derive(Facet, Debug)]
struct Config {
    #[facet(kdl::children)]
    dependencies: Vec<DependencySpec>,
}

fn main() {
    let kdl = r#"
    dependencies {
        serde "1.0"
        tokio "1.0" features=["full"]
    }
    "#;

    println!("Testing KDL parsing...");
    let result: Result<Config, _> = facet_kdl_legacy::from_str(kdl);
    match result {
        Ok(config) => println!("Success: {:#?}", config),
        Err(e) => println!("Error: {}", e),
    }
}
