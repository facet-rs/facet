

<h1>
<picture>
<source srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/logo-only.webp">
<img src="https://github.com/facet-rs/facet/raw/main/static/logo-v2/logo-only.png" height="35" alt="Facet logo - a reflection library for Rust">
</picture> &nbsp; facet-json
</h1>

[![experimental](https://img.shields.io/badge/status-experimental-yellow)](https://github.com/fasterthanlime/facet)
[![free of syn](https://img.shields.io/badge/free%20of-syn-hotpink)](https://github.com/fasterthanlime/free-of-syn)
[![crates.io](https://img.shields.io/crates/v/facet-json.svg)](https://crates.io/crates/facet-json)
[![documentation](https://docs.rs/facet-json/badge.svg)](https://docs.rs/facet-json)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-json.svg)](./LICENSE)

_Logo by [Misiasart](https://misiasart.com/)_

Thanks to all individual and corporate sponsors, without whom this work could not exist:

<p> <a href="https://ko-fi.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/ko-fi-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/ko-fi-light.svg" height="40" alt="Ko-fi">
    </picture>
</a> <a href="https://github.com/sponsors/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/github-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/github-light.svg" height="40" alt="GitHub Sponsors">
    </picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/patreon-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> <a href="https://zed.dev">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-light.svg" height="40" alt="Zed">
    </picture>
</a> </p>
             

JSON serialization and deserialization for [facet](https://crates.io/crates/facet).

## Usage

### Serialization Example

```rust
use facet::Facet;
use facet_json::to_json_string;
use facet_poke::Peek;

#[derive(facet::Facet)]
struct Person {
    name: String,
    age: u32,
}

fn main() {
    // Create a struct to serialize
    let person = Person {
        name: "Alice".to_string(),
        age: 30,
    };

    // Create a Peek object from the struct
    let peek = Peek::new(&person);

    // Serialize to JSON (true = pretty-print)
    let json = to_json_string(peek, true);

    println!("{}", json);
    // Output:
    // {
    //   "name": "Alice",
    //   "age": 30
    // }
}
```

### Deserialization Example

```rust
use facet::Facet;
use facet_json::from_str;

#[derive(facet::Facet, Debug)]
struct Person {
    name: String,
    age: u32,
}

fn main() {
    // JSON string to deserialize
    let json = r#"{"name":"Bob","age":25}"#;

    // Deserialize the JSON to a Person struct
    let person: Person = from_str(json).unwrap();

    println!("{:?}", person);
    // Output: Person { name: "Bob", age: 25 }
}
```


## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.