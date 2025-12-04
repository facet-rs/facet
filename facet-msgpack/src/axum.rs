//! Axum integration for `MsgPack<T>`.

use crate::MsgPack;
use axum_core::{
    body::Body,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
};
use core::fmt;
use facet_core::Facet;
use http::{HeaderValue, StatusCode, header};
use http_body_util::BodyExt;

/// Rejection type for MessagePack extraction errors.
#[derive(Debug)]
pub struct MsgPackRejection {
    kind: MsgPackRejectionKind,
}

#[derive(Debug)]
enum MsgPackRejectionKind {
    Body(axum_core::Error),
    Deserialize(crate::DecodeError),
}

impl MsgPackRejection {
    /// Returns the status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            MsgPackRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            MsgPackRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }
}

impl fmt::Display for MsgPackRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            MsgPackRejectionKind::Body(err) => write!(f, "Failed to read request body: {err}"),
            MsgPackRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize MessagePack: {err}")
            }
        }
    }
}

impl core::error::Error for MsgPackRejection {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
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
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| MsgPackRejection {
                kind: MsgPackRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

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
        let bytes = crate::to_vec(&self.0);
        let mut res = Response::new(Body::from(bytes));
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/msgpack"),
        );
        res
    }
}
