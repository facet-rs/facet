//! Axum integration for `Kdl<T>`.

use crate::Kdl;
use axum_core::{
    body::Body,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
};
use core::fmt;
use facet_core::Facet;
use http::{HeaderValue, StatusCode, header};
use http_body_util::BodyExt;

/// Rejection type for KDL extraction errors.
#[derive(Debug)]
pub struct KdlRejection {
    kind: KdlRejectionKind,
}

#[derive(Debug)]
enum KdlRejectionKind {
    Body(axum_core::Error),
    Deserialize(crate::KdlError),
    InvalidUtf8,
}

impl KdlRejection {
    /// Returns the status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            KdlRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            KdlRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
            KdlRejectionKind::InvalidUtf8 => StatusCode::BAD_REQUEST,
        }
    }
}

impl fmt::Display for KdlRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            KdlRejectionKind::Body(err) => write!(f, "Failed to read request body: {err}"),
            KdlRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize KDL: {err}")
            }
            KdlRejectionKind::InvalidUtf8 => write!(f, "Request body is not valid UTF-8"),
        }
    }
}

impl core::error::Error for KdlRejection {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match &self.kind {
            KdlRejectionKind::Body(err) => Some(err),
            KdlRejectionKind::Deserialize(err) => Some(err),
            KdlRejectionKind::InvalidUtf8 => None,
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
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| KdlRejection {
                kind: KdlRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        let body_str = core::str::from_utf8(&bytes).map_err(|_| KdlRejection {
            kind: KdlRejectionKind::InvalidUtf8,
        })?;

        let value: T = crate::from_str_owned(body_str).map_err(|e| KdlRejection {
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
            Ok(kdl_string) => {
                let mut res = Response::new(Body::from(kdl_string));
                // KDL doesn't have an official MIME type yet
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
