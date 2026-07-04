//! JSON-RPC 2.0 messages and LSP `Content-Length` framing.

use std::fmt;
use std::io::{BufRead, Write};

use facet::Facet;
use facet_json::RawJson;

/// JSON-RPC request id.
#[derive(Clone, Debug, Facet, PartialEq, Eq, PartialOrd, Ord)]
#[facet(untagged)]
#[repr(u8)]
pub enum RpcId {
    /// Numeric JSON-RPC id.
    Number(i64),
    /// String JSON-RPC id.
    String(String),
}

impl From<i64> for RpcId {
    fn from(value: i64) -> Self {
        Self::Number(value)
    }
}

impl From<&str> for RpcId {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

/// Incoming JSON-RPC message. `params` remains raw until the method is known.
#[derive(Debug, Facet, PartialEq)]
pub struct IncomingMessage {
    /// Protocol version.
    pub jsonrpc: String,
    /// Request id when this is a request.
    pub id: Option<RpcId>,
    /// Method name.
    pub method: String,
    /// Raw params decoded by the method dispatcher.
    pub params: Option<RawJson<'static>>,
}

/// Typed JSON-RPC request, useful for tests and client fixtures.
#[derive(Debug, Facet)]
pub struct RequestMessage<T> {
    /// Protocol version.
    pub jsonrpc: String,
    /// Request id.
    pub id: RpcId,
    /// Method name.
    pub method: String,
    /// Typed params.
    pub params: T,
}

impl<T> RequestMessage<T> {
    /// Construct a request message.
    pub fn new(id: impl Into<RpcId>, method: impl Into<String>, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            id: id.into(),
            method: method.into(),
            params,
        }
    }
}

/// Typed JSON-RPC notification, useful for tests and client fixtures.
#[derive(Debug, Facet)]
pub struct NotificationMessage<T> {
    /// Protocol version.
    pub jsonrpc: String,
    /// Method name.
    pub method: String,
    /// Typed params.
    pub params: T,
}

impl<T> NotificationMessage<T> {
    /// Construct a notification message.
    pub fn new(method: impl Into<String>, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC response.
#[derive(Debug, Facet)]
pub struct ResponseMessage {
    /// Protocol version.
    pub jsonrpc: String,
    /// Correlated request id.
    pub id: RpcId,
    /// Result payload.
    pub result: Option<RawJson<'static>>,
    /// Error payload.
    pub error: Option<ResponseError>,
}

impl ResponseMessage {
    /// Successful response from a typed result.
    pub fn result<T: Facet<'static>>(id: RpcId, result: &T) -> Result<Self, FrameError> {
        Ok(Self {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(RawJson::from_owned(serialize_json(result)?)),
            error: None,
        })
    }

    /// Error response.
    pub fn error(id: RpcId, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            id,
            result: None,
            error: Some(ResponseError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// JSON-RPC error object.
#[derive(Debug, Facet)]
pub struct ResponseError {
    /// JSON-RPC error code.
    pub code: i32,
    /// Error message.
    pub message: String,
    /// Optional raw error data.
    pub data: Option<RawJson<'static>>,
}

/// Outgoing server notification with raw params.
#[derive(Debug, Facet)]
pub struct RawNotificationMessage {
    /// Protocol version.
    pub jsonrpc: String,
    /// Method name.
    pub method: String,
    /// Raw params.
    pub params: RawJson<'static>,
}

impl RawNotificationMessage {
    /// Construct a notification from typed params.
    pub fn typed<T: Facet<'static>>(
        method: impl Into<String>,
        params: &T,
    ) -> Result<Self, FrameError> {
        Ok(Self {
            jsonrpc: "2.0".to_owned(),
            method: method.into(),
            params: RawJson::from_owned(serialize_json(params)?),
        })
    }
}

/// Errors from framing and message serialization.
#[derive(Debug)]
pub enum FrameError {
    /// I/O failed.
    Io(std::io::Error),
    /// Missing `Content-Length`.
    MissingContentLength,
    /// Malformed header.
    BadHeader(String),
    /// Malformed JSON.
    Json(facet_json::DeserializeError),
    /// Serialization failed.
    Serialize(String),
    /// Body was not UTF-8.
    Utf8(std::string::FromUtf8Error),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::MissingContentLength => f.write_str("missing Content-Length header"),
            Self::BadHeader(header) => write!(f, "bad LSP header: {header:?}"),
            Self::Json(err) => write!(f, "JSON decode error: {err}"),
            Self::Serialize(err) => write!(f, "JSON encode error: {err}"),
            Self::Utf8(err) => write!(f, "non-UTF-8 message body: {err}"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<std::io::Error> for FrameError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<facet_json::DeserializeError> for FrameError {
    fn from(value: facet_json::DeserializeError) -> Self {
        Self::Json(value)
    }
}

impl From<std::string::FromUtf8Error> for FrameError {
    fn from(value: std::string::FromUtf8Error) -> Self {
        Self::Utf8(value)
    }
}

/// Serialize a typed JSON value as an LSP frame.
pub fn frame<T: Facet<'static>>(message: &T) -> Result<Vec<u8>, FrameError> {
    let json = serialize_json(message)?;
    Ok(frame_json(&json))
}

fn serialize_json<T: Facet<'static>>(message: &T) -> Result<String, FrameError> {
    facet_json::to_string(message).map_err(|err| FrameError::Serialize(err.to_string()))
}

/// Build an LSP frame from an already-serialized JSON value.
pub fn frame_json(json: &str) -> Vec<u8> {
    let mut out = format!("Content-Length: {}\r\n\r\n", json.len()).into_bytes();
    out.extend_from_slice(json.as_bytes());
    out
}

/// Write one typed JSON message as an LSP frame.
pub fn write_frame<T: Facet<'static>>(
    writer: &mut impl Write,
    message: &T,
) -> Result<(), FrameError> {
    writer.write_all(&frame(message)?)?;
    writer.flush()?;
    Ok(())
}

/// Read one raw LSP frame body. Returns `Ok(None)` at clean EOF before headers.
pub fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, FrameError> {
    let mut content_length = None;
    let mut saw_header = false;

    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return if saw_header {
                Err(FrameError::MissingContentLength)
            } else {
                Ok(None)
            };
        }
        saw_header = true;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            return Err(FrameError::BadHeader(trimmed.to_owned()));
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            let len = value
                .trim()
                .parse::<usize>()
                .map_err(|_| FrameError::BadHeader(trimmed.to_owned()))?;
            content_length = Some(len);
        }
    }

    let len = content_length.ok_or(FrameError::MissingContentLength)?;
    let mut body = vec![0; len];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

/// Read and decode one incoming JSON-RPC message.
pub fn read_message(reader: &mut impl BufRead) -> Result<Option<IncomingMessage>, FrameError> {
    let Some(body) = read_frame(reader)? else {
        return Ok(None);
    };
    let json = String::from_utf8(body)?;
    Ok(Some(facet_json::from_str(&json)?))
}

/// Decode one incoming JSON-RPC message body.
pub fn decode_message(body: &[u8]) -> Result<IncomingMessage, FrameError> {
    let json = std::str::from_utf8(body)
        .map_err(|err| FrameError::BadHeader(format!("message body is not UTF-8: {err}")))?;
    Ok(facet_json::from_str(json)?)
}
