//! Axum integration for MsgPack format.
//!
//! This module provides the `MsgPack<T>` extractor and response type for axum.
//!
//! # Example
//!
//! ```ignore
//! use axum::{Router, routing::post};
//! use facet::Facet;
//! use facet_msgpack::MsgPack;
//!
//! #[derive(Facet)]
//! struct Point {
//!     x: i32,
//!     y: i32,
//! }
//!
//! async fn transform(MsgPack(point): MsgPack<Point>) -> MsgPack<Point> {
//!     MsgPack(Point { x: point.x * 2, y: point.y * 2 })
//! }
//!
//! let app = Router::new().route("/transform", post(transform));
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

use crate::{DeserializeError, MsgPackSerializeError};

/// A wrapper type for MsgPack-encoded request/response bodies.
///
/// This type implements `FromRequest` for extracting MsgPack-encoded data from
/// request bodies, and `IntoResponse` for serializing data as MsgPack in responses.
#[derive(Debug, Clone, Copy, Default)]
pub struct MsgPack<T>(pub T);

impl<T> MsgPack<T> {
    /// Consume the wrapper and return the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for MsgPack<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for MsgPack<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for MsgPack<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

/// Rejection type for MsgPack extraction errors.
#[derive(Debug)]
pub struct MsgPackRejection {
    kind: MsgPackRejectionKind,
}

#[derive(Debug)]
enum MsgPackRejectionKind {
    /// Failed to read the request body.
    Body(axum_core::Error),
    /// Failed to deserialize the MsgPack data.
    Deserialize(DeserializeError),
}

impl MsgPackRejection {
    /// Returns the HTTP status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            MsgPackRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            MsgPackRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    /// Returns true if this is a body reading error.
    pub fn is_body_error(&self) -> bool {
        matches!(&self.kind, MsgPackRejectionKind::Body(_))
    }

    /// Returns true if this is a deserialization error.
    pub fn is_deserialize_error(&self) -> bool {
        matches!(&self.kind, MsgPackRejectionKind::Deserialize(_))
    }
}

impl fmt::Display for MsgPackRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            MsgPackRejectionKind::Body(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            MsgPackRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize MsgPack: {err}")
            }
        }
    }
}

impl std::error::Error for MsgPackRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            MsgPackRejectionKind::Body(err) => Some(err),
            MsgPackRejectionKind::Deserialize(err) => Some(err),
        }
    }
}

impl IntoResponse for MsgPackRejection {
    fn into_response(self) -> Response {
        (self.status(), self.to_string()).into_response()
    }
}

impl<T, S> FromRequest<S> for MsgPack<T>
where
    T: Facet<'static>,
    S: Send + Sync,
{
    type Rejection = MsgPackRejection;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        // Read the body
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| MsgPackRejection {
                kind: MsgPackRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        // Deserialize
        let value: T = crate::from_slice(&bytes).map_err(|e| MsgPackRejection {
            kind: MsgPackRejectionKind::Deserialize(e),
        })?;

        Ok(MsgPack(value))
    }
}

impl<T> IntoResponse for MsgPack<T>
where
    T: Facet<'static>,
{
    fn into_response(self) -> Response {
        match crate::to_vec(&self.0) {
            Ok(bytes) => {
                let mut res = Response::new(Body::from(bytes));
                res.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/msgpack"),
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

/// Rejection type for MsgPack serialization errors in responses.
#[derive(Debug)]
pub struct MsgPackSerializeRejection(pub MsgPackSerializeError);

impl fmt::Display for MsgPackSerializeRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to serialize MsgPack response: {}", self.0)
    }
}

impl std::error::Error for MsgPackSerializeRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl IntoResponse for MsgPackSerializeRejection {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}
