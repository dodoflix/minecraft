//! `net.minecraft.server.network.FilteredText`.

use super::filter_mask::FilterMaskExt;
use mc_core::java_string::JavaString;
use mc_protocol::packets::play::FilterMask;

#[derive(Clone, Debug, PartialEq)]
pub struct FilteredText {
    pub raw: String,
    pub mask: FilterMask,
}

impl FilteredText {
    pub fn empty() -> Self {
        Self::pass_through("")
    }

    pub fn pass_through(message: impl Into<String>) -> Self {
        Self {
            raw: message.into(),
            mask: FilterMask::PassThrough,
        }
    }

    pub fn fully_filtered(message: impl Into<String>) -> Self {
        Self {
            raw: message.into(),
            mask: FilterMask::FullyFiltered,
        }
    }

    pub fn filtered(&self) -> Option<JavaString> {
        self.mask.apply(&self.raw)
    }

    pub fn filtered_or_empty(&self) -> JavaString {
        self.filtered().unwrap_or_default()
    }

    pub fn is_filtered(&self) -> bool {
        !self.mask.is_empty()
    }
}
