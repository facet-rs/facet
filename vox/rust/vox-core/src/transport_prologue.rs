use vox_types::{Link, LinkRx, LinkTx, LinkTxPermit, SplitLink, TransportMode, WriteSlot};
use zerocopy::FromBytes;
use zerocopy::little_endian::U32 as LeU32;

const TRANSPORT_HELLO_MAGIC: u32 = u32::from_le_bytes(*b"VOTH");
const TRANSPORT_ACCEPT_MAGIC: u32 = u32::from_le_bytes(*b"VOTA");
const TRANSPORT_REJECT_MAGIC: u32 = u32::from_le_bytes(*b"VOTR");
const TRANSPORT_VERSION: u8 = 9;
const REJECT_UNSUPPORTED_MODE: u8 = 1;

fn transport_mode_as_u8(mode: TransportMode) -> u8 {
    match mode {
        TransportMode::Bare => 0,
        TransportMode::Stable => 1,
    }
}

fn transport_mode_from_u8(value: u8) -> Result<TransportMode, TransportPrologueError> {
    match value {
        0 => Ok(TransportMode::Bare),
        1 => Ok(TransportMode::Stable),
        _ => Err(TransportPrologueError::Protocol(format!(
            "unknown conduit mode {value}"
        ))),
    }
}

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
    UnsupportedMode,
}

impl std::fmt::Display for TransportRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedMode => write!(f, "unsupported conduit mode"),
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
    requested_mode: u8,
    reserved: [u8; 2],
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
    selected_mode: u8,
    reserved: [u8; 2],
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

pub async fn initiate_transport<L: Link>(
    link: L,
    requested_mode: TransportMode,
) -> Result<SplitLink<L::Tx, L::Rx>, TransportPrologueError> {
    if !L::supports_transport_mode(requested_mode) {
        return Err(TransportPrologueError::Rejected(
            TransportRejectReason::UnsupportedMode,
        ));
    }

    let (tx, mut rx) = link.split();
    let hello = TransportHello {
        magic: LeU32::new(TRANSPORT_HELLO_MAGIC),
        version: TRANSPORT_VERSION,
        requested_mode: transport_mode_as_u8(requested_mode),
        reserved: [0; 2],
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
        let selected_mode = transport_mode_from_u8(accept.selected_mode)?;
        if selected_mode != requested_mode {
            return Err(TransportPrologueError::Protocol(format!(
                "transport selected {selected_mode:?}, requested {requested_mode:?}"
            )));
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
            REJECT_UNSUPPORTED_MODE => TransportRejectReason::UnsupportedMode,
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

pub async fn accept_transport<L: Link>(
    link: L,
) -> Result<(TransportMode, SplitLink<L::Tx, L::Rx>), TransportPrologueError> {
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
    let requested_mode = transport_mode_from_u8(hello.requested_mode)?;
    if !L::supports_transport_mode(requested_mode) {
        reject_transport(&tx, TransportRejectReason::UnsupportedMode).await?;
        return Err(TransportPrologueError::Rejected(
            TransportRejectReason::UnsupportedMode,
        ));
    }

    let accept = TransportAccept {
        magic: LeU32::new(TRANSPORT_ACCEPT_MAGIC),
        version: TRANSPORT_VERSION,
        selected_mode: transport_mode_as_u8(requested_mode),
        reserved: [0; 2],
    };
    send_message(&tx, &accept).await?;
    Ok((requested_mode, SplitLink { tx, rx }))
}

pub async fn reject_transport<L: LinkTx>(
    tx: &L,
    reason: TransportRejectReason,
) -> Result<(), TransportPrologueError> {
    let code = match reason {
        TransportRejectReason::UnsupportedMode => REJECT_UNSUPPORTED_MODE,
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
    let bytes = message.as_bytes();
    let permit = tx.reserve().await.map_err(TransportPrologueError::Io)?;
    let mut slot = permit
        .alloc(bytes.len())
        .map_err(TransportPrologueError::Io)?;
    slot.as_mut_slice().copy_from_slice(bytes);
    slot.commit();
    Ok(())
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
