//! Axum integration for YAML format.
//!
//! This module provides the `Yaml<T>` extractor and response type for axum.
//!
//! # Example
//!
//! ```ignore
//! use axum::{Router, routing::post};
//! use facet::Facet;
//! use facet_yaml::Yaml;
//!
//! #[derive(Facet)]
//! struct Config {
//!     name: String,
//!     port: u16,
//! }
//!
//! async fn update_config(Yaml(config): Yaml<Config>) -> Yaml<Config> {
//!     Yaml(config)
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

use crate::{DeserializeError, YamlError};

/// A wrapper type for YAML-encoded request/response bodies.
///
/// This type implements `FromRequest` for extracting YAML-encoded data from
/// request bodies, and `IntoResponse` for serializing data as YAML in responses.
#[derive(Debug, Clone, Copy, Default)]
pub struct Yaml<T>(pub T);

impl<T> Yaml<T> {
    /// Consume the wrapper and return the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Yaml<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Yaml<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for Yaml<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

/// Rejection type for YAML extraction errors.
#[derive(Debug)]
pub struct YamlRejection {
    kind: YamlRejectionKind,
}

#[derive(Debug)]
enum YamlRejectionKind {
    /// Failed to read the request body.
    Body(axum_core::Error),
    /// Failed to deserialize the YAML data.
    Deserialize(DeserializeError<YamlError>),
}

impl YamlRejection {
    /// Returns the HTTP status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            YamlRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            YamlRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    /// Returns true if this is a body reading error.
    pub fn is_body_error(&self) -> bool {
        matches!(&self.kind, YamlRejectionKind::Body(_))
    }

    /// Returns true if this is a deserialization error.
    pub fn is_deserialize_error(&self) -> bool {
        matches!(&self.kind, YamlRejectionKind::Deserialize(_))
    }
}

impl fmt::Display for YamlRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            YamlRejectionKind::Body(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            YamlRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize YAML: {err}")
            }
        }
    }
}

impl std::error::Error for YamlRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            YamlRejectionKind::Body(err) => Some(err),
            YamlRejectionKind::Deserialize(err) => Some(err),
        }
    }
}

impl IntoResponse for YamlRejection {
    fn into_response(self) -> Response {
        (self.status(), self.to_string()).into_response()
    }
}

impl<T, S> FromRequest<S> for Yaml<T>
where
    T: Facet<'static>,
    S: Send + Sync,
{
    type Rejection = YamlRejection;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        // Read the body
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| YamlRejection {
                kind: YamlRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        // Deserialize (from_slice handles UTF-8 validation)
        let value: T = crate::from_slice(&bytes).map_err(|e| YamlRejection {
            kind: YamlRejectionKind::Deserialize(e),
        })?;

        Ok(Yaml(value))
    }
}

impl<T> IntoResponse for Yaml<T>
where
    T: Facet<'static>,
{
    fn into_response(self) -> Response {
        match crate::to_string(&self.0) {
            Ok(s) => {
                let mut res = Response::new(Body::from(s));
                res.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/yaml"),
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
