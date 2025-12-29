//! Axum integration for `Toml<T>`.

use crate::Toml;
use axum_core::{
    body::Body,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
};
use core::fmt;
use facet_core::Facet;
use http::{HeaderValue, StatusCode, header};
use http_body_util::BodyExt;

/// Rejection type for TOML extraction errors.
#[derive(Debug)]
pub struct TomlRejection {
    kind: TomlRejectionKind,
}

#[derive(Debug)]
enum TomlRejectionKind {
    Body(axum_core::Error),
    Deserialize(String),
    InvalidUtf8,
}

impl TomlRejection {
    /// Returns the status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            TomlRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            TomlRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
            TomlRejectionKind::InvalidUtf8 => StatusCode::BAD_REQUEST,
        }
    }
}

impl fmt::Display for TomlRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            TomlRejectionKind::Body(err) => write!(f, "Failed to read request body: {err}"),
            TomlRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize TOML: {err}")
            }
            TomlRejectionKind::InvalidUtf8 => write!(f, "Request body is not valid UTF-8"),
        }
    }
}

impl core::error::Error for TomlRejection {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match &self.kind {
            TomlRejectionKind::Body(err) => Some(err),
            TomlRejectionKind::Deserialize(_) => None,
            TomlRejectionKind::InvalidUtf8 => None,
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
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| TomlRejection {
                kind: TomlRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        let body_str = core::str::from_utf8(&bytes).map_err(|_| TomlRejection {
            kind: TomlRejectionKind::InvalidUtf8,
        })?;

        let value: T = crate::from_str(body_str).map_err(|e| TomlRejection {
            kind: TomlRejectionKind::Deserialize(e.to_string()),
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
            Ok(toml_string) => {
                let mut res = Response::new(Body::from(toml_string));
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
