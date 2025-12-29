fn main() {
    // Test escape sequences
    let inputs = [
        r#"node "hello\bworld""#,
        r#"node "hello\fworld""#,
        r#"node "\u{0008}""#, // backspace as unicode
        r#"node "\u{000C}""#, // form feed as unicode
    ];

    for input in inputs {
        println!("Testing: {}", input);
        match input.parse::<kdl::KdlDocument>() {
            Ok(doc) => {
                let node = &doc.nodes()[0];
                if !node.entries().is_empty()
                    && let kdl::KdlValue::String(s) = node.entries()[0].value()
                {
                    println!("  Parsed OK: {:?} (bytes: {:?})", s, s.as_bytes());
                }
            }
            Err(e) => {
                println!("  Parse error: {}", e);
            }
        }
    }
}
