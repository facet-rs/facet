use facet::Facet;
use facet_core::{Type, UserType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Status {
    id: u64,
    #[serde(default)]
    #[facet(default)]
    in_reply_to_status_id: Option<u64>,
}

fn main() {
    let shape = Status::SHAPE;
    println!("Shape type: {:?}", shape.ty);

    if let Type::User(UserType::Struct(struct_type)) = shape.ty {
        for field in struct_type.fields {
            println!(
                "Field '{}': has_default={}",
                field.name,
                field.has_default()
            );
        }
    }

    // Try to deserialize
    let toml_str = r#"
id = 123
"#;

    match facet_toml::from_str::<Status>(toml_str) {
        Ok(s) => println!("Success: {:?}", s),
        Err(e) => println!("Error: {}", e),
    }
}
