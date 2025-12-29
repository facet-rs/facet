//! Axum integration for KDL format.
//!
//! This module provides the `Kdl<T>` extractor and response type for axum.
//!
//! # Example
//!
//! ```ignore
//! use axum::{Router, routing::post};
//! use facet::Facet;
//! use facet_format_kdl::Kdl;
//!
//! #[derive(Facet)]
//! struct Config {
//!     #[facet(kdl::property)]
//!     name: String,
//! }
//!
//! async fn update_config(Kdl(config): Kdl<Config>) -> Kdl<Config> {
//!     Kdl(config)
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

use crate::{DeserializeError, KdlError};

/// A wrapper type for KDL-encoded request/response bodies.
///
/// This type implements `FromRequest` for extracting KDL-encoded data from
/// request bodies, and `IntoResponse` for serializing data as KDL in responses.
#[derive(Debug, Clone, Copy, Default)]
pub struct Kdl<T>(pub T);

impl<T> Kdl<T> {
    /// Consume the wrapper and return the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Kdl<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Kdl<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for Kdl<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

/// Rejection type for KDL extraction errors.
#[derive(Debug)]
pub struct KdlRejection {
    kind: KdlRejectionKind,
}

#[derive(Debug)]
enum KdlRejectionKind {
    /// Failed to read the request body.
    Body(axum_core::Error),
    /// Failed to deserialize the KDL data.
    Deserialize(DeserializeError<KdlError>),
}

impl KdlRejection {
    /// Returns the HTTP status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            KdlRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            KdlRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    /// Returns true if this is a body reading error.
    pub fn is_body_error(&self) -> bool {
        matches!(&self.kind, KdlRejectionKind::Body(_))
    }

    /// Returns true if this is a deserialization error.
    pub fn is_deserialize_error(&self) -> bool {
        matches!(&self.kind, KdlRejectionKind::Deserialize(_))
    }
}

impl fmt::Display for KdlRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            KdlRejectionKind::Body(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            KdlRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize KDL: {err}")
            }
        }
    }
}

impl std::error::Error for KdlRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            KdlRejectionKind::Body(err) => Some(err),
            KdlRejectionKind::Deserialize(err) => Some(err),
        }
    }
}

impl IntoResponse for KdlRejection {
    fn into_response(self) -> Response {
        (self.status(), self.to_string()).into_response()
    }
}

impl<T, S> FromRequest<S> for Kdl<T>
where
    T: Facet<'static>,
    S: Send + Sync,
{
    type Rejection = KdlRejection;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        // Read the body
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| KdlRejection {
                kind: KdlRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        // Deserialize (from_slice handles UTF-8 validation)
        let value: T = crate::from_slice(&bytes).map_err(|e| KdlRejection {
            kind: KdlRejectionKind::Deserialize(e),
        })?;

        Ok(Kdl(value))
    }
}

impl<T> IntoResponse for Kdl<T>
where
    T: Facet<'static>,
{
    fn into_response(self) -> Response {
        match crate::to_string(&self.0) {
            Ok(s) => {
                let mut res = Response::new(Body::from(s));
                // KDL doesn't have an official MIME type, use text/plain or application/kdl
                res.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/kdl"),
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
