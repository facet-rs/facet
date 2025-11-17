use facet::Facet;
use facet::Type;
use facet::UserType;

#[repr(u32)]
#[derive(Facet)]
enum Bruh {
    /// Welcome to Joe's!
    #[expect(unused)]
    Joes,
}
fn main() {
    let Type::User(UserType::Enum(ty)) = Bruh::SHAPE.ty else {
        unreachable!("Expected EntityId to be an enum");
    };
    let doc = ty.variants[0].doc[0];
    assert!(doc == "Welcome to Joe's!", "Unexpected docstring, does it contain any unexpected escape sequences? : {}", doc);
}