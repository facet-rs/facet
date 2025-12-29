//! Axum integration for postcard format.
//!
//! This module provides the `Postcard<T>` extractor and response type for axum.
//!
//! # Example
//!
//! ```ignore
//! use axum::{Router, routing::post};
//! use facet::Facet;
//! use facet_postcard::Postcard;
//!
//! #[derive(Facet)]
//! struct CreateUser {
//!     name: String,
//!     email: String,
//! }
//!
//! #[derive(Facet)]
//! struct User {
//!     id: u64,
//!     name: String,
//!     email: String,
//! }
//!
//! async fn create_user(Postcard(payload): Postcard<CreateUser>) -> Postcard<User> {
//!     let user = User {
//!         id: 1,
//!         name: payload.name,
//!         email: payload.email,
//!     };
//!     Postcard(user)
//! }
//!
//! let app = Router::new().route("/users", post(create_user));
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

use crate::{DeserializeError, PostcardError, SerializeError};

/// A wrapper type for postcard-encoded request/response bodies.
///
/// This type implements `FromRequest` for extracting postcard-encoded data from
/// request bodies, and `IntoResponse` for serializing data as postcard in responses.
#[derive(Debug, Clone, Copy, Default)]
pub struct Postcard<T>(pub T);

impl<T> Postcard<T> {
    /// Consume the wrapper and return the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Postcard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Postcard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for Postcard<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

/// Rejection type for Postcard extraction errors.
#[derive(Debug)]
pub struct PostcardRejection {
    kind: PostcardRejectionKind,
}

#[derive(Debug)]
enum PostcardRejectionKind {
    /// Failed to read the request body.
    Body(axum_core::Error),
    /// Failed to deserialize the postcard data.
    Deserialize(DeserializeError<PostcardError>),
}

impl PostcardRejection {
    /// Returns the HTTP status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            PostcardRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            PostcardRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    /// Returns true if this is a body reading error.
    pub fn is_body_error(&self) -> bool {
        matches!(&self.kind, PostcardRejectionKind::Body(_))
    }

    /// Returns true if this is a deserialization error.
    pub fn is_deserialize_error(&self) -> bool {
        matches!(&self.kind, PostcardRejectionKind::Deserialize(_))
    }
}

impl fmt::Display for PostcardRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            PostcardRejectionKind::Body(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            PostcardRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize Postcard: {err}")
            }
        }
    }
}

impl std::error::Error for PostcardRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            PostcardRejectionKind::Body(err) => Some(err),
            PostcardRejectionKind::Deserialize(err) => Some(err),
        }
    }
}

impl IntoResponse for PostcardRejection {
    fn into_response(self) -> Response {
        (self.status(), self.to_string()).into_response()
    }
}

impl<T, S> FromRequest<S> for Postcard<T>
where
    T: for<'de> Facet<'de>,
    S: Send + Sync,
{
    type Rejection = PostcardRejection;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| PostcardRejection {
                kind: PostcardRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        let value: T = crate::from_slice(&bytes).map_err(|e| PostcardRejection {
            kind: PostcardRejectionKind::Deserialize(e),
        })?;

        Ok(Postcard(value))
    }
}

impl<T> IntoResponse for Postcard<T>
where
    T: for<'de> Facet<'de>,
{
    fn into_response(self) -> Response {
        match crate::to_vec(&self.0) {
            Ok(bytes) => {
                let mut res = Response::new(Body::from(bytes));
                res.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
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

/// Rejection type for Postcard serialization errors in responses.
#[derive(Debug)]
pub struct PostcardSerializeRejection(pub SerializeError);

impl fmt::Display for PostcardSerializeRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to serialize Postcard response: {}", self.0)
    }
}

impl std::error::Error for PostcardSerializeRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl IntoResponse for PostcardSerializeRejection {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}
