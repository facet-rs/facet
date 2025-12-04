//! Axum integration for `Xml<T>`.

use crate::Xml;
use axum_core::{
    body::Body,
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
};
use core::fmt;
use facet_core::Facet;
use http::{HeaderValue, StatusCode, header};
use http_body_util::BodyExt;

/// Rejection type for XML extraction errors.
#[derive(Debug)]
pub struct XmlRejection {
    kind: XmlRejectionKind,
}

#[derive(Debug)]
enum XmlRejectionKind {
    Body(axum_core::Error),
    Deserialize(crate::XmlError),
}

impl XmlRejection {
    /// Returns the status code for this rejection.
    pub fn status(&self) -> StatusCode {
        match &self.kind {
            XmlRejectionKind::Body(_) => StatusCode::BAD_REQUEST,
            XmlRejectionKind::Deserialize(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }
}

impl fmt::Display for XmlRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            XmlRejectionKind::Body(err) => write!(f, "Failed to read request body: {err}"),
            XmlRejectionKind::Deserialize(err) => {
                write!(f, "Failed to deserialize XML: {err}")
            }
        }
    }
}

impl core::error::Error for XmlRejection {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
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
        let bytes = req
            .into_body()
            .collect()
            .await
            .map_err(|e| XmlRejection {
                kind: XmlRejectionKind::Body(axum_core::Error::new(e)),
            })?
            .to_bytes();

        let value: T = crate::from_slice_owned(&bytes).map_err(|e| XmlRejection {
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
        match crate::to_string(&self.0) {
            Ok(xml_string) => {
                let mut res = Response::new(Body::from(xml_string));
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
