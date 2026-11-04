//! Container, hotbar-selection, book/sign and cooldown packets.
use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_text::Component;

use crate::codec::{DecodeError, DecodeResult, read_count, read_utf, read_varint, write_varint};
use crate::packets::play::serverbound::SignTextSlot;
use crate::packets::play::types::InteractionHand;
use crate::{Decode, Encode, Packet};

/// `ClientboundContainerClosePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "container_close")]
pub struct ClientboundContainerClosePacket {
    #[mc(var)]
    pub container_id: i32,
}

/// `ClientboundContainerSetDataPacket` (id and value are shorts on the wire)
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "container_set_data")]
pub struct ClientboundContainerSetDataPacket {
    #[mc(var)]
    pub container_id: i32,
    pub id: i16,
    pub value: i16,
}

/// `ClientboundCooldownPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "cooldown")]
pub struct ClientboundCooldownPacket {
    pub cooldown_group: Identifier,
    #[mc(var)]
    pub duration: i32,
}

/// `ClientboundOpenBookPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "open_book")]
pub struct ClientboundOpenBookPacket {
    pub hand: InteractionHand,
}

/// `ClientboundOpenScreenPacket`; `menu_type` is vanilla's `type`.
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "open_screen")]
pub struct ClientboundOpenScreenPacket {
    #[mc(var)]
    pub container_id: i32,
    pub menu_type: mc_data::MenuType,
    pub title: Component,
}

/// `ClientboundOpenSignEditorPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "open_sign_editor")]
pub struct ClientboundOpenSignEditorPacket {
    pub pos: BlockPos,
    pub slot: SignTextSlot,
}

/// `ClientboundSetHeldSlotPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_held_slot")]
pub struct ClientboundSetHeldSlotPacket {
    #[mc(var)]
    pub slot: i32,
}

/// `ServerboundSelectBundleItemPacket` (`bundle_item_selected`)
#[derive(Debug, Clone, PartialEq, Eq, Packet)]
#[packet(state = play, flow = serverbound, name = "bundle_item_selected")]
pub struct ServerboundSelectBundleItemPacket {
    pub slot_id: i32,
    /// `-1` or non-negative (anything else is rejected on decode).
    pub selected_item_index: i32,
}

impl Encode for ServerboundSelectBundleItemPacket {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.slot_id);
        write_varint(out, self.selected_item_index);
    }
}
impl Decode for ServerboundSelectBundleItemPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let slot_id = read_varint(input)?;
        let selected_item_index = read_varint(input)?;
        if selected_item_index < 0 && selected_item_index != -1 {
            return Err(DecodeError::Message(format!(
                "Invalid selectedItemIndex: {selected_item_index}"
            )));
        }
        Ok(Self {
            slot_id,
            selected_item_index,
        })
    }
}

/// `ServerboundContainerButtonClickPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "container_button_click")]
pub struct ServerboundContainerButtonClickPacket {
    #[mc(var)]
    pub container_id: i32,
    #[mc(var)]
    pub button_id: i32,
}

/// `ServerboundContainerClosePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "container_close")]
pub struct ServerboundContainerClosePacket {
    #[mc(var)]
    pub container_id: i32,
}

/// `ServerboundContainerSlotStateChangedPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "container_slot_state_changed")]
pub struct ServerboundContainerSlotStateChangedPacket {
    #[mc(var)]
    pub slot_id: i32,
    #[mc(var)]
    pub container_id: i32,
    pub new_state: bool,
}

/// `ServerboundEditBookPacket`: at most 100 pages of at most 1024 chars, optional title of at most 32 chars.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "edit_book")]
pub struct ServerboundEditBookPacket {
    #[mc(var)]
    pub slot: i32,
    #[mc(with = edit_book_pages)]
    pub pages: Vec<String>,
    #[mc(max = 32)]
    pub title: Option<String>,
}

mod edit_book_pages {
    use super::*;

    pub fn encode(pages: &[String], out: &mut Vec<u8>) {
        pages.encode(out)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<String>> {
        let count = read_count(input, 100)?;
        (0..count).map(|_| read_utf(input, 1024)).collect()
    }
}

/// `ServerboundRenameItemPacket` (`readUtf()`: default 32767 chars)
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "rename_item")]
pub struct ServerboundRenameItemPacket {
    pub name: String,
}

/// `ServerboundSelectTradePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "select_trade")]
pub struct ServerboundSelectTradePacket {
    #[mc(var)]
    pub item: i32,
}

/// `ServerboundSetCarriedItemPacket` (a short)
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "set_carried_item")]
pub struct ServerboundSetCarriedItemPacket {
    pub slot: i16,
}
