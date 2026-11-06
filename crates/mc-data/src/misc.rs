//! Other code-defined vanilla tables that are not part of the data pack.

use crate::enums::{HeightmapTypes, HeightmapUsage, NoteBlockInstrument};
use crate::ids::{Block, SoundEvent};

/// `GameRule` default value (bool or int rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameRuleValue {
    Bool(bool),
    Int(i32),
}

/// Indexed by the `minecraft:game_rule` registry id.
#[derive(Debug)]
pub struct GameRuleData {
    pub default: GameRuleValue,
    /// Integer rules only (`IntegerArgumentType` bounds).
    pub min: Option<i32>,
    pub max: Option<i32>,
    /// `GameRuleCategory` id (`minecraft:updates`, ...).
    pub category: &'static str,
}

#[derive(Debug)]
pub struct DyeColorData {
    pub id: u8,
    pub name: &'static str,
    /// `MapColor` ids.
    pub map_color: u8,
    pub terracotta_color: u8,
    pub texture_diffuse_color: i32,
    pub firework_color: i32,
    pub text_color: i32,
}

#[derive(Debug)]
pub struct NoteBlockInstrumentData {
    pub instrument: NoteBlockInstrument,
    pub sound: SoundEvent,
    pub tunable: bool,
    pub has_custom_sound: bool,
    pub works_above_note_block: bool,
}

#[derive(Debug)]
pub struct HeightmapTypeData {
    pub ty: HeightmapTypes,
    pub serialized_name: &'static str,
    pub usage: HeightmapUsage,
}

/// `PoiType` (`PoiTypes`): matching block state ids (sorted), ticket count, search range.
#[derive(Debug)]
pub struct PoiTypeData {
    pub name: &'static str,
    pub matching_states: &'static [u16],
    pub max_tickets: i32,
    pub valid_range: i32,
}

/// One `SynchedEntityData.defineId` accessor: id within the entity class hierarchy and its serializer.
#[derive(Debug)]
pub struct EntityDataAccessorDef {
    /// Simple name of the declaring entity class.
    pub class: &'static str,
    pub field: &'static str,
    pub id: u8,
    /// `EntityDataSerializers` constant name.
    pub serializer: &'static str,
    pub serializer_id: u8,
}

include!(concat!(env!("OUT_DIR"), "/misc.rs"));

fn lookup<V: Copy>(table: &'static [(Block, V)], block: Block) -> Option<V> {
    table
        .binary_search_by_key(&block.id(), |(b, _)| b.id())
        .ok()
        .map(|i| table[i].1)
}

/// `FireBlock` `(igniteOdds, burnOdds)` of a block (flammability), if flammable.
pub fn fire_odds(block: Block) -> Option<(i32, i32)> {
    FIRE_ODDS
        .binary_search_by_key(&block.id(), |(b, _, _)| b.id())
        .ok()
        .map(|i| (FIRE_ODDS[i].1, FIRE_ODDS[i].2))
}

/// `WeatheringCopper.NEXT_BY_BLOCK`.
pub fn weathering_next(block: Block) -> Option<Block> {
    lookup(&WEATHERING_NEXT, block)
}

/// `HoneycombItem.WAXABLES`.
pub fn waxed(block: Block) -> Option<Block> {
    lookup(&WAXABLES, block)
}

/// `FlowerPotBlock.POTTED_BY_CONTENT`: the potted variant for a flower pot content.
pub fn potted_by_content(content: Block) -> Option<Block> {
    lookup(&POTTED_BY_CONTENT, content)
}
