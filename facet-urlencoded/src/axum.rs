//! Axum integration for `Form<T>` and `Query<T>`.
//!
//! This module provides implementations of Axum's extractor traits
//! for URL-encoded form data and query parameters.
//!
//! # Example
//!
//! ```ignore
//! use axum::{routing::get, routing::post, Router};
//! use facet::Facet;
//! use facet_urlencoded::{Form, Query};
//!
//! #[derive(Debug, Facet)]
//! struct SearchParams {
//!     q: String,
//!     page: u64,
//! }
//!
//! #[derive(Debug, Facet)]
//! struct LoginForm {
//!     username: String,
//!     password: String,
//! }
//!
//! async fn search(Query(params): Query<SearchParams>) -> String {
//!     format!("Searching for '{}' on page {}", params.q, params.page)
//! }
//!
//! async fn login(Form(form): Form<LoginForm>) -> String {
//!     format!("Logging in user: {}", form.username)
//! }
//!
//! let app = Router::new()
//!     .route("/search", get(search))
//!     .route("/login", post(login));
//! ```

use crate::{Form, Query};
use axum_core::{
    extract::{FromRequest, FromRequestParts, Request},
    response::{IntoResponse, Response},
};
use facet_core::Facet;
use http::{StatusCode, header, request::Parts};
use http_body_util::BodyExt;
use std::fmt;

/// Rejection type for form extraction errors.
#[derive(Debug)]
pub struct FormRejection {
    kind: FormRejectionKind,
}

#[derive(Debug)]
enum FormRejectionKind {
    /// Failed to buffer the request body.
    BodyError(axum_core::Error),
    /// Failed to deserialize the form data.
    DeserializeError(crate::UrlEncodedError),
    /// Invalid UTF-8 in request body.
    InvalidUtf8,
    /// Missing or invalid `Content-Type` header.
    InvalidContentType,
}

impl FormRejection {
    /// Returns the status code for this rejection.
    pub const fn status(&self) -> StatusCode {
        match &self.kind {
            FormRejectionKind::BodyError(_) => StatusCode::BAD_REQUEST,
            FormRejectionKind::DeserializeError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            FormRejectionKind::InvalidUtf8 => StatusCode::BAD_REQUEST,
            FormRejectionKind::InvalidContentType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        }
    }
}

impl fmt::Display for FormRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            FormRejectionKind::BodyError(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            FormRejectionKind::DeserializeError(err) => {
                write!(f, "Failed to deserialize form data: {err}")
            }
            FormRejectionKind::InvalidUtf8 => {
                write!(f, "Request body is not valid UTF-8")
            }
            FormRejectionKind::InvalidContentType => {
                write!(
                    f,
                    "Invalid `Content-Type` header: expected `application/x-www-form-urlencoded`"
                )
            }
        }
    }
}

impl std::error::Error for FormRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            FormRejectionKind::BodyError(err) => Some(err),
            FormRejectionKind::DeserializeError(err) => Some(err),
            FormRejectionKind::InvalidUtf8 => None,
            FormRejectionKind::InvalidContentType => None,
        }
    }
}

impl IntoResponse for FormRejection {
    fn into_response(self) -> Response {
        let body = self.to_string();
        let status = self.status();
        (status, body).into_response()
    }
}

impl From<axum_core::Error> for FormRejection {
    fn from(err: axum_core::Error) -> Self {
        FormRejection {
            kind: FormRejectionKind::BodyError(err),
        }
    }
}

impl From<crate::UrlEncodedError> for FormRejection {
    fn from(err: crate::UrlEncodedError) -> Self {
        FormRejection {
            kind: FormRejectionKind::DeserializeError(err),
        }
    }
}

/// Rejection type for query parameter extraction errors.
#[derive(Debug)]
pub struct QueryRejection {
    kind: QueryRejectionKind,
}

#[derive(Debug)]
enum QueryRejectionKind {
    /// Failed to deserialize the query parameters.
    DeserializeError(crate::UrlEncodedError),
}

impl QueryRejection {
    /// Returns the status code for this rejection.
    pub const fn status(&self) -> StatusCode {
        match &self.kind {
            QueryRejectionKind::DeserializeError(_) => StatusCode::BAD_REQUEST,
        }
    }
}

impl fmt::Display for QueryRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            QueryRejectionKind::DeserializeError(err) => {
                write!(f, "Failed to deserialize query parameters: {err}")
            }
        }
    }
}

impl std::error::Error for QueryRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            QueryRejectionKind::DeserializeError(err) => Some(err),
        }
    }
}

impl IntoResponse for QueryRejection {
    fn into_response(self) -> Response {
        let body = self.to_string();
        let status = self.status();
        (status, body).into_response()
    }
}

impl From<crate::UrlEncodedError> for QueryRejection {
    fn from(err: crate::UrlEncodedError) -> Self {
        QueryRejection {
            kind: QueryRejectionKind::DeserializeError(err),
        }
    }
}

/// Checks if the content type is form-urlencoded.
fn is_form_content_type(req: &Request) -> bool {
    let Some(content_type) = req.headers().get(header::CONTENT_TYPE) else {
        return false;
    };

    let Ok(content_type) = content_type.to_str() else {
        return false;
    };

    content_type.starts_with("application/x-www-form-urlencoded")
}

impl<T, S> FromRequest<S> for Form<T>
where
    T: Facet<'static>,
    S: Send + Sync,
{
    type Rejection = FormRejection;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        // Check content type
        if !is_form_content_type(&req) {
            return Err(FormRejection {
                kind: FormRejectionKind::InvalidContentType,
            });
        }

        // Read the body
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(axum_core::Error::new)?
            .to_bytes();

        // Convert to string
        let body_str = std::str::from_utf8(&bytes).map_err(|_| FormRejection {
            kind: FormRejectionKind::InvalidUtf8,
        })?;

        // Deserialize using from_str_owned to get an owned value
        let value: T = crate::from_str_owned(body_str)?;

        Ok(Form(value))
    }
}

impl<T, S> FromRequestParts<S> for Query<T>
where
    T: Facet<'static>,
    S: Send + Sync,
{
    type Rejection = QueryRejection;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or_default();
        // Deserialize using from_str_owned to get an owned value
        let value: T = crate::from_str_owned(query)?;
        Ok(Query(value))
    }
}
