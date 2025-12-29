//! Axum integration for `Yaml<T>`.

use crate::Yaml;
use axum_core::{
    body::Body,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
};
use core::fmt;
use facet_core::Facet;
use http::{HeaderValue, StatusCode, header};
use http_body_util::BodyExt;

/// Rejection type for YAML extraction errors.
#[derive(Debug)]
pub struct YamlRejection {
    kind: YamlRejectionKind,
}

#[derive(Debug)]
enum YamlRejectionKind {
    Body(axum_core::Error),
    Deserialize(crate::YamlError),
    InvalidUtf8,
}

impl YamlRejection {
    /// Returns the status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            YamlRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            YamlRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
            YamlRejectionKind::InvalidUtf8 => StatusCode::BAD_REQUEST,
        }
    }
}

impl fmt::Display for YamlRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            YamlRejectionKind::Body(err) => write!(f, "Failed to read request body: {err}"),
            YamlRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize YAML: {err}")
            }
            YamlRejectionKind::InvalidUtf8 => write!(f, "Request body is not valid UTF-8"),
        }
    }
}

impl core::error::Error for YamlRejection {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match &self.kind {
            YamlRejectionKind::Body(err) => Some(err),
            YamlRejectionKind::Deserialize(err) => Some(err),
            YamlRejectionKind::InvalidUtf8 => None,
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
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| YamlRejection {
                kind: YamlRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        let body_str = core::str::from_utf8(&bytes).map_err(|_| YamlRejection {
            kind: YamlRejectionKind::InvalidUtf8,
        })?;

        let value: T = crate::from_str(body_str).map_err(|e| YamlRejection {
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
            Ok(yaml_string) => {
                let mut res = Response::new(Body::from(yaml_string));
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
