//! `LinkSource` and `Attachment`: how the connection machinery is given the
//! transport link it should use.
//!
//! This is a small abstraction over "give me a link once", kept as a trait
//! so the existing connection API composes with tests and production sockets.

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

/// Source of transport links.
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
