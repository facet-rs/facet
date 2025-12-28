fn main() {
    let input = "42";
    match input.parse::<kdl::KdlDocument>() {
        Ok(doc) => {
            println!("Parsed successfully: {:?}", doc);
        }
        Err(e) => {
            println!("Parse error: {}", e);
        }
    }

    // Try with node name
    let input2 = "value 42";
    match input2.parse::<kdl::KdlDocument>() {
        Ok(doc) => {
            println!("Parsed 'value 42' successfully: {:?}", doc);
            for node in doc.nodes() {
                println!("  Node: {} with {} args", node.name(), node.entries().len());
            }
        }
        Err(e) => {
            println!("Parse error: {}", e);
        }
    }
}
