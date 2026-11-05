//! `Raids` (`data/minecraft/raids.dat`, one per dimension): the saved-data shell of the raid system.
//!
//! The raid list is kept as the raw per-raid NBT (`RaidWithId`: `id` plus the `Raid` fields), so a world's raids
//! round-trip untouched; `Raid`'s own logic (`Raid.tick`, `createOrExtendRaid`, ...) belongs to the raid/mobs slice and
//! has no model here: [`Raids::tick`] advances the counter and, like vanilla, drops every raid while `doRaids` is off.

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, ListTag, Tag};

use super::{SavedData, SavedDataType};
use crate::impl_saved_data;
use crate::util::codec::{self, DataResult, DecodeResult};

#[derive(Debug, Clone)]
pub struct Raids {
    dirty: bool,
    /// `(raid id, RaidWithId compound)` in map order.
    raids: Vec<(i32, CompoundTag)>,
    next_id: i32,
    tick: i32,
}

impl_saved_data!(Raids);

impl Default for Raids {
    /// `new Raids()`: `setDirty()` so that a fresh level always writes its file.
    fn default() -> Self {
        Raids { dirty: true, raids: Vec::new(), next_id: 1, tick: 0 }
    }
}

impl Raids {
    pub fn tick_counter(&self) -> i32 {
        self.tick
    }

    pub fn raid_count(&self) -> usize {
        self.raids.len()
    }

    /// `tick(level)` with `doRaids` = `raids_enabled`.
    pub fn tick(&mut self, raids_enabled: bool) {
        self.tick = self.tick.wrapping_add(1);
        if !raids_enabled && !self.raids.is_empty() {
            // `raid.stop()` -> `isStopped()` -> removed
            self.raids.clear();
            self.set_dirty_to(true);
        }
        if self.tick % 200 == 0 {
            self.set_dirty_to(true);
        }
    }

    pub fn decode(tag: &Tag) -> DecodeResult<Raids> {
        let m = codec::compound(tag)?;
        let raids = match codec::optional(m, "raids", codec::list)? {
            None => Vec::new(),
            Some(list) => list
                .iter()
                .map(|entry| {
                    let c = codec::compound(entry)?;
                    Ok((codec::int(codec::field(c, "id")?)?, c.clone()))
                })
                .collect::<DataResult<Vec<_>>>()?,
        };
        let next_id = codec::int(codec::field(m, "next_id")?)?;
        let tick = codec::int(codec::field(m, "tick")?)?;
        Ok(Raids { dirty: false, raids, next_id, tick })
    }

    pub fn encode(&self) -> DataResult<Tag> {
        let mut m = CompoundTag::new();
        // `optionalFieldOf("raids", List.of())` omits an empty list.
        if !self.raids.is_empty() {
            let mut list = ListTag::new();
            for (_, raid) in &self.raids {
                list.push(Tag::Compound(raid.clone()));
            }
            m.put("raids", Tag::List(list));
        }
        m.put_int("next_id", self.next_id);
        m.put_int("tick", self.tick);
        Ok(Tag::Compound(m))
    }

    pub fn type_() -> SavedDataType<Raids> {
        SavedDataType::new(Identifier::parse("minecraft:raids").expect("static id"), Raids::default, Raids::decode, Raids::encode)
    }
}
