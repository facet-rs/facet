---
weight = 4
slug = "rust-bindings"
---

# Rust Bindings

This document shows how Rust types map to STYX syntax. The examples use
[facet](https://github.com/facet-rs/facet) for derive macros, but the
mappings apply to any STYX deserializer.

## Primitives

```compare
/// rust
let x: u32 = 42;
/// styx
42
```

```compare
/// rust
let s: String = "hello".into();
/// styx
"hello"
```

```compare
/// rust
let b: bool = true;
/// styx
true
```

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

```compare
/// rust
// Inline style
Server { host: "localhost".into(), port: 8080 }
/// styx
{ host localhost, port 8080 }
```

## Optional fields

```compare
/// rust
#[derive(Facet)]
struct Config {
    timeout: Option<Duration>,
}

// None
Config { timeout: None }
/// styx
// Absent
{ }
```

```compare
/// rust
// Some
Config { timeout: Some(Duration::from_secs(30)) }
/// styx
// Present
{ timeout 30s }
```

```compare
/// rust
// Explicit empty (if application distinguishes)
Config { timeout: ??? }
/// styx
// Unit value
{ timeout @ }
```

## Sequences

```compare
/// rust
let tags: Vec<String> = vec!["web", "prod"];
/// styx
(web prod)
```

```compare
/// rust
#[derive(Facet)]
struct App {
    tags: Vec<String>,
}
/// styx
tags (web prod)
```

## Maps

```compare
/// rust
let env: HashMap<String, String> = [
    ("HOME", "/home/user"),
    ("PATH", "/usr/bin"),
].into();
/// styx
{
  HOME /home/user
  PATH /usr/bin
}
```

```compare
/// rust
#[derive(Facet)]
struct Config {
    env: HashMap<String, String>,
}
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
struct Tls {
    cert: String,
    key: String,
}

#[derive(Facet)]
struct Server {
    host: String,
    tls: Tls,
}
/// styx
host localhost
tls {
  cert /path/to/cert.pem
  key /path/to/key.pem
}
```

## Flatten

Flattening merges fields from a nested struct into the parent's key-space.

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
    permissions: Vec<String>,
}
/// styx
// Document is flat
name Alice
email alice@example.com
permissions (read write admin)
```

The deserializer routes `name` and `email` to the nested `User`, then assigns it to `admin.user`.

## Enums

Unit variants:

```compare
/// rust
#[derive(Facet)]
enum Status {
    Ok,
    Pending,
}

let s = Status::Ok;
/// styx
@ok
```

Struct variants:

```compare
/// rust
#[derive(Facet)]
enum Result {
    Err { message: String },
}

let r = Result::Err { message: "timeout".into() };
/// styx
@err{ message "timeout" }
```

Tuple variants:

```compare
/// rust
#[derive(Facet)]
enum Data {
    Values(u32, u32, u32),
}

let d = Data::Values(1, 2, 3);
/// styx
@values(1 2 3)
```

Newtype variants:

```compare
/// rust
#[derive(Facet)]
enum Wrapper {
    Message(String),
}

let w = Wrapper::Message("hello".into());
/// styx
@message"hello"
```

## Special types

Duration:

```compare
/// rust
use std::time::Duration;

let d = Duration::from_secs(30);
/// styx
30s
```

```compare
/// rust
let d = Duration::from_millis(100);
/// styx
100ms
```

Bytes (hex):

```compare
/// rust
let bytes: Vec<u8> = vec![0xde, 0xad, 0xbe, 0xef];
/// styx
0xdeadbeef
```

Bytes (base64):

```compare
/// rust
let bytes: Vec<u8> = b"Hello".to_vec();
/// styx
b64"SGVsbG8="
```
