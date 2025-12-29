//! Axum integration for XML format.
//!
//! This module provides the `Xml<T>` extractor and response type for axum.
//!
//! # Example
//!
//! ```ignore
//! use axum::{Router, routing::post};
//! use facet::Facet;
//! use facet_xml::Xml;
//!
//! #[derive(Facet)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! async fn create_person(Xml(person): Xml<Person>) -> Xml<Person> {
//!     Xml(person)
//! }
//!
//! let app = Router::new().route("/person", post(create_person));
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

use crate::{DeserializeError, XmlError};

/// A wrapper type for XML-encoded request/response bodies.
///
/// This type implements `FromRequest` for extracting XML-encoded data from
/// request bodies, and `IntoResponse` for serializing data as XML in responses.
#[derive(Debug, Clone, Copy, Default)]
pub struct Xml<T>(pub T);

impl<T> Xml<T> {
    /// Consume the wrapper and return the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Xml<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Xml<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for Xml<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

/// Rejection type for XML extraction errors.
#[derive(Debug)]
pub struct XmlRejection {
    kind: XmlRejectionKind,
}

#[derive(Debug)]
enum XmlRejectionKind {
    /// Failed to read the request body.
    Body(axum_core::Error),
    /// Failed to deserialize the XML data.
    Deserialize(DeserializeError<XmlError>),
}

impl XmlRejection {
    /// Returns the HTTP status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            XmlRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            XmlRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    /// Returns true if this is a body reading error.
    pub fn is_body_error(&self) -> bool {
        matches!(&self.kind, XmlRejectionKind::Body(_))
    }

    /// Returns true if this is a deserialization error.
    pub fn is_deserialize_error(&self) -> bool {
        matches!(&self.kind, XmlRejectionKind::Deserialize(_))
    }
}

impl fmt::Display for XmlRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            XmlRejectionKind::Body(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            XmlRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize XML: {err}")
            }
        }
    }
}

impl std::error::Error for XmlRejection {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            XmlRejectionKind::Body(err) => Some(err),
            XmlRejectionKind::Deserialize(err) => Some(err),
        }
    }
}

impl IntoResponse for XmlRejection {
    fn into_response(self) -> Response {
        (self.status(), self.to_string()).into_response()
    }
}

impl<T, S> FromRequest<S> for Xml<T>
where
    T: Facet<'static>,
    S: Send + Sync,
{
    type Rejection = XmlRejection;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        // Read the body
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| XmlRejection {
                kind: XmlRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        // Deserialize
        let value: T = crate::from_slice(&bytes).map_err(|e| XmlRejection {
            kind: XmlRejectionKind::Deserialize(e),
        })?;

        Ok(Xml(value))
    }
}

impl<T> IntoResponse for Xml<T>
where
    T: Facet<'static>,
{
    fn into_response(self) -> Response {
        match crate::to_vec(&self.0) {
            Ok(bytes) => {
                let mut res = Response::new(Body::from(bytes));
                res.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/xml"),
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
