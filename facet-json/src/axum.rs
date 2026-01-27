//! Axum integration for JSON format.
//!
//! This module provides the `Json<T>` extractor and response type for axum.
//!
//! # Example
//!
//! ```ignore
//! use axum::{Router, routing::post};
//! use facet::Facet;
//! use facet_json::Json;
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
//! async fn create_user(Json(payload): Json<CreateUser>) -> Json<User> {
//!     let user = User {
//!         id: 1,
//!         name: payload.name,
//!         email: payload.email,
//!     };
//!     Json(user)
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

use crate::DeserializeError;

/// A wrapper type for JSON-encoded request/response bodies.
///
/// This type implements `FromRequest` for extracting JSON-encoded data from
/// request bodies, and `IntoResponse` for serializing data as JSON in responses.
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Consume the wrapper and return the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for Json<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

/// Rejection type for JSON extraction errors.
#[derive(Debug)]
pub struct JsonRejection {
    kind: JsonRejectionKind,
}

#[derive(Debug)]
enum JsonRejectionKind {
    /// Failed to read the request body.
    Body(axum_core::Error),
    /// Failed to deserialize the JSON data.
    Deserialize(DeserializeError),
    /// Missing `Content-Type: application/json` header.
    MissingContentType,
    /// Invalid `Content-Type` header (not application/json).
    InvalidContentType,
}

impl JsonRejection {
    /// Returns the HTTP status code for this rejection.
    pub const fn status(&self) -> StatusCode {
        match &self.kind {
            JsonRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            JsonRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
            JsonRejectionKind::MissingContentType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            JsonRejectionKind::InvalidContentType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        }
    }

    /// Returns true if this is a body reading error.
    pub const fn is_body_error(&self) -> bool {
        matches!(&self.kind, JsonRejectionKind::Body(_))
    }

    /// Returns true if this is a deserialization error.
    pub const fn is_deserialize_error(&self) -> bool {
        matches!(&self.kind, JsonRejectionKind::Deserialize(_))
    }

    /// Returns true if this is a missing content type error.
    pub const fn is_missing_content_type(&self) -> bool {
        matches!(&self.kind, JsonRejectionKind::MissingContentType)
    }

    /// Returns true if this is an invalid content type error.
    pub const fn is_invalid_content_type(&self) -> bool {
        matches!(&self.kind, JsonRejectionKind::InvalidContentType)
    }
}

impl fmt::Display for JsonRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            JsonRejectionKind::Body(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            JsonRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize JSON: {err}")
            }
            JsonRejectionKind::MissingContentType => {
                write!(f, "Missing `Content-Type: application/json` header")
            }
            JsonRejectionKind::InvalidContentType => {
                write!(
                    f,
                    "Invalid `Content-Type` header: expected `application/json`"
                )
            }
        }
    }
}

impl std::error::Error for JsonRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            JsonRejectionKind::Body(err) => Some(err),
            JsonRejectionKind::Deserialize(err) => Some(err),
            JsonRejectionKind::MissingContentType => None,
            JsonRejectionKind::InvalidContentType => None,
        }
    }
}

impl IntoResponse for JsonRejection {
    fn into_response(self) -> Response {
        (self.status(), self.to_string()).into_response()
    }
}

/// Checks if the content type is JSON.
fn is_json_content_type(req: &Request) -> bool {
    let Some(content_type) = req.headers().get(header::CONTENT_TYPE) else {
        return false;
    };

    let Ok(content_type) = content_type.to_str() else {
        return false;
    };

    let mime = content_type.parse::<mime::Mime>();
    match mime {
        Ok(mime) => {
            mime.type_() == mime::APPLICATION
                && (mime.subtype() == mime::JSON || mime.suffix() == Some(mime::JSON))
        }
        Err(_) => false,
    }
}

impl<T, S> FromRequest<S> for Json<T>
where
    T: Facet<'static>,
    S: Send + Sync,
{
    type Rejection = JsonRejection;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        // Check content type
        if !is_json_content_type(&req) {
            if req.headers().get(header::CONTENT_TYPE).is_none() {
                return Err(JsonRejection {
                    kind: JsonRejectionKind::MissingContentType,
                });
            }
            return Err(JsonRejection {
                kind: JsonRejectionKind::InvalidContentType,
            });
        }

        // Read the body
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| JsonRejection {
                kind: JsonRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        // Deserialize using from_slice to get an owned value
        let value: T = crate::from_slice(&bytes).map_err(|e| JsonRejection {
            kind: JsonRejectionKind::Deserialize(e),
        })?;

        Ok(Json(value))
    }
}

impl<T> IntoResponse for Json<T>
where
    T: Facet<'static>,
{
    fn into_response(self) -> Response {
        match crate::to_vec(&self.0) {
            Ok(bytes) => {
                let mut res = Response::new(Body::from(bytes));
                res.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
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
