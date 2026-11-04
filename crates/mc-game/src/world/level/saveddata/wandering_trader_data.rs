//! `WanderingTraderData` (`data/minecraft/wandering_trader.dat`).

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};

use super::SavedDataType;
use crate::impl_saved_data;
use crate::util::codec::{self, DataResult, DecodeResult};

#[derive(Debug, Clone)]
pub struct WanderingTraderData {
    dirty: bool,
    spawn_delay: i32,
    spawn_chance: i32,
}

impl_saved_data!(WanderingTraderData);

impl Default for WanderingTraderData {
    fn default() -> Self {
        WanderingTraderData::new(24000, 25)
    }
}

impl WanderingTraderData {
    pub fn new(spawn_delay: i32, spawn_chance: i32) -> Self {
        WanderingTraderData { dirty: false, spawn_delay, spawn_chance }
    }

    pub fn spawn_delay(&self) -> i32 {
        self.spawn_delay
    }
    pub fn set_spawn_delay(&mut self, v: i32) {
        if self.spawn_delay != v {
            self.spawn_delay = v;
            self.set_dirty_to(true);
        }
    }
    pub fn spawn_chance(&self) -> i32 {
        self.spawn_chance
    }
    pub fn set_spawn_chance(&mut self, v: i32) {
        if self.spawn_chance != v {
            self.spawn_chance = v;
            self.set_dirty_to(true);
        }
    }

    pub fn decode(tag: &Tag) -> DecodeResult<WanderingTraderData> {
        let m = codec::compound(tag)?;
        Ok(WanderingTraderData::new(
            codec::optional(m, "spawn_delay", codec::int)?.unwrap_or(24000),
            codec::optional(m, "spawn_chance", codec::int)?.unwrap_or(25),
        ))
    }

    pub fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        // `optionalFieldOf(name, default)` omits a value equal to its default.
        if self.spawn_delay != 24000 {
            m.put_int("spawn_delay", self.spawn_delay);
        }
        if self.spawn_chance != 25 {
            m.put_int("spawn_chance", self.spawn_chance);
        }
        Ok(Tag::Compound(m))
    }

    pub fn type_() -> SavedDataType<WanderingTraderData> {
        SavedDataType::new(Identifier::parse("minecraft:wandering_trader").expect("static id"), WanderingTraderData::default, WanderingTraderData::decode, WanderingTraderData::encode)
    }
}
