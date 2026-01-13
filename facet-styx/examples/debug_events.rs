use facet_format::FormatParser;

fn main() {
    // Compare both formats with exact matching field names
    println!("=== Explicit braces ===");
    let source1 = "value { optional (hello) }";
    let mut parser1 = facet_styx::StyxParser::new(source1);

    println!("Parsing: {source1}");
    println!("---");
    loop {
        match parser1.next_event() {
            Ok(Some(event)) => println!("Event: {event:?}"),
            Ok(None) => {
                println!("Done");
                break;
            }
            Err(e) => {
                println!("Error: {e:?}");
                break;
            }
        }
    }

    println!("\n=== Tag syntax ===");
    let source2 = "value @optional(hello)";
    let mut parser2 = facet_styx::StyxParser::new(source2);

    println!("Parsing: {source2}");
    println!("---");
    loop {
        match parser2.next_event() {
            Ok(Some(event)) => println!("Event: {event:?}"),
            Ok(None) => {
                println!("Done");
                break;
            }
            Err(e) => {
                println!("Error: {e:?}");
                break;
            }
        }
    }
}
