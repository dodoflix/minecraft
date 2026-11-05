//! `MapBanner`, `MapFrame` and `MapIndex` (the `maps/last_id` counter).

use super::map_decoration::{MapDecorationType, banner_decoration};
use crate::util::codec::{self, DataResult, DecodeResult};
use crate::world::block_entity::BlockEntity;
use crate::world::level::BlockGetter;
use crate::world::level::saveddata::{SavedData, SavedDataType};
use crate::impl_saved_data;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::DyeColor;
use mc_text::Component;

/// `BlockPos.CODEC` (an int array of three) as a tag.
pub fn block_pos_to_tag(pos: BlockPos) -> Tag {
    Tag::IntArray(vec![pos.x(), pos.y(), pos.z()])
}

/// `BlockPos.CODEC`.
pub fn block_pos_from_tag(tag: &Tag) -> DataResult<BlockPos> {
    let ints = codec::list(tag)?.iter().map(codec::int).collect::<DataResult<Vec<_>>>()?;
    match ints[..] {
        [x, y, z] => Ok(BlockPos::new(x, y, z)),
        _ => Err(format!("Input is not a list of 3 ints: {} elements", ints.len())),
    }
}

/// `MapBanner`.
#[derive(Clone, Debug, PartialEq)]
pub struct MapBanner {
    pub pos: BlockPos,
    pub color: DyeColor,
    pub name: Option<Component>,
}

impl MapBanner {
    /// `MapBanner.fromWorld(level, pos)`: the banner block entity at `pos` (`getBaseColor()` is the banner block's color,
    /// `getCustomName()` its saved `CustomName`).
    pub fn from_world(level: &dyn BlockGetter, pos: BlockPos) -> Option<MapBanner> {
        let be = level.get_block_entity(pos)?;
        if be.get_type() != BlockEntityType::BANNER {
            return None;
        }
        let block = be.get_block_state().block();
        let path = block.name().trim_start_matches("minecraft:");
        let color = path.strip_suffix("_wall_banner").or_else(|| path.strip_suffix("_banner")).and_then(DyeColor::from_name)?;
        Some(MapBanner { pos, color, name: custom_name(be) })
    }

    pub fn get_decoration(&self) -> MapDecorationType {
        banner_decoration(self.color)
    }

    /// `getId()`.
    pub fn get_id(&self) -> String {
        format!("banner-{},{},{}", self.pos.x(), self.pos.y(), self.pos.z())
    }

    pub fn decode(tag: &Tag) -> DataResult<MapBanner> {
        let m = codec::compound(tag)?;
        let pos = block_pos_from_tag(codec::field(m, "pos")?)?;
        // `lenientOptionalFieldOf`: an unreadable value counts as absent.
        let color = m.get("color").and_then(|t| t.as_string()).and_then(DyeColor::from_name).unwrap_or(DyeColor::White);
        let name = m.get("name").and_then(|t| Component::from_nbt(t).ok());
        Ok(MapBanner { pos, color, name })
    }

    pub fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        m.put("pos", block_pos_to_tag(self.pos));
        if self.color != DyeColor::White {
            m.put_string("color", self.color.name());
        }
        if let Some(name) = &self.name {
            m.put("name", name.to_nbt().map_err(|e| format!("{e:?}"))?);
        }
        Ok(Tag::Compound(m))
    }
}

/// `BannerBlockEntity.getCustomName()`: the `CustomName` of its saved data.
fn custom_name(be: &dyn BlockEntity) -> Option<Component> {
    let mut tag = CompoundTag::new();
    be.save_additional(&mut tag);
    tag.get("CustomName").and_then(|t| Component::from_nbt(t).ok())
}

/// `MapFrame`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapFrame {
    pub pos: BlockPos,
    pub rotation: i32,
    pub entity_id: i32,
}

impl MapFrame {
    pub fn get_id(&self) -> String {
        Self::frame_id(self.pos)
    }

    /// `MapFrame.frameId(pos)`.
    pub fn frame_id(pos: BlockPos) -> String {
        format!("frame-{},{},{}", pos.x(), pos.y(), pos.z())
    }

    pub fn decode(tag: &Tag) -> DataResult<MapFrame> {
        let m = codec::compound(tag)?;
        Ok(MapFrame {
            pos: block_pos_from_tag(codec::field(m, "pos")?)?,
            rotation: codec::int(codec::field(m, "rotation")?)?,
            entity_id: codec::int(codec::field(m, "entity_id")?)?,
        })
    }

    pub fn encode(&self) -> Tag {
        let mut m = CompoundTag::new();
        m.put("pos", block_pos_to_tag(self.pos));
        m.put_int("rotation", self.rotation);
        m.put_int("entity_id", self.entity_id);
        Tag::Compound(m)
    }
}

/// `MapIndex` (`data/minecraft/maps/last_id.dat`): the last allocated map id.
#[derive(Debug, Clone)]
pub struct MapIndex {
    dirty: bool,
    last_map_id: i32,
}

impl_saved_data!(MapIndex);

impl Default for MapIndex {
    fn default() -> Self {
        MapIndex { dirty: false, last_map_id: -1 }
    }
}

impl MapIndex {
    /// `getNextMapId()`.
    pub fn get_next_map_id(&mut self) -> mc_protocol::components::MapId {
        self.last_map_id = self.last_map_id.wrapping_add(1);
        let id = mc_protocol::components::MapId(self.last_map_id);
        self.set_dirty();
        id
    }

    pub fn decode(tag: &Tag) -> DecodeResult<MapIndex> {
        let m = codec::compound(tag)?;
        Ok(MapIndex { dirty: false, last_map_id: codec::optional(m, "map", codec::int)?.unwrap_or(-1) })
    }

    pub fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        if self.last_map_id != -1 {
            m.put_int("map", self.last_map_id);
        }
        Ok(Tag::Compound(m))
    }

    /// `MapIndex.TYPE`.
    pub fn type_() -> SavedDataType<MapIndex> {
        SavedDataType::new(Identifier::parse("minecraft:maps/last_id").expect("static id"), MapIndex::default, MapIndex::decode, MapIndex::encode)
    }
}
