+++
title = "Cargo.toml"
weight = 2
slug = "cargo"
insert_anchor_links = "heading"
+++

A Rust Cargo.toml in TOML vs Styx.

```compare
/// toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2024"
authors = ["Alice <alice@example.com>"]
description = "A sample application"
license = "MIT"
repository = "https://github.com/example/my-app"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }
anyhow = "1.0"
tracing = "0.1"
facet.workspace = true

[dependencies.reqwest]
version = "0.11"
features = ["json", "rustls-tls"]
default-features = false

[dev-dependencies]
insta = "1.0"
criterion = { version = "0.5", features = ["html_reports"] }

[features]
default = ["logging"]
logging = ["tracing"]
full = ["logging", "metrics"]

[[bin]]
name = "my-app"
path = "src/main.rs"

[profile.release]
lto = true
codegen-units = 1
strip = true
/// styx
package {
  name my-app
  version 0.1.0
  edition 2024
  authors ("Alice <alice@example.com>")
  description "A sample application"
  license MIT
  repository https://github.com/example/my-app
}

dependencies {
  serde version>1.0 features>(derive)
  tokio version>1.0 features>(full)
  anyhow 1.0
  tracing 0.1
  facet.workspace true
  reqwest {
    version 0.11
    features (json rustls-tls)
    default-features false
  }
}

dev-dependencies {
  insta 1.0
  criterion version>0.5 features>(html_reports)
}

features {
  default (logging)
  logging (tracing)
  full (logging metrics)
}

bin ({
  name my-app
  path src/main.rs
})

profile.release {
  lto true
  codegen-units 1
  strip true
}
```
