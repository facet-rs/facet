//! `LinkSource` and `Attachment`: how the session machinery is given the
//! transport link it should use.
//!
//! Originally these lived in `stable_conduit` because reconnect was the
//! only consumer. With reconnect removed, they're just "give me a link
//! once" — kept as a trait so the existing session API still composes,
//! and so callers that want to swap implementations (e.g. tests vs
//! production sockets) keep working without changes.

use std::future::Future;

use vox_types::{Link, MaybeSend};

/// One transport attachment consumed by [`LinkSource::next_link`].
pub struct Attachment<L> {
    link: L,
}

impl<L> Attachment<L> {
    /// Build an attachment around a single ready-to-use link.
    pub fn initiator(link: L) -> Self {
        Self { link }
    }

    pub fn into_link(self) -> L {
        self.link
    }
}

/// Source of transport links. With reconnect machinery removed there's
/// only ever one call to `next_link` per session, but the trait remains
/// so existing code paths don't have to special-case the single-link
/// case.
pub trait LinkSource: MaybeSend + 'static {
    type Link: Link + MaybeSend;

    fn next_link(
        &mut self,
    ) -> impl Future<Output = std::io::Result<Attachment<Self::Link>>> + MaybeSend + '_;
}

/// One-shot link source: hands out its single attachment, then errors on
/// every subsequent call.
pub struct SingleAttachmentSource<L> {
    attachment: Option<Attachment<L>>,
}

pub fn single_attachment_source<L: Link + MaybeSend + 'static>(
    attachment: Attachment<L>,
) -> SingleAttachmentSource<L> {
    SingleAttachmentSource {
        attachment: Some(attachment),
    }
}

pub fn single_link_source<L: Link + MaybeSend + 'static>(link: L) -> SingleAttachmentSource<L> {
    single_attachment_source(Attachment::initiator(link))
}

impl<L: Link + MaybeSend + 'static> LinkSource for SingleAttachmentSource<L> {
    type Link = L;

    async fn next_link(&mut self) -> std::io::Result<Attachment<Self::Link>> {
        self.attachment.take().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "single-use LinkSource exhausted",
            )
        })
    }
}
