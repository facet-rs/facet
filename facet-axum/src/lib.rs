//! Axum integration for Facet.
//!
//! This crate provides Axum extractors and response types that use Facet's
//! serialization instead of serde. This allows you to use Facet-derived types
//! directly in your Axum handlers without needing serde derives.
//!
//! # Features
//!
//! - `json` (default): Enables `Json<T>` extractor/response using `facet-json`
//! - `form` (default): Enables `Form<T>` and `Query<T>` extractors using `facet-urlencoded`
//!
//! # Example
//!
//! ```ignore
//! use axum::{routing::{get, post}, Router};
//! use facet::Facet;
//! use facet_axum::{Json, Query};
//!
//! #[derive(Debug, Facet)]
//! struct CreateUser {
//!     name: String,
//!     email: String,
//! }
//!
//! #[derive(Debug, Facet)]
//! struct User {
//!     id: u64,
//!     name: String,
//!     email: String,
//! }
//!
//! #[derive(Debug, Facet)]
//! struct SearchParams {
//!     q: String,
//!     page: u64,
//! }
//!
//! async fn create_user(Json(payload): Json<CreateUser>) -> Json<User> {
//!     Json(User {
//!         id: 1,
//!         name: payload.name,
//!         email: payload.email,
//!     })
//! }
//!
//! async fn search(Query(params): Query<SearchParams>) -> String {
//!     format!("Searching for '{}' on page {}", params.q, params.page)
//! }
//!
//! let app = Router::new()
//!     .route("/users", post(create_user))
//!     .route("/search", get(search));
//! ```

#![warn(missing_docs)]

// Re-export JSON types
#[cfg(feature = "json")]
pub use facet_json::{Json, JsonRejection};

// Re-export form/query types
#[cfg(feature = "form")]
pub use facet_urlencoded::{Form, FormRejection, Query, QueryRejection};

// Re-export YAML types
#[cfg(feature = "yaml")]
pub use facet_yaml::{Yaml, YamlRejection};

// Re-export TOML types
#[cfg(feature = "toml")]
pub use facet_toml::{Toml, TomlRejection};

// Re-export XML types
#[cfg(feature = "xml")]
pub use facet_xml::{Xml, XmlRejection};

// Re-export MessagePack types
#[cfg(feature = "msgpack")]
pub use facet_msgpack::{MsgPack, MsgPackRejection};

// Re-export Postcard types
#[cfg(feature = "postcard")]
pub use facet_postcard::{Postcard, PostcardRejection};
