# rapace-http

[![crates.io](https://img.shields.io/crates/v/rapace-http.svg)](https://crates.io/crates/rapace-http)
[![documentation](https://docs.rs/rapace-http/badge.svg)](https://docs.rs/rapace-http)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-http.svg)](./LICENSE)

HTTP types and service trait for rapace RPC.

This crate provides transport-agnostic HTTP types that can be serialized via facet and sent over any rapace transport.

## Types

- **HttpRequest**: An HTTP request with method, path, headers, and body
- **HttpResponse**: An HTTP response with status, headers, and body
- **HttpService**: A trait for handling HTTP requests

## Architecture

This crate is intentionally minimal and has no HTTP framework dependencies. The types are designed to be easily converted to/from framework-specific types (axum, hyper, etc.) on either side of the RPC boundary.

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                              HOST PROCESS                               │
│  ┌─────────────┐       ┌──────────────────┐       ┌─────────────────┐  │
│  │ HTTP Server │──────►│ HttpRequest      │──────►│ HttpServiceClient│  │
│  │  (hyper)    │       │ (rapace types)   │       │  (RPC call)     │  │
│  └─────────────┘       └──────────────────┘       └────────┬────────┘  │
│                                                             │           │
└─────────────────────────────────────────────────────────────┼───────────┘
                                  rapace transport            │
┌─────────────────────────────────────────────────────────────┼───────────┐
│                                                             ▼           │
│                             PLUGIN PROCESS                              │
│  ┌─────────────────┐       ┌──────────────────┐       ┌─────────────┐  │
│  │ HttpServiceServer│◄──────│ HttpRequest      │◄──────│ axum Router │  │
│  │  (dispatcher)   │       │ (rapace types)   │       │             │  │
│  └─────────────────┘       └──────────────────┘       └─────────────┘  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Example

```rust
use rapace_http::{HttpRequest, HttpResponse, HttpService};

struct MyHttpHandler;

impl HttpService for MyHttpHandler {
    async fn handle(&self, req: HttpRequest) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
            body: b"Hello, World!".to_vec(),
        }
    }
}
```

## License

MIT OR Apache-2.0
