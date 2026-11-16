//! `net.minecraft.server.ServerLinks`: the links a server shows in its client's pause menu, sent in the configuration
//! phase.

use mc_protocol::codec::Either;
use mc_protocol::packets::common::{KnownLinkType, UntrustedEntry};
use mc_text::Component;

/// `ServerLinks.Entry`: a known link type or a custom display name, and the link (`java.net.URI.toString()` is the text
/// the URI was parsed from).
#[derive(Debug, Clone, PartialEq)]
pub struct ServerLink {
    pub link_type: Either<KnownLinkType, Component>,
    pub link: String,
}

impl ServerLink {
    /// `ServerLinks.KnownLinkType.create(link)`.
    pub fn known(link_type: KnownLinkType, link: String) -> Self {
        Self {
            link_type: Either::Left(link_type),
            link,
        }
    }
}

/// `ServerLinks`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServerLinks {
    pub entries: Vec<ServerLink>,
}

impl ServerLinks {
    /// `ServerLinks.EMPTY`.
    pub const EMPTY: ServerLinks = ServerLinks {
        entries: Vec::new(),
    };

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `untrust()`: what the packet carries.
    pub fn untrust(&self) -> Vec<UntrustedEntry> {
        self.entries
            .iter()
            .map(|e| UntrustedEntry {
                link_type: e.link_type.clone(),
                link: e.link.clone(),
            })
            .collect()
    }
}
