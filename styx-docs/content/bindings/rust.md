+++
title = "Rust"
weight = 1
slug = "rust"
insert_anchor_links = "heading"
+++

How Rust types map to STYX syntax. Examples use [facet](https://github.com/facet-rs/facet) derives.

## Implicit root object

STYX documents are implicitly objects. When parsing, the top-level content is wrapped in an implicit root object. This means you cannot parse a bare tagged value directly into an enum — you need a wrapper struct.

```compare
/// rust
// This struct wraps the enum value
#[derive(Facet)]
struct Doc {
    status: Status,
}

#[derive(Facet)]
enum Status {
    Ok,
    Pending,
}

// Parse: "status @ok"
let doc: Doc = from_str("status @ok")?;
assert_eq!(doc.status, Status::Ok);
/// styx
status @ok
```

If you need to parse a document where the root *is* the tagged value, use an explicit root object:

```styx
{@ @ok}
```

This creates an object with a unit key (`@`) whose value is the `@ok` tag.

## Structs

```compare
/// rust
#[derive(Facet)]
struct Server {
    host: String,
    port: u16,
}
/// styx
host localhost
port 8080
```

## Optional fields (absent)

```compare
/// rust
#[derive(Facet)]
struct Config {
    timeout: Option<Duration>,
}

let c = Config { timeout: None };
/// styx
{ }
```

## Optional fields (present)

```compare
/// rust
#[derive(Facet)]
struct Config {
    timeout: Option<Duration>,
}

let c = Config {
    timeout: Some(Duration::from_secs(30)),
};
/// styx
{timeout 30s}
```

## Sequences

```compare
/// rust
#[derive(Facet)]
struct Doc {
    tags: Vec<String>,
}

let d = Doc {
    tags: vec![
        "web".into(),
        "prod".into(),
    ],
};
/// styx
tags (web prod)
```

## Maps

```compare
/// rust
#[derive(Facet)]
struct Doc {
    env: HashMap<String, String>,
}

let d = Doc {
    env: HashMap::from([
        ("HOME".into(), "/home/user".into()),
        ("PATH".into(), "/usr/bin".into()),
    ]),
};
/// styx
env {
    HOME /home/user
    PATH /usr/bin
}
```

## Nested structs

```compare
/// rust
#[derive(Facet)]
struct Server {
    host: String,
    tls: TlsConfig,
}

#[derive(Facet)]
struct TlsConfig {
    cert: String,
    key: String,
}
/// styx
host localhost
tls {
    cert /path/cert.pem
    key /path/key.pem
}
```

## Flatten

```compare
/// rust
#[derive(Facet)]
struct User {
    name: String,
    email: String,
}

#[derive(Facet)]
struct Admin {
    #[facet(flatten)]
    user: User,
    perms: Vec<String>,
}
/// styx
name Alice
email alice@example.com
perms (read write)
```

## Unit enum variants

Enum variants with no payload use implicit unit (`@variant` is shorthand for `@variant@`).

```compare
/// rust
#[derive(Facet)]
struct Doc {
    status: Status,
}

#[derive(Facet)]
enum Status {
    Ok,
    Pending,
}

let d = Doc { status: Status::Ok };
/// styx
status @ok
```

## Struct enum variants

Struct variants use `@variant{...}` syntax with the variant's fields inside.

```compare
/// rust
#[derive(Facet)]
struct Doc {
    result: MyResult,
}

#[derive(Facet)]
enum MyResult {
    Err { message: String },
}

let d = Doc {
    result: MyResult::Err {
        message: "timeout".into(),
    },
};
/// styx
result @err{message "timeout"}
```

## Tuple enum variants

Tuple variants use `@variant(...)` syntax. Note: parentheses create a *sequence*, so each tuple element is a sequence element.

```compare
/// rust
#[derive(Facet)]
struct Doc {
    color: Color,
}

#[derive(Facet)]
enum Color {
    Rgb(u8, u8, u8),
}

let d = Doc {
    color: Color::Rgb(255, 128, 0),
};
/// styx
color @rgb(255 128 0)
```

## Catch-all variants with `#[facet(other)]`

For extensible enums, mark a variant with `#[facet(other)]` to catch unknown tags.
Use `#[facet(tag)]` and `#[facet(content)]` on fields to capture the tag name and payload.

```compare
/// rust
#[derive(Facet)]
struct Doc {
    schema: Schema,
}

#[derive(Facet)]
enum Schema {
    // Known variant
    Object { fields: Vec<String> },
    // Catch-all for unknown type references
    #[facet(other)]
    Type {
        #[facet(tag)]
        name: String,
    },
}

// @string doesn't match any known variant,
// so it's captured as Type { name: "string" }
let d: Doc = from_str("schema @string")?;
/// styx
schema @string
```

## Durations

```compare
/// rust
use std::time::Duration;

let d = Duration::from_secs(30);
/// styx
30s
```

## Bytes (hex)

```compare
/// rust
let bytes: Vec<u8> = vec![
    0xde, 0xad, 0xbe, 0xef,
];
/// styx
deadbeef
```
