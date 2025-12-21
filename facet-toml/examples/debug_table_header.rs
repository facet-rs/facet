use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
#[facet(untagged)]
enum Dependency {
    Version(String),
    Workspace(WorkspaceDependency),
    Detailed(DependencyDetail),
}

#[derive(Facet, Debug, PartialEq)]
struct WorkspaceDependency {
    workspace: bool,
    features: Option<Vec<String>>,
}

#[derive(Facet, Debug, PartialEq)]
struct DependencyDetail {
    path: Option<String>,
    features: Option<Vec<String>>,
    version: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct Dependencies {
    #[facet(default)]
    deps: std::collections::HashMap<String, Dependency>,
}

fn main() {
    // Test 1: Inline table (works)
    let toml1 = r#"
[deps]
backtrace = { path = "../..", features = ["std"] }
dioxus = { workspace = true, features = ["router"] }
"#;

    println!("=== Test 1: Inline table syntax ===");
    match facet_toml::from_str::<Dependencies>(toml1) {
        Ok(deps) => {
            println!("✓ Success!");
            for (name, dep) in deps.deps {
                println!("  {}: {:?}", name, dep);
            }
        }
        Err(e) => {
            eprintln!("✗ Failed: {}", e);
        }
    }

    // Test 2: Table header (fails)
    let toml2 = r#"
[deps.backtrace]
path = "../.."
features = ["std"]

[deps.dioxus]
workspace = true
features = ["router"]
"#;

    println!("\n=== Test 2: Table header syntax ===");
    match facet_toml::from_str::<Dependencies>(toml2) {
        Ok(deps) => {
            println!("✓ Success!");
            for (name, dep) in deps.deps {
                println!("  {}: {:?}", name, dep);
            }
        }
        Err(e) => {
            eprintln!("✗ Failed: {}", e);
        }
    }
}
