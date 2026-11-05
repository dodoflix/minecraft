//! Packets that carry item stacks: container contents, cursor/inventory slots, clicks, creative slot.
use crate::codec::EncodeResult;
use crate::codec::{DecodeError, DecodeResult, read_varint};
use crate::components::{HashedStack, ItemStack};
use crate::packets::common::id_enum;
use crate::{Decode, Encode, Packet};

id_enum!(
    /// `ContainerInput` (formerly `ClickType`): VarInt id, unknown = pickup.
    ContainerInput { Pickup = 0, QuickMove = 1, Swap = 2, Clone = 3, Throw = 4, QuickCraft = 5, PickupAll = 6 } out_of_bounds = ZERO
);

/// `ClientboundContainerSetContentPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "container_set_content")]
pub struct ClientboundContainerSetContentPacket {
    #[mc(var)]
    pub container_id: i32,
    #[mc(var)]
    pub state_id: i32,
    pub items: Vec<ItemStack>,
    pub carried_item: ItemStack,
}

/// `ClientboundContainerSetSlotPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "container_set_slot")]
pub struct ClientboundContainerSetSlotPacket {
    #[mc(var)]
    pub container_id: i32,
    #[mc(var)]
    pub state_id: i32,
    pub slot: i16,
    pub item_stack: ItemStack,
}

/// `ClientboundSetCursorItemPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_cursor_item")]
pub struct ClientboundSetCursorItemPacket {
    pub contents: ItemStack,
}

/// `ClientboundSetPlayerInventoryPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_player_inventory")]
pub struct ClientboundSetPlayerInventoryPacket {
    #[mc(var)]
    pub slot: i32,
    pub contents: ItemStack,
}

/// `ServerboundContainerClickPacket`; `changed_slots` is vanilla's `Int2ObjectMap` (at most 128 entries) in wire order.
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "container_click")]
pub struct ServerboundContainerClickPacket {
    #[mc(var)]
    pub container_id: i32,
    #[mc(var)]
    pub state_id: i32,
    pub slot_num: i16,
    pub button_num: i8,
    pub container_input: ContainerInput,
    #[mc(with = changed_slots)]
    pub changed_slots: Vec<(i16, HashedStack)>,
    pub carried_item: HashedStack,
}

mod changed_slots {
    use super::*;

    pub fn encode(slots: &[(i16, HashedStack)], out: &mut Vec<u8>) -> EncodeResult<()> {
        crate::codec::write_count(out, slots.len(), 128)?;
        <(i16, HashedStack)>::encode_slice(slots, out)
    }
    /// `ByteBufCodecs.map(Int2ObjectOpenHashMap::new, .., 128)`: a repeated key replaces the earlier value.
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<(i16, HashedStack)>> {
        let count = read_varint(input)?;
        if count > 128 {
            return Err(DecodeError::Message(format!(
                "{count} elements exceeded max size of: 128"
            )));
        }
        if count < 0 {
            return Err(DecodeError::msg(
                "The expected number of elements must be nonnegative",
            ));
        }
        let mut slots: Vec<(i16, HashedStack)> = Vec::with_capacity(count as usize);
        for _ in 0..count {
            slots.push((i16::decode(input)?, HashedStack::decode(input)?));
        }
        Ok(crate::codec::collapse_map(slots))
    }
}

/// `ServerboundSetCreativeModeSlotPacket`: the stack uses the untrusted codec and is validated on decode
/// (`ItemStack.validatedStreamCodec`). Vanilla only handles it when the player has infinite materials.
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "set_creative_mode_slot")]
pub struct ServerboundSetCreativeModeSlotPacket {
    pub slot_num: i16,
    #[mc(with = creative_stack)]
    pub item_stack: ItemStack,
}

mod creative_stack {
    use super::*;

    pub fn encode(stack: &ItemStack, out: &mut Vec<u8>) -> EncodeResult<()> {
        stack.encode_optional_untrusted(out)?;
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<ItemStack> {
        ItemStack::decode_validated_untrusted(input)
    }
}
