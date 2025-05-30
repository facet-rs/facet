use facet::Facet;
use facet_json::to_string;
use time::OffsetDateTime;

#[derive(Facet, Debug, PartialEq)]
struct FooBar {
    created_at: OffsetDateTime,
}

fn main() {
    use time::macros::datetime;

    let value = FooBar {
        created_at: datetime!(2023-01-15 12:34:56 UTC),
    };

    eprintln!("About to serialize...");
    let json = to_string(&value);
    eprintln!("Result: {}", json);
    eprintln!("Expected: {}", r#"{"created_at":"2023-01-15T12:34:56Z"}"#);
}
