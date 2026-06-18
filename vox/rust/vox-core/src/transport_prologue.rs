use vox_types::{Link, LinkRx, LinkTx, SplitLink};
use zerocopy::FromBytes;
use zerocopy::little_endian::U32 as LeU32;

const TRANSPORT_HELLO_MAGIC: u32 = u32::from_le_bytes(*b"VOTH");
const TRANSPORT_ACCEPT_MAGIC: u32 = u32::from_le_bytes(*b"VOTA");
const TRANSPORT_REJECT_MAGIC: u32 = u32::from_le_bytes(*b"VOTR");
const TRANSPORT_VERSION: u8 = 9;
const REJECT_UNSUPPORTED_PROLOGUE: u8 = 1;
const RESERVED_ZERO: [u8; 3] = [0; 3];

#[derive(Debug)]
pub enum TransportPrologueError {
    Io(std::io::Error),
    LinkDead,
    Protocol(String),
    Rejected(TransportRejectReason),
}

impl std::fmt::Display for TransportPrologueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "io error: {error}"),
            Self::LinkDead => write!(f, "link closed during transport prologue"),
            Self::Protocol(message) => write!(f, "protocol error: {message}"),
            Self::Rejected(reason) => write!(f, "transport rejected: {reason}"),
        }
    }
}

impl std::error::Error for TransportPrologueError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportRejectReason {
    UnsupportedPrologue,
}

impl std::fmt::Display for TransportRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPrologue => write!(f, "unsupported transport prologue"),
        }
    }
}

#[derive(
    Clone,
    Copy,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
#[repr(C)]
struct TransportHello {
    magic: LeU32,
    version: u8,
    reserved: [u8; 3],
}

#[derive(
    Clone,
    Copy,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
#[repr(C)]
struct TransportAccept {
    magic: LeU32,
    version: u8,
    reserved: [u8; 3],
}

#[derive(
    Clone,
    Copy,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
#[repr(C)]
struct TransportReject {
    magic: LeU32,
    version: u8,
    reason: u8,
    reserved: [u8; 2],
}

// r[impl transport.prologue]
// r[impl transport.prologue.request]
// r[impl transport.prologue.accept]
// r[impl transport.prologue.reject-close]
pub async fn initiate_transport<L: Link>(
    link: L,
) -> Result<SplitLink<L::Tx, L::Rx>, TransportPrologueError> {
    let (tx, mut rx) = link.split();
    let hello = TransportHello {
        magic: LeU32::new(TRANSPORT_HELLO_MAGIC),
        version: TRANSPORT_VERSION,
        reserved: RESERVED_ZERO,
    };
    send_message(&tx, &hello).await?;

    let raw = recv_bytes(&mut rx).await?;
    let bytes = raw.as_bytes();
    let magic = bytes
        .get(..4)
        .and_then(|prefix| prefix.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| {
            TransportPrologueError::Protocol("transport prologue message size mismatch".into())
        })?;

    if magic == TRANSPORT_ACCEPT_MAGIC {
        let accept = TransportAccept::read_from_bytes(bytes).map_err(|_| {
            TransportPrologueError::Protocol("transport prologue message size mismatch".into())
        })?;
        if accept.version != TRANSPORT_VERSION {
            return Err(TransportPrologueError::Protocol(format!(
                "unsupported transport version {}",
                accept.version
            )));
        }
        if accept.reserved != RESERVED_ZERO {
            return Err(TransportPrologueError::Protocol(
                "transport accept reserved bytes must be zero".into(),
            ));
        }
        return Ok(SplitLink { tx, rx });
    }

    if magic == TRANSPORT_REJECT_MAGIC {
        let reject = TransportReject::read_from_bytes(bytes).map_err(|_| {
            TransportPrologueError::Protocol("transport prologue message size mismatch".into())
        })?;
        if reject.version != TRANSPORT_VERSION {
            return Err(TransportPrologueError::Protocol(format!(
                "unsupported transport version {}",
                reject.version
            )));
        }
        let reason = match reject.reason {
            REJECT_UNSUPPORTED_PROLOGUE => TransportRejectReason::UnsupportedPrologue,
            other => {
                return Err(TransportPrologueError::Protocol(format!(
                    "unknown transport reject reason {other}"
                )));
            }
        };
        return Err(TransportPrologueError::Rejected(reason));
    }

    Err(TransportPrologueError::Protocol(
        "expected TransportAccept or TransportReject".into(),
    ))
}

// r[impl transport.prologue]
// r[impl transport.prologue.first-payload]
// r[impl transport.prologue.request]
// r[impl transport.prologue.accept]
// r[impl transport.prologue.reject-close]
pub async fn accept_transport<L: Link>(
    link: L,
) -> Result<SplitLink<L::Tx, L::Rx>, TransportPrologueError> {
    let (tx, mut rx) = link.split();
    let hello = recv_message::<_, TransportHello>(&mut rx).await?;
    if hello.magic.get() != TRANSPORT_HELLO_MAGIC {
        return Err(TransportPrologueError::Protocol(
            "transport hello magic mismatch".into(),
        ));
    }
    if hello.version != TRANSPORT_VERSION {
        return Err(TransportPrologueError::Protocol(format!(
            "unsupported transport version {}",
            hello.version
        )));
    }
    if hello.reserved != RESERVED_ZERO {
        reject_transport(&tx, TransportRejectReason::UnsupportedPrologue).await?;
        return Err(TransportPrologueError::Protocol(
            "transport hello reserved bytes must be zero".into(),
        ));
    }

    let accept = TransportAccept {
        magic: LeU32::new(TRANSPORT_ACCEPT_MAGIC),
        version: TRANSPORT_VERSION,
        reserved: RESERVED_ZERO,
    };
    send_message(&tx, &accept).await?;
    Ok(SplitLink { tx, rx })
}

// r[impl transport.prologue.accept]
// r[impl transport.prologue.reject-close]
pub async fn reject_transport<L: LinkTx>(
    tx: &L,
    reason: TransportRejectReason,
) -> Result<(), TransportPrologueError> {
    let code = match reason {
        TransportRejectReason::UnsupportedPrologue => REJECT_UNSUPPORTED_PROLOGUE,
    };
    let reject = TransportReject {
        magic: LeU32::new(TRANSPORT_REJECT_MAGIC),
        version: TRANSPORT_VERSION,
        reason: code,
        reserved: [0; 2],
    };
    send_message(tx, &reject).await
}

async fn send_message<LTx: LinkTx, M: zerocopy::IntoBytes + zerocopy::Immutable>(
    tx: &LTx,
    message: &M,
) -> Result<(), TransportPrologueError> {
    tx.send(message.as_bytes().to_vec())
        .await
        .map_err(TransportPrologueError::Io)
}

async fn recv_message<
    LRx: LinkRx,
    M: zerocopy::FromBytes + zerocopy::KnownLayout + zerocopy::Immutable,
>(
    rx: &mut LRx,
) -> Result<M, TransportPrologueError> {
    let raw = recv_bytes(rx).await?;
    M::read_from_bytes(raw.as_bytes()).map_err(|_| {
        TransportPrologueError::Protocol("transport prologue message size mismatch".into())
    })
}

async fn recv_bytes<LRx: LinkRx>(
    rx: &mut LRx,
) -> Result<vox_types::Backing, TransportPrologueError> {
    rx.recv()
        .await
        .map_err(|error| {
            TransportPrologueError::Protocol(format!("transport recv failed: {error}"))
        })?
        .ok_or(TransportPrologueError::LinkDead)
}
