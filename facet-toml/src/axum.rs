//! Axum integration for TOML format.
//!
//! This module provides the `Toml<T>` extractor and response type for axum.
//!
//! # Example
//!
//! ```ignore
//! use axum::{Router, routing::post};
//! use facet::Facet;
//! use facet_toml::Toml;
//!
//! #[derive(Facet)]
//! struct Config {
//!     name: String,
//!     port: u16,
//! }
//!
//! #[derive(Facet)]
//! struct ConfigResponse {
//!     success: bool,
//!     name: String,
//! }
//!
//! async fn update_config(Toml(config): Toml<Config>) -> Toml<ConfigResponse> {
//!     Toml(ConfigResponse {
//!         success: true,
//!         name: config.name,
//!     })
//! }
//!
//! let app = Router::new().route("/config", post(update_config));
//! ```

use axum_core::{
    body::Body,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
};
use core::fmt;
use core::ops::{Deref, DerefMut};
use facet_core::Facet;
use http::{HeaderValue, StatusCode, header};
use http_body_util::BodyExt;

use crate::{DeserializeError, TomlError};

/// A wrapper type for TOML-encoded request/response bodies.
///
/// This type implements `FromRequest` for extracting TOML-encoded data from
/// request bodies, and `IntoResponse` for serializing data as TOML in responses.
#[derive(Debug, Clone, Copy, Default)]
pub struct Toml<T>(pub T);

impl<T> Toml<T> {
    /// Consume the wrapper and return the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Toml<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Toml<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for Toml<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

/// Rejection type for TOML extraction errors.
#[derive(Debug)]
pub struct TomlRejection {
    kind: TomlRejectionKind,
}

#[derive(Debug)]
enum TomlRejectionKind {
    /// Failed to read the request body.
    Body(axum_core::Error),
    /// Failed to deserialize the TOML data.
    Deserialize(DeserializeError),
}

impl TomlRejection {
    /// Returns the HTTP status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            TomlRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            TomlRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    /// Returns true if this is a body reading error.
    pub fn is_body_error(&self) -> bool {
        matches!(&self.kind, TomlRejectionKind::Body(_))
    }

    /// Returns true if this is a deserialization error.
    pub fn is_deserialize_error(&self) -> bool {
        matches!(&self.kind, TomlRejectionKind::Deserialize(_))
    }
}

impl fmt::Display for TomlRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            TomlRejectionKind::Body(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            TomlRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize TOML: {err}")
            }
        }
    }
}

impl std::error::Error for TomlRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            TomlRejectionKind::Body(err) => Some(err),
            TomlRejectionKind::Deserialize(err) => Some(err),
        }
    }
}

impl IntoResponse for TomlRejection {
    fn into_response(self) -> Response {
        (self.status(), self.to_string()).into_response()
    }
}

impl<T, S> FromRequest<S> for Toml<T>
where
    T: Facet<'static>,
    S: Send + Sync,
{
    type Rejection = TomlRejection;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        // Read the body
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| TomlRejection {
                kind: TomlRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        // Deserialize (from_slice handles UTF-8 validation)
        let value: T = crate::from_slice(&bytes).map_err(|e| TomlRejection {
            kind: TomlRejectionKind::Deserialize(e),
        })?;

        Ok(Toml(value))
    }
}

impl<T> IntoResponse for Toml<T>
where
    T: Facet<'static>,
{
    fn into_response(self) -> Response {
        match crate::to_string(&self.0) {
            Ok(s) => {
                let mut res = Response::new(Body::from(s));
                res.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/toml"),
                );
                res
            }
            Err(err) => {
                let body = format!("Failed to serialize response: {err}");
                (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
            }
        }
    }
}
