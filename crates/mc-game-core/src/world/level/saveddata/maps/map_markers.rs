//! `MapBanner`, `MapFrame` and `MapIndex` (the `maps/last_id` counter).

use super::map_decoration::{MapDecorationType, banner_decoration};
use crate::impl_saved_data;
use crate::world::block_entity::BlockEntity;
use crate::world::level::BlockGetter;
use crate::world::level::saveddata::{SavedData, SavedDataType};
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_dfu::{Codec, INT};
use mc_nbt::CompoundTag;
use mc_protocol::components::{DyeColor, block_pos_codec, component_codec};
use mc_text::Component;

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
        let color = path
            .strip_suffix("_wall_banner")
            .or_else(|| path.strip_suffix("_banner"))
            .and_then(DyeColor::from_name)?;
        Some(MapBanner {
            pos,
            color,
            name: custom_name(be),
        })
    }

    pub fn get_decoration(&self) -> MapDecorationType {
        banner_decoration(self.color)
    }

    /// `getId()`.
    pub fn get_id(&self) -> String {
        format!("banner-{},{},{}", self.pos.x(), self.pos.y(), self.pos.z())
    }

    /// `MapBanner.CODEC`.
    pub fn codec() -> Codec<MapBanner> {
        mc_dfu::record!(MapBanner {
            pos: block_pos_codec().field_of("pos"),
            color: DyeColor::codec().lenient_optional_field_of_default("color", DyeColor::White),
            name: component_codec().lenient_optional_field_of("name"),
        })
        .codec()
    }
}

/// `BannerBlockEntity.getCustomName()`: the `CustomName` of its saved data.
fn custom_name(be: &dyn BlockEntity) -> Option<Component> {
    let mut tag = CompoundTag::new();
    be.save_additional(&mut tag);
    tag.get("CustomName")
        .and_then(|t| Component::from_nbt(t).ok())
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

    /// `MapFrame.CODEC`.
    pub fn codec() -> Codec<MapFrame> {
        mc_dfu::record!(MapFrame {
            pos: block_pos_codec().field_of("pos"),
            rotation: INT.field_of("rotation"),
            entity_id: INT.field_of("entity_id"),
        })
        .codec()
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
        MapIndex {
            dirty: false,
            last_map_id: -1,
        }
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

    /// `MapIndex.CODEC`.
    pub fn codec() -> Codec<MapIndex> {
        static CODEC: std::sync::LazyLock<Codec<MapIndex>> =
            std::sync::LazyLock::new(MapIndex::build_codec);
        CODEC.clone()
    }

    fn build_codec() -> Codec<MapIndex> {
        mc_dfu::record!(Packed {
            map: INT.optional_field_of_default("map", -1),
        })
        .codec()
        .xmap(
            |p| MapIndex {
                dirty: false,
                last_map_id: p.map,
            },
            |i: &MapIndex| Packed { map: i.last_map_id },
        )
    }

    /// `MapIndex.TYPE`.
    pub fn type_() -> SavedDataType<MapIndex> {
        SavedDataType::from_codec(
            Identifier::parse("minecraft:maps/last_id").expect("static id"),
            MapIndex::default,
            MapIndex::codec(),
            mc_datafix::DataFixTypes::SAVED_DATA_MAP_INDEX,
        )
    }
}

/// The serialized shape of [`MapIndex`].
struct Packed {
    map: i32,
}
