use facet::Facet;
use std::path::PathBuf;

fn main() {
    let shape = PathBuf::SHAPE;
    println!("PathBuf shape: {:?}", shape.type_identifier);
    println!("PathBuf has_display: {:?}", shape.vtable.has_display());
    println!("PathBuf has_debug: {:?}", shape.vtable.has_debug());

    let p = PathBuf::from("/tmp/test.log");
    match facet_json::to_string(&p) {
        Ok(s) => println!("Serialized PathBuf: {}", s),
        Err(e) => println!("Failed to serialize PathBuf: {:?}", e),
    }
}
