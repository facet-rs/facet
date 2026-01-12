+++
title = "Rust Bindings"
weight = 4
slug = "rust-bindings"
insert_anchor_links = "heading"
+++

How Rust types map to STYX syntax. Examples use [facet](https://github.com/facet-rs/facet) derives.

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
{ timeout 30s }
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

## Struct enum variants

```compare
/// rust
#[derive(Facet)]
enum Result {
    Err { message: String },
}

let r = Result::Err {
    message: "timeout".into(),
};
/// styx
@err{ message "timeout" }
```

## Tuple enum variants

```compare
/// rust
#[derive(Facet)]
enum Color {
    Rgb(u8, u8, u8),
}

let c = Color::Rgb(255, 128, 0);
/// styx
@rgb(255 128 0)
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
