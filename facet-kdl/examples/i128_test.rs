fn main() {
    // Test large integer parsing
    let inputs = [
        r#"record { val 170141183460469231731687303715884105728 }"#,
        r#"record { val "170141183460469231731687303715884105728" }"#,
        r#"record val=170141183460469231731687303715884105728"#,
        r#"record val="170141183460469231731687303715884105728""#,
    ];

    for input in inputs {
        println!("Testing: {}", input);
        match input.parse::<kdl::KdlDocument>() {
            Ok(doc) => {
                let node = &doc.nodes()[0];
                if !node.entries().is_empty() {
                    println!("  Parsed OK: {:?}", node.entries()[0].value());
                } else {
                    println!("  Parsed OK but no entries");
                }
            }
            Err(e) => {
                println!("  Parse error: {}", e);
            }
        }
    }
}
