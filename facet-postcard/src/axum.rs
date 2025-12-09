//! Axum integration for `Postcard<T>`.

use crate::Postcard;
use axum_core::{
    body::Body,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
};
use core::fmt;
use facet_core::Facet;
use http::{HeaderValue, StatusCode, header};
use http_body_util::BodyExt;

/// Rejection type for Postcard extraction errors.
#[derive(Debug)]
pub struct PostcardRejection {
    kind: PostcardRejectionKind,
}

#[derive(Debug)]
enum PostcardRejectionKind {
    Body(axum_core::Error),
    Deserialize(crate::DeserializeError),
}

impl PostcardRejection {
    /// Returns the status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            PostcardRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            PostcardRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
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

impl core::error::Error for PostcardRejection {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
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
    T: Facet<'static>,
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
    T: Facet<'static>,
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
