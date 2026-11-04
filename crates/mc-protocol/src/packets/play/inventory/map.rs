//! `ClientboundMapItemDataPacket` and its nested types.
use mc_text::Component;

use super::untrusted_text;
use crate::codec::DecodeResult;
use crate::{Decode, Encode, Packet};

/// `MapId`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub struct MapId {
    #[mc(var)]
    pub id: i32,
}

/// `MapDecoration`; `rot` is masked to 4 bits on decode like vanilla's constructor.
#[derive(Debug, Clone, PartialEq, Encode)]
pub struct MapDecoration {
    pub decoration_type: mc_data::MapDecorationType,
    pub x: i8,
    pub y: i8,
    pub rot: i8,
    #[mc(with = untrusted_text::optional_component)]
    pub name: Option<Component>,
}

impl Decode for MapDecoration {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self {
            decoration_type: Decode::decode(input)?,
            x: i8::decode(input)?,
            y: i8::decode(input)?,
            rot: i8::decode(input)? & 15,
            name: untrusted_text::optional_component::decode(input)?,
        })
    }
}

/// `MapItemSavedData.MapPatch`: a `width` x `height` rectangle of colors at (`start_x`, `start_y`). Vanilla writes
/// every field as one byte and a width of 0 means "no patch" on the wire, so `width` must be at least 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapItemSavedDataMapPatch {
    pub start_x: u8,
    pub start_y: u8,
    pub width: u8,
    pub height: u8,
    pub map_colors: Vec<u8>,
}

/// `MapPatch.STREAM_CODEC` (`Optional<MapPatch>`): a zero width byte for the empty case.
mod map_patch {
    use super::*;

    pub fn encode(patch: &Option<MapItemSavedDataMapPatch>, out: &mut Vec<u8>) {
        match patch {
            Some(p) => {
                out.extend_from_slice(&[p.width, p.height, p.start_x, p.start_y]);
                p.map_colors.encode(out);
            }
            None => out.push(0),
        }
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Option<MapItemSavedDataMapPatch>> {
        let width = u8::decode(input)?;
        if width == 0 {
            return Ok(None);
        }
        let [height, start_x, start_y] = crate::codec::take_array(input)?;
        Ok(Some(MapItemSavedDataMapPatch { start_x, start_y, width, height, map_colors: Vec::decode(input)? }))
    }
}

/// `ClientboundMapItemDataPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "map_item_data")]
pub struct ClientboundMapItemDataPacket {
    pub map_id: MapId,
    pub scale: i8,
    pub locked: bool,
    pub decorations: Option<Vec<MapDecoration>>,
    #[mc(with = map_patch)]
    pub color_patch: Option<MapItemSavedDataMapPatch>,
}
