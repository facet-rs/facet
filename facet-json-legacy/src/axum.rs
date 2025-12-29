//! Axum integration for `Json<T>`.
//!
//! This module provides implementations of Axum's `FromRequest` and `IntoResponse`
//! traits for the `Json<T>` wrapper type, enabling seamless use of Facet types
//! in Axum handlers.
//!
//! # Example
//!
//! ```ignore
//! use axum::{routing::post, Router};
//! use facet::Facet;
//! use facet_json_legacy::Json;
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
//! async fn create_user(Json(payload): Json<CreateUser>) -> Json<User> {
//!     Json(User {
//!         id: 1,
//!         name: payload.name,
//!         email: payload.email,
//!     })
//! }
//!
//! let app = Router::new().route("/users", post(create_user));
//! ```

use crate::Json;
use axum_core::{
    body::Body,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
};
use facet_core::Facet;
use http::{HeaderValue, StatusCode, header};
use http_body_util::BodyExt;
use std::fmt;

/// Rejection type for JSON extraction errors.
///
/// This is returned when the `Json` extractor fails to parse the request body.
#[derive(Debug)]
pub struct JsonRejection {
    kind: JsonRejectionKind,
}

#[derive(Debug)]
enum JsonRejectionKind {
    /// Failed to buffer the request body.
    BodyError(axum_core::Error),
    /// Failed to deserialize the JSON body.
    DeserializeError(crate::JsonError),
    /// Missing `Content-Type: application/json` header.
    MissingContentType,
    /// Invalid `Content-Type` header (not application/json).
    InvalidContentType,
}

impl JsonRejection {
    /// Returns the status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            JsonRejectionKind::BodyError(_) => StatusCode::BAD_REQUEST,
            JsonRejectionKind::DeserializeError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            JsonRejectionKind::MissingContentType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            JsonRejectionKind::InvalidContentType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        }
    }
}

impl fmt::Display for JsonRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            JsonRejectionKind::BodyError(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            JsonRejectionKind::DeserializeError(err) => {
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
            JsonRejectionKind::BodyError(err) => Some(err),
            JsonRejectionKind::DeserializeError(err) => Some(err),
            JsonRejectionKind::MissingContentType => None,
            JsonRejectionKind::InvalidContentType => None,
        }
    }
}

impl IntoResponse for JsonRejection {
    fn into_response(self) -> Response {
        let body = self.to_string();
        let status = self.status();
        (status, body).into_response()
    }
}

impl From<axum_core::Error> for JsonRejection {
    fn from(err: axum_core::Error) -> Self {
        JsonRejection {
            kind: JsonRejectionKind::BodyError(err),
        }
    }
}

impl From<crate::JsonError> for JsonRejection {
    fn from(err: crate::JsonError) -> Self {
        JsonRejection {
            kind: JsonRejectionKind::DeserializeError(err),
        }
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
            .map_err(axum_core::Error::new)?
            .to_bytes();

        // Deserialize using from_slice to get an owned value
        let value: T = crate::from_slice(&bytes)?;

        Ok(Json(value))
    }
}

impl<T> IntoResponse for Json<T>
where
    T: Facet<'static>,
{
    fn into_response(self) -> Response {
        // Serialize to JSON string
        let json_string = crate::to_string(&self.0);
        let mut res = Response::new(Body::from(json_string));
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        res
    }
}
