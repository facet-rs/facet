use facet::Facet;
use facet::Type;
use facet::UserType;

#[repr(u32)]
#[derive(Facet)]
enum Bruh {
    /// Joe loves \bold{pizza}
    /// Five backslashes: \\\\\
    /// Six backslashes: \\\\\\
    /// Five backslashes and a quote: \\\\\"
    /// Six backslashes and a quote: \\\\\\"
    #[expect(unused)]
    Joe,
}
fn main() {
    let Type::User(UserType::Enum(ty)) = Bruh::SHAPE.ty else {
        unreachable!("Expected EntityId to be an enum");
    };
    let doc = &ty.variants[0].doc;
    assert_eq!(doc[0], r#" Joe loves \bold{pizza}"#);
    assert_eq!(doc[1], r#" Five backslashes: \\\\\"#);
    assert_eq!(doc[2], r#" Six backslashes: \\\\\\"#);
    assert_eq!(doc[3], r#" Five backslashes and a quote: \\\\\""#);
    assert_eq!(doc[4], r#" Six backslashes and a quote: \\\\\\""#);
}
