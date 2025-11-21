use facet::Facet;
use facet::Type;
use facet::UserType;

#[derive(Facet)]
#[repr(u32)]
enum Complex {
    /// This is a complex docstring with various escape sequences:
    /// 1. Double quotes: "quoted" and \"escaped quote\"
    /// 2. Single quotes: 'quoted' and \'escaped quote\'
    /// 3. Backslashes: \\ (single backslash) and \\\\ (double backslash)
    /// 4. Mixed: \\" (backslash quote) and \\\" (backslash escaped quote)
    /// 5. Trailing backslash: \\
    /// 6. ASCII symbols: ! " # $ % & ' ( ) * + , - . / 0 1 2 3 4 5 6 7 8 9 : ; < = > ? @ A B C D E F G H I J K L M N O P Q R S T U V W X Y Z [ \ ] ^ _ ` a b c d e f g h i j k l m n o p q r s t u v w x y z { | } ~
    #[expect(unused)]
    Variant,
}

fn main() {
    let Type::User(UserType::Enum(ty)) = Complex::SHAPE.ty else {
        unreachable!("Expected EntityId to be an enum");
    };
    let doc_lines = &ty.variants[0].doc;

    // Read own source code
    // file!() returns the path to the file. In the test environment, this is likely src/main.rs
    // and the CWD is the project root, so this should work.
    let source = std::fs::read_to_string(file!()).expect("Failed to read source file");
    let lines: Vec<&str> = source.lines().collect();

    let start_idx = lines
        .iter()
        .position(|l| l.contains("enum Complex {"))
        .expect("Could not find enum start")
        + 1;
    let end_idx = lines
        .iter()
        .position(|l| l.contains("#[expect"))
        .expect("Could not find enum end");

    let expected_lines: Vec<String> = lines[start_idx..end_idx]
        .iter()
        .filter(|l| l.trim().starts_with("///"))
        .map(|l| {
            // Split on "///" and take the second part to get the content including the leading space
            let parts: Vec<&str> = l.splitn(2, "///").collect();
            parts[1].to_string()
        })
        .collect();

    assert_eq!(doc_lines.len(), expected_lines.len(), "Line count mismatch");

    for (i, (actual, expected)) in doc_lines.iter().zip(expected_lines.iter()).enumerate() {
        assert_eq!(
            actual,
            expected,
            "Mismatch at line {}\nExpected: '{}'\nActual:   '{}'",
            i + 1,
            expected,
            actual
        );
    }
}
