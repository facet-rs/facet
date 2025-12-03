+++
title = "The Derive Macro"
weight = 5
insert_anchor_links = "heading"
+++

## How It Works

The `#[derive(Facet)]` macro:

1. Parses the type definition using [unsynn](https://docs.rs/unsynn)
2. Collects field information, attributes, and doc comments
3. Generates a `Facet` impl with a `SHAPE` constant
4. Processes `#[facet(...)]` attributes (both built-in and extension)

## Generated Code

For this input:

```rust
#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
}
```

The macro generates (simplified):

```rust
unsafe impl Facet<'_> for Person {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(value_vtable!(Person, |f, _| write!(f, "Person")))
            .type_identifier("Person")
            .ty(Type::User(UserType::Struct(StructType {
                kind: StructKind::Named,
                fields: &[
                    Field {
                        name: "name",
                        shape: || <String as Facet>::SHAPE,
                        offset: offset_of!(Person, name),
                        // ...
                    },
                    Field {
                        name: "age",
                        shape: || <u32 as Facet>::SHAPE,
                        offset: offset_of!(Person, age),
                        // ...
                    },
                ],
                // ...
            })))
            .build()
    };
}
```

## Extension Attributes

The derive macro supports namespaced extension attributes like `#[facet(kdl::property)]`. See the [Extend guide](/extend/) for details.
