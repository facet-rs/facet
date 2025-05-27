use facet_derive_emit::*;
use rust_format::{Formatter, RustFmt};

fn expand(input: &str) -> String {
    // Trim surrounding whitespace which might interfere with parsing,
    // especially when dealing with multi-line raw strings.
    let trimmed_input = input.trim();
    match trimmed_input.parse() {
        Ok(ts) => RustFmt::default()
            .format_tokens(facet_derive(ts))
            .unwrap_or_else(|e| panic!("Expand error: {e}")),
        Err(e) => panic!("Failed to parse input:\n{}\nError: {}", trimmed_input, e),
    }
}

#[test]
fn unit_struct() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct UnitStruct;
        "#
    ));
}

#[test]
fn tuple_struct() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct TupleStruct(i32, String);
        "#
    ));
}

#[test]
fn simple_struct() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct Blah {
            foo: u32,
            bar: String,
        }
        "#
    ));
}

#[test]
fn enum_with_variants() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(u8)]
        enum EnumWithVariants {
            Variant1,
            Variant2(i32),
            Variant3 { field1: i32, field2: String },
        }
        "#
    ));
}

#[test]
fn enum_with_discriminants_decimal() {
    insta::assert_snapshot!(expand(
        r#"
        #[repr(u8)]
        #[derive(Facet)]
        enum Test {
          Red,
          Blue = 7,
          Green,
          Yellow = 10,
        }
        "#
    ));
}

#[test]
fn enum_with_discriminants_hexadecimal() {
    insta::assert_snapshot!(expand(
        r#"
        #[repr(u16)]
        #[derive(Facet)]
        enum Color {
          Red      = 0x01,
          Blue     = 0x7F,
          Green    = 0x80,
          Yellow   = 0x10,
          Magenta  = 0xfeed,
          Cyan     = 0xBEEF,
        }
        "#
    ));
}

#[test]
fn enum_with_discriminants_binary() {
    insta::assert_snapshot!(expand(
        r#"
        #[repr(u8)]
        #[derive(Facet)]
        enum BitFlags {
          None = 0b0000_0000,
          Read = 0b0000_0001,
          Write = 0b0000_0010,
          Execute = 0b0000_0100,
          All = 0b0000_0111,
        }
        "#
    ));
}

#[test]
fn enum_with_discriminants_mixed_notations() {
    insta::assert_snapshot!(expand(
        r#"
        #[repr(u32)]
        #[derive(Facet)]
        enum Status {
            Ok = 1,
            Warn = 0xA,
            Error = 0b1111,
            Timeout = 0o77,
        }
        "#
    ));
}

#[test]
fn repr_c_enum() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(C)]
        enum EnumWithVariants {
            /// Comment A
            Variant1,
            /// Comment B
            Variant2(i32),
            Variant3 { field1: i32, field2: String },
        }
        "#
    ));
}

#[test]
fn repr_c_enum_empty_struct_variant() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(C)]
        enum EnumWithEmptyStructVariant {
            Variant1 { },
        }
        "#
    ));
}

#[test]
fn repr_c_enum_lifetime_field() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(C)]
        enum EnumWithLifetimeField {
            Variant1 { field1: &'static str },
        }
        "#
    ));
}

#[test]
fn struct_with_generics_simple() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct StructWithGenericsSimple<T, U> {
            field1: T,
            field2: U,
        }
        "#
    ));
}

#[test]
fn struct_with_sensitive_field() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct Blah {
            foo: u32,
            #[facet(sensitive)]
            bar: String,
        }
        "#
    ));
}

#[test]
fn struct_repr_c() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(C)]
        struct Blah {
            foo: u32,
            bar: String,
        }
        "#
    ));
}

#[test]
fn struct_doc_comment() {
    insta::assert_snapshot!(expand(
        r#"
        /// yes
        #[derive(Facet)]
        struct Foo {}
        "#
    ));
}

#[test]
fn struct_doc_comment_multi_line() {
    insta::assert_snapshot!(expand(
        r#"
        /// yes
        /// no
        #[derive(Facet)]
        struct Foo {}
        "#
    ));
}

#[test]
fn struct_doc_comment_unicode() {
    insta::assert_snapshot!(expand(
        r#"
        /// yes 😄
        /// no
        #[derive(Facet)]
        struct Foo {}
        "#
    ));
}

#[test]
fn struct_doc_comment_quotes() {
    insta::assert_snapshot!(expand(
        r#"
        /// what about "quotes"
        #[derive(Facet)]
        struct Foo {}
        "#
    ));
}

#[test]
fn enum_doc_comment() {
    insta::assert_snapshot!(expand(
        r#"
        /// This is an enum
        #[derive(Facet)]
        #[repr(u8)]
        enum MyEnum {
            #[allow(dead_code)]
            A,
            #[allow(dead_code)]
            B,
        }
        "#
    ));
}

#[test]
fn struct_field_doc_comment() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct Foo {
            /// This field has a doc comment
            bar: u32,
        }
        "#
    ));
}

#[test]
fn tuple_struct_field_doc_comment() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct MyTupleStruct(
            /// This is a documented field
            u32,
            /// This is another documented field
            String,
        );
        "#
    ));
}

#[test]
fn enum_variants_with_comments() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(u8)]
        enum CommentedEnum {
            /// This is variant A
            #[allow(dead_code)]
            A,
            /// This is variant B
            /// with multiple lines
            #[allow(dead_code)]
            B(u32),
            /// This is variant C
            /// which has named fields
            #[allow(dead_code)]
            C {
                /// This is field x
                x: u32,
                /// This is field y
                y: String,
            },
        }
        "#
    ));
}

#[test]
fn struct_with_pub_field() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct Foo {
            /// This is a public field
            pub bar: u32,
        }
        "#
    ));
}

#[test]
fn tuple_struct_repr_transparent() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(transparent)]
        struct Blah(u32);
        "#
    ));
}

#[test]
fn tuple_struct_doc_comment() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(transparent)]
        /// This is a struct for sure
        struct Blah(u32);
        "#
    ));
}

#[test]
fn tuple_struct_field_doc_comment_repr_transparent() {
    // Note: This test is similar to tuple_struct_field_doc_comment, but adds repr(transparent)
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(transparent)]
        /// This is a struct for sure
        struct Blah(
            /// and this is a field
            u32,
        );
        "#
    ));
}

#[test]
fn record_struct_generic() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct Blah<'a, T: Facet + core::hash::Hash, const C: usize = 3>
        where
            T: Debug, // Added a Debug bound for demonstration if needed, adjust as per Facet constraints
        {
            field: core::marker::PhantomData<&'a T>,
            another: T,
            constant_val: [u8; C],
        }
        "#
    ));
}

#[test]
fn tuple_struct_generic() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(transparent)]
        struct Blah<'a, T: Facet + core::hash::Hash, const C: usize = 3>(T, core::marker::PhantomData<&'a [u8; C]>)
        where
            T: Debug; // Added a Debug bound for demonstration
        "#
    ));
}

#[test]
fn unit_struct_generic() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct Blah<const C: usize = 3>
        where
             [u8; C]: Debug; // Example bound using the const generic
        "#
    ));
}

#[test]
fn enum_generic() {
    insta::assert_snapshot!(expand(
        r#"
        #[allow(dead_code)]
        #[derive(Facet)]
        #[repr(u8)]
        enum E<'a, T: Facet<'a> + core::hash::Hash, const C: usize = 3>
        where
            T: Debug, // Added Debug bound
             [u8; C]: Debug, // Added Debug bound
        {
            Unit,
            Tuple(T, core::marker::PhantomData<&'a [u8; C]>),
            Record {
                field: T,
                phantom: core::marker::PhantomData<&'a ()>,
                constant_val: [u8; C],
            },
        }
        "#
    ));
}

#[test]
fn tuple_struct_with_pub_fields() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        /// This is a struct for sure
        struct Blah(
            /// and this is a public field
            pub u32,
            /// and this is a crate public field
            pub(crate) u32,
        );
        "#
    ));
}

#[test]
fn cfg_attrs() {
    // Note: The effectiveness of this test depends on the features enabled when running tests.
    // The generated code might differ based on `feature = "testfeat"`.
    // Snapshot testing might need feature-specific snapshots.
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[cfg_attr(feature = "testfeat", derive(Debug))]
        #[cfg_attr(feature = "testfeat", facet(deny_unknown_fields))]
        pub struct CubConfig {}
        "#
    ));
}

#[test]
fn cfg_attrs_on_field() {
    // Similar note as cfg_attrs regarding features.
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[cfg_attr(feature = "testfeat", derive(Debug))]
        #[cfg_attr(feature = "testfeat", facet(deny_unknown_fields))]
        pub struct CubConfig {
            /// size the disk cache is allowed to use
            #[cfg_attr(feature = "testfeat", facet(skip_serializing))]
            #[cfg_attr(
                feature = "testfeat",
                facet(default = "serde_defaults::default_disk_cache_size")
            )]
            pub disk_cache_size: String,
        }
        "#
    ));
}

#[test]
fn struct_with_std_string() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct FileInfo {
            path: std::string::String, // Explicitly use std::string::String
            size: u64,
        }
        "#
    ));
}

#[test]
fn derive_real_life_cub_config() {
    // Similar note as cfg_attrs regarding features.
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[cfg_attr(feature = "testfeat", facet(deny_unknown_fields))]
        pub struct CubConfig {
            /// size the disk cache is allowed to use
            #[cfg_attr(feature = "testfeat", facet(skip_serializing))]
            #[cfg_attr(
                feature = "testfeat",
                facet(default = "serde_defaults::default_disk_cache_size")
            )]
            pub disk_cache_size: String,

            /// Listen address without http, something like "127.0.0.1:1111"
            #[cfg_attr(feature = "testfeat", facet(default = "serde_defaults::address"))]
            pub address: std::string::String,

            /// Something like `http://localhost:1118`
            /// or `http://mom.svc.cluster.local:1118`, never
            /// a trailing slash.
            #[cfg_attr(feature = "testfeat", facet(default = "serde_defaults::mom_base_url"))]
            pub mom_base_url: String,

            /// API key used to talk to mom
            #[cfg_attr(feature = "testfeat", facet(default = "serde_defaults::mom_api_key"))]
            #[cfg_attr(feature = "testfeat", facet(sensitive))] // Example addition
            pub mom_api_key: String,
        }
        "#
    ));
}

#[test]
fn macroed_type() {
    // Testing derive inside a macro requires careful handling of the input string.
    // We expand the macro manually here for the test input.
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Debug, Facet, PartialEq)]
        struct Macroed {
            // NOTICE type is variable here
            value: u32,
        }
        "#
    ));
}

#[test]
fn array_field_in_enum() {
    insta::assert_snapshot!(expand(
        r#"
        /// Network packet types
        #[derive(Facet)]
        #[repr(u8)]
        pub enum Packet {
            /// Array of bytes representing the header
            Header([u8; 4]),
            Payload(Vec<u8>), // Add another variant for completeness
        }
        "#
    ));
}

#[test]
fn array_field_in_struct() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        pub struct DataPacket {
            header: [u8; 16],
            payload: Vec<u8>,
            metadata: [MetadataTag; 4],
        }
        "#
    ));
}

#[test]
fn struct_impls_drop() {
    // The derive should still work even if the struct implements Drop.
    // The Drop impl itself is not part of the derive input.
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct BarFoo {
            bar: u32,
            foo: String,
        }
        // The Drop impl doesn't affect the derive macro input:
        // impl Drop for BarFoo { fn drop(&mut self) {} }
        "#
    ));
}

#[test]
fn opaque_arc() {
    // Need to ensure the derive handles `#[facet(opaque)]` correctly.
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        pub struct Handle(#[facet(opaque)] std::sync::Arc<NotDerivingFacet>);
        "#
    ));
}

#[test]
fn struct_with_facet_attributes() {
    // Test various facet attributes together
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(name = "MyCoolStruct", deny_unknown_fields, version = 2, type_tag = "rs.facet.MyCoolStruct")]
        struct StructWithAttributes {
            #[facet(name = "identifier", default = generate_id, sensitive)]
            id: String,
            #[facet(skip, version = 3)]
            internal_data: Vec<u8>,
            #[facet(deprecated = "Use 'new_value' instead")]
            old_value: i32,
            new_value: i32,
        }
        "#
    ));
}

#[test]
fn enum_with_facet_attributes() {
    // Test various facet attributes on enums and variants
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(name = "MyCoolEnum", repr = "u16", type_tag = "rs.facet.MyCoolEnum")]
        #[repr(u16)] // Ensure repr matches if specified in facet attribute
        enum EnumWithAttributes {
            #[facet(name = "FirstVariant", discriminant = 10)]
            VariantA,

            #[facet(skip)]
            InternalVariant(i32),

            #[facet(deprecated = "Use VariantD instead")]
            VariantC {
                #[facet(sensitive)]
                secret: String
            },

            VariantD {
                 #[facet(default = forty_two())]
                 value: i32
            },
        }
        "#
    ));
}

// Keep the original struct_with_defaults test if the `=` syntax is supported
// Otherwise, replace or remove it if only `#[facet(default = ...)]` is supported.
// Assuming the derive supports `= default` for now.
#[test]
fn struct_with_equal_defaults() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct StructWithDefaults {
            field1: i32 = 42,
            field2: String = "default".to_string(),
        }
        "#
    ));
}

#[test]
fn struct_with_field_default_facets() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(default)]
        struct ForFacetDefaultDemo {
            #[facet(default)]
            field1: u32,
            #[facet(default = my_field_default_fn())]
            field2: String,
            field3: bool,
        }
        "#
    ));
}

#[test]
fn generic_bounds_t() {
    insta::assert_snapshot!(expand(
        r#"
        struct Foo<T> where T: Copy {
            inner: Vec<T>,
        }
        "#
    ));
}

#[test]
fn generic_bounds_k_v() {
    insta::assert_snapshot!(expand(
        r#"
        struct Foo<K, V> where K: Eq + Hash {
            inner: HashMap<K, V>,
        }
        "#
    ));
}

#[test]
fn generic_bounds_tuple_t() {
    insta::assert_snapshot!(expand(
        r#"
        struct Foo<T>(Vec<T>);
        "#
    ));
}

#[test]
fn enum_with_nested_generic_in_variant_one_level() {
    // This test has a generic type nested one layer inside an enum variant.
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(u8)]
        enum OneLevelNested<T> {
            VariantA(Result<T, String>),
            VariantB(Option<T>),
            // Also include a unit variant to check un-nested
            Plain,
        }
        "#
    ));
}

#[test]
fn enum_with_nested_generic_in_variant_two_levels() {
    // This test has a generic type nested two layers inside an enum variant.
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(u8)]
        enum DeeplyNested<T> {
            LevelOne(
                Option<
                    Result<
                        Vec<T>,
                        String
                    >
                >
            ),
            // Another variant to prove non-nested still works.
            Simple(T),
        }
        "#
    ));
}

#[test]
fn struct_with_renamed_field() {
    // Test a struct with snake_case fields and camelCase rename attributes
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct Person {
            #[facet(rename = "firstName")]
            first_name: String,
            #[facet(rename = "lastName")]
            last_name: String,
            age: u32,
        }
        "#
    ));
}

#[test]
fn struct_with_rename_all() {
    // Test a struct with rename_all attribute using different case conventions
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(rename_all = "camelCase")]
        struct PersonInfo {
            first_name: String,
            last_name: String,
            home_address: String,
            phone_number: u32,
        }
        "#
    ));
}

#[test]
fn enum_with_rename_all() {
    // Test an enum with rename_all attribute
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(u8)]
        #[facet(rename_all = "snake_case")]
        enum ApiResponse {
            OkResponse {
                #[facet(rename = "responseData")]
                data: String,
            },
            ErrorResponse {
                code: u32,
                message: String,
            },
        }
        "#
    ));
}

#[test]
fn struct_with_mixed_rename_attributes() {
    // Test struct with a mix of rename_all and individual rename attributes
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(rename_all = "snake_case")]
        struct ConfigSettings {
            server_url: String,
            #[facet(rename = "apiKey")]
            api_key: String,
            timeout_secs: u32,
            max_retry_count: u8,
        }
        "#
    ));
}

#[test]
fn rename_all_snake_case() {
    // Test snake_case conversion
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(rename_all = "snake_case")]
        struct SnakeCaseExample {
            field_one: String,
            field_two: String,
        }
        "#
    ));
}

#[test]
fn rename_all_screaming_snake_case() {
    // Test SCREAMING_SNAKE_CASE conversion
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(rename_all = "SCREAMING_SNAKE_CASE")]
        struct UpperCaseExample {
            field_one: String,
            field_two: String,
        }
        "#
    ));
}

#[test]
fn rename_all_pascalcase() {
    // Test PascalCase conversion
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(rename_all = "PascalCase")]
        struct PascalCaseExample {
            field_one: String,
            field_two: String,
        }
        "#
    ));
}

#[test]
fn rename_all_camelcase() {
    // Test camelCase conversion
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(rename_all = "camelCase")]
        struct CamelCaseExample {
            field_one: String,
            field_two: String,
        }
        "#
    ));
}

#[test]
fn rename_all_snakecase() {
    // Test snake_case conversion
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(rename_all = "snake_case")]
        struct SnakeCaseExample {
            fieldOne: String, // Note the camelCase input field name
            fieldTwo: String,
        }
        "#
    ));
}

#[test]
fn rename_all_screaming_snakecase() {
    // Test SCREAMING_SNAKE_CASE conversion
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(rename_all = "SCREAMING_SNAKE_CASE")]
        struct ScreamingSnakeCaseExample {
            field_one: String,
            field_two: String,
        }
        "#
    ));
}

#[test]
fn rename_all_kebabcase() {
    // Test kebab-case conversion
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(rename_all = "kebab-case")]
        struct KebabCaseExample {
            field_one: String,
            field_two: String,
        }
        "#
    ));
}

#[test]
fn rename_all_screaming_kebabcase() {
    // Test SCREAMING-KEBAB-CASE conversion
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(rename_all = "SCREAMING-KEBAB-CASE")]
        struct ScreamingKebabCaseExample {
            field_one: String,
            field_two: String,
        }
        "#
    ));
}

#[test]
fn tuple_struct_with_renamed_field() {
    // Test a tuple struct with positional fields that use rename attributes to give descriptive names
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct Point(
            #[facet(rename = "x_coordinate")]
            f32,
            #[facet(rename = "y_coordinate")]
            f32,
            #[facet(rename = "z_coordinate")]
            f32,
        );
        "#
    ));
}

#[test]
fn enum_with_renamed_variants_and_fields() {
    // Test an enum with renamed variants and fields, converting from Rust-idiomatic names to API-style names
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(u8)]
        enum ApiResponse {
            #[facet(rename = "Success")]
            Ok {
                #[facet(rename = "responseData")]
                data: String,
            },
            #[facet(rename = "Error")]
            Err {
                #[facet(rename = "errorCode")]
                code: u32,
                #[facet(rename = "errorMessage")]
                message: String,
            },
        }
        "#
    ));
}

#[test]
fn mixed_rename_and_sensitive_attributes() {
    // Test combining rename attributes with sensitive attributes on the same fields
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct User {
            #[facet(rename = "userName")]
            name: String,
            #[facet(rename = "userEmail", sensitive)]
            email: String,
            #[facet(sensitive)]
            password: String,
        }
        "#
    ));
}

#[test]
fn enum_with_multiple_attributes_per_variant() {
    // Test an enum with rename attributes on variants and fields, including mixed attributes
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(u8)]
        enum ConfigValue {
            #[facet(rename = "TextValue")]
            Text(String),
            #[facet(rename = "NumberValue")]
            Number {
                #[facet(rename = "numValue")]
                value: f64,
                #[facet(rename = "unitName", sensitive)]
                unit: String,
            },
            #[facet(rename = "BoolValue")]
            Boolean(bool),
        }
        "#
    ));
}

#[test]
fn visibility() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        pub struct Test<T> {
            pub(crate) a: T,
        }
        "#
    ));
}

#[test]
fn struct_facet_transparent() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[facet(transparent)]
        struct Wrapper(u32);
        "#
    ));
}

#[test]
fn enum_with_macro_discriminants() {
    insta::assert_snapshot!(expand(
        r#"
        #[repr(u16)]
        #[derive(Facet)]
        enum TestEnum {
            Value1 = test_macro!(1, 2),
            Value2 = test_macro!(3, 4),
        }
        "#
    ));
}

#[test]
fn struct_with_option_cow_str() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        struct Foo<'a> {
            #[facet(default)]
            name: Option<Cow<'a, str>>,
        }
        "#
    ));
}

#[test]
fn pub_crate_enum() {
    insta::assert_snapshot!(expand(
        r#"
        #[derive(Facet)]
        #[repr(u8)]
        pub(crate) enum LogLevel {
            Debug,
            Info,
            Warn,
            Error,
        }
        "#
    ));
}
