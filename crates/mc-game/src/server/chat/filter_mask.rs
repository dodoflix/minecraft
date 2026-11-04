//! `net.minecraft.network.chat.FilterMask` behavior on top of the wire enum `mc_protocol::packets::play::FilterMask`.

use mc_protocol::codec::BitSet;
use mc_protocol::packets::play::FilterMask;
use mc_text::{ChatFormatting, Component, HoverEvent, Style};

pub trait FilterMaskExt {
    /// `new FilterMask(length)`: partially filtered, nothing masked yet.
    fn partial() -> FilterMask;
    fn set_filtered(&mut self, index: usize);
    /// `apply(text)`; `None` = fully filtered. Works on UTF-16 units like `String.toCharArray()`.
    fn apply(&self, text: &str) -> Option<String>;
    fn apply_with_formatting(&self, text: &str) -> Option<Component>;
    fn is_empty(&self) -> bool;
    fn is_fully_filtered(&self) -> bool;
}

/// `FilterMask.FILTERED_STYLE`.
pub fn filtered_style() -> Style {
    Style::EMPTY
        .with_color_formatting(Some(ChatFormatting::DarkGray))
        .with_hover_event(Some(HoverEvent::show_text(Component::translatable("chat.filtered", vec![]))))
}

impl FilterMaskExt for FilterMask {
    fn partial() -> FilterMask {
        FilterMask::PartiallyFiltered(BitSet::new())
    }

    fn set_filtered(&mut self, index: usize) {
        if let FilterMask::PartiallyFiltered(mask) = self {
            mask.set(index);
        }
    }

    fn apply(&self, text: &str) -> Option<String> {
        match self {
            FilterMask::PassThrough => Some(text.to_owned()),
            FilterMask::FullyFiltered => None,
            FilterMask::PartiallyFiltered(mask) => {
                let mut units: Vec<u16> = text.encode_utf16().collect();
                for (i, unit) in units.iter_mut().enumerate() {
                    if i < mask.length() && mask.get(i) {
                        *unit = u16::from(b'#');
                    }
                }
                Some(String::from_utf16_lossy(&units))
            }
        }
    }

    fn apply_with_formatting(&self, text: &str) -> Option<Component> {
        match self {
            FilterMask::PassThrough => Some(Component::literal(text)),
            FilterMask::FullyFiltered => None,
            FilterMask::PartiallyFiltered(mask) => {
                let units: Vec<u16> = text.encode_utf16().collect();
                let mut result = Component::empty();
                let mut previous = 0usize;
                let mut filtered = mask.get(0);
                loop {
                    // nextClearBit / nextSetBit (-1 when none: nextSetBit only)
                    let next = if filtered {
                        (previous..).find(|&i| !mask.get(i))
                    } else {
                        (previous..mask.length()).find(|&i| mask.get(i))
                    };
                    let next = next.unwrap_or(units.len());
                    if next == previous {
                        return Some(result);
                    }
                    if filtered {
                        result = result.append(Component::literal("#".repeat(next - previous)).with_style(&filtered_style()));
                    } else {
                        // Java `substring` would throw past the end; the mask never exceeds the text in practice
                        let end = next.min(units.len());
                        result = result.append_str(&String::from_utf16_lossy(&units[previous.min(end)..end]));
                    }
                    filtered = !filtered;
                    previous = next;
                }
            }
        }
    }

    fn is_empty(&self) -> bool {
        matches!(self, FilterMask::PassThrough)
    }

    fn is_fully_filtered(&self) -> bool {
        matches!(self, FilterMask::FullyFiltered)
    }
}
