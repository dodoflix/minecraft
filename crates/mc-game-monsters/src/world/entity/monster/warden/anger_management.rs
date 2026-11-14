//! `AngerManagement`: the warden's per-suspect anger.
//!
//! Vanilla keeps the suspect `Entity` objects (`Object2IntMap<Entity>`, `ArrayList<Entity>`); entities are referenced by id
//! here ([`Suspect`] also keeps the UUID and the player flag, which the codec and the sorter read, so they stay answerable when
//! the entity is out of the level). The `filter` predicate of the constructor (`Warden::canTargetEntity`) is passed to the
//! methods that use it, because the warden owns this object and cannot be borrowed twice.
//!
//! `angerByUuid` is an [`FastOpenHashMap`] keyed by the UUID's `hashCode()`, so the saved `suspects` list of not-yet-resolved
//! UUIDs comes out in vanilla's hash order.
//!
//! shortcut: an entity that vanished from the level is kept by UUID only when it is a player (vanilla: removal reason
//! `CHANGED_DIMENSION`/`UNLOADED_*`, the reason is not observable here once the entity is gone); upgrade when the level keeps
//! removal reasons. `convertFromUuids` removes from the map while iterating: the order of the entries that fastutil's iterator
//! revisits after a wrapped backward shift is not modelled.

use crate::world::entity::monster::warden::anger_level::AngerLevel;
use mc_core::java_collections::FastOpenHashMap;
use mc_core::math::mth;
use mc_core::random;
use mc_core::uuid_util::{uuid_from_int_array, uuid_to_int_array};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_nbt::{CompoundTag, ListTag, Tag};
use uuid::Uuid;

/// `AngerManagement.CONVERSION_DELAY`.
pub const CONVERSION_DELAY: i32 = 2;
/// `AngerManagement.MAX_ANGER`.
pub const MAX_ANGER: i32 = 150;
/// `DEFAULT_ANGER_DECREASE`.
const DEFAULT_ANGER_DECREASE: i32 = 1;

/// A suspect entity as `AngerManagement` needs it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Suspect {
    pub id: EntityId,
    pub uuid: Uuid,
    pub is_player: bool,
}

impl Suspect {
    pub fn of(entity: &dyn Entity) -> Suspect {
        Suspect {
            id: entity.id(),
            uuid: entity.uuid(),
            is_player: entity.is_player(),
        }
    }
}

/// `UUID.hashCode()`.
fn uuid_hash(uuid: &Uuid) -> i32 {
    let (most, least) = uuid.as_u64_pair();
    let both = most ^ least;
    ((both >> 32) as i32) ^ (both as i32)
}

/// `AngerManagement`.
#[derive(Debug)]
pub struct AngerManagement {
    conversion_delay: i32,
    highest_anger: i32,
    /// `suspects`: kept sorted by the sorter.
    pub suspects: Vec<Suspect>,
    /// `angerBySuspect`.
    pub anger_by_suspect: Vec<(Suspect, i32)>,
    /// `angerByUuid`.
    pub anger_by_uuid: FastOpenHashMap<Uuid, i32>,
}

impl Default for AngerManagement {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl AngerManagement {
    /// `new AngerManagement(filter, angerByUuid)`: `conversionDelay` is `Mth.randomBetweenInclusive(
    /// RandomSource.createThreadLocalInstance(), 0, 2)`.
    pub fn new(anger_by_uuid: Vec<(Uuid, i32)>) -> Self {
        let mut random = random::create_thread_local_instance_new();
        let mut management = Self::with_conversion_delay(anger_by_uuid, 0);
        management.conversion_delay = mth::random_between_inclusive(&mut random, 0, 2);
        management
    }

    /// The constructor with the delay given (tests).
    pub fn with_conversion_delay(anger_by_uuid: Vec<(Uuid, i32)>, conversion_delay: i32) -> Self {
        let mut by_uuid = FastOpenHashMap::new(anger_by_uuid.len(), uuid_hash);
        for (uuid, anger) in anger_by_uuid {
            by_uuid.put(uuid, anger);
        }
        AngerManagement {
            conversion_delay,
            highest_anger: 0,
            suspects: Vec::new(),
            anger_by_suspect: Vec::new(),
            anger_by_uuid: by_uuid,
        }
    }

    /// `highestAnger`.
    pub fn highest_anger(&self) -> i32 {
        self.highest_anger
    }

    fn anger_of(&self, id: EntityId) -> Option<i32> {
        self.anger_by_suspect
            .iter()
            .find(|(s, _)| s.id == id)
            .map(|(_, a)| *a)
    }

    /// `createUuidAngerPairs()`: the stream over `angerByUuid` walks the table from its first slot, the opposite of the map
    /// iterator that the other methods use.
    fn create_uuid_anger_pairs(&self) -> Vec<(Uuid, i32)> {
        let mut by_uuid: Vec<(Uuid, i32)> =
            self.anger_by_uuid.iter().map(|(u, a)| (*u, *a)).collect();
        by_uuid.reverse();
        self.suspects
            .iter()
            .map(|s| (s.uuid, self.anger_of(s.id).unwrap_or(0)))
            .chain(by_uuid)
            .collect()
    }

    /// `AngerManagement.codec(filter)` (encode).
    pub fn to_nbt(&self) -> CompoundTag {
        let pairs = self.create_uuid_anger_pairs();
        let mut tag = CompoundTag::new();
        // `lenientOptionalFieldOf("suspects", List.of())` leaves the field out while the list is the default
        if pairs.is_empty() {
            return tag;
        }
        let mut list = ListTag::new();
        for (uuid, anger) in pairs {
            let mut pair = CompoundTag::new();
            pair.put("uuid", Tag::IntArray(uuid_to_int_array(&uuid).to_vec()));
            pair.put_int("anger", anger);
            list.push(Tag::Compound(pair));
        }
        tag.put("suspects", Tag::List(list));
        tag
    }

    /// `AngerManagement.codec(filter)` (decode): `suspects` is a lenient optional field, one invalid pair drops the list.
    pub fn from_nbt(tag: &CompoundTag) -> AngerManagement {
        let pairs = tag.get_list("suspects").and_then(|list| {
            list.iter()
                .map(|t| {
                    let pair = t.as_compound()?;
                    let uuid = match pair.get("uuid")? {
                        Tag::IntArray(a) => <[i32; 4]>::try_from(a.clone())
                            .ok()
                            .map(uuid_from_int_array)?,
                        _ => return None,
                    };
                    let anger = pair.get_int("anger").filter(|a| *a >= 0)?;
                    Some((uuid, anger))
                })
                .collect::<Option<Vec<_>>>()
        });
        AngerManagement::new(pairs.unwrap_or_default())
    }

    /// `tick(level, validEntity)`.
    pub fn tick(&mut self, level: &ServerLevel, valid_entity: &dyn Fn(&dyn Entity) -> bool) {
        self.conversion_delay -= 1;
        if self.conversion_delay <= 0 {
            self.convert_from_uuids(level);
            self.conversion_delay = CONVERSION_DELAY;
        }
        let keys: Vec<Uuid> = self.anger_by_uuid.iter().map(|(u, _)| *u).collect();
        for uuid in keys {
            let anger = self.anger_by_uuid.get(&uuid).copied().unwrap_or(0);
            if anger <= 1 {
                self.anger_by_uuid.remove_during_iteration(&uuid);
            } else if let Some(slot) = self.anger_by_uuid.get_mut(&uuid) {
                *slot = anger - DEFAULT_ANGER_DECREASE;
            }
        }
        let mut i = 0;
        while i < self.anger_by_suspect.len() {
            let (suspect, anger) = self.anger_by_suspect[i];
            let entity = level.get_entity(suspect.id);
            // an entity that is taken out of the level (not found, but still known) cannot be tested: it stays
            let present = entity.is_some() || level.is_entity_loaded(suspect.uuid);
            if anger > 1 && present && entity.is_none_or(valid_entity) {
                self.anger_by_suspect[i].1 = anger - DEFAULT_ANGER_DECREASE;
                i += 1;
            } else {
                self.suspects.retain(|s| s.id != suspect.id);
                self.anger_by_suspect.remove(i);
                if anger > 1 && !present && suspect.is_player {
                    self.anger_by_uuid.put(suspect.uuid, anger - 1);
                }
            }
        }
        self.sort_and_update_highest_anger();
    }

    /// `sortAndUpdateHighestAnger()`: the sorter puts angry suspects first, players before other entities, then by anger
    /// descending (stable); `highestAnger` becomes the highest anger of the suspects (every suspect takes part in a
    /// comparison when there are two or more, and a single one is read directly).
    fn sort_and_update_highest_anger(&mut self) {
        let anger_of = |id: EntityId, map: &[(Suspect, i32)]| {
            map.iter().find(|(s, _)| s.id == id).map_or(0, |(_, a)| *a)
        };
        let map = std::mem::take(&mut self.anger_by_suspect);
        self.suspects.sort_by(|e1, e2| {
            if e1.id == e2.id {
                return std::cmp::Ordering::Equal;
            }
            let anger1 = anger_of(e1.id, &map);
            let anger2 = anger_of(e2.id, &map);
            let angry_at1 = AngerLevel::by_anger(anger1).is_angry();
            let angry_at2 = AngerLevel::by_anger(anger2).is_angry();
            if angry_at1 != angry_at2 {
                if angry_at1 {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            } else if e1.is_player != e2.is_player {
                if e1.is_player {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            } else {
                anger2.cmp(&anger1)
            }
        });
        self.highest_anger = self
            .suspects
            .iter()
            .map(|s| anger_of(s.id, &map))
            .max()
            .unwrap_or(0);
        self.anger_by_suspect = map;
    }

    /// `convertFromUuids(level)`.
    fn convert_from_uuids(&mut self, level: &ServerLevel) {
        let entries: Vec<(Uuid, i32)> = self.anger_by_uuid.iter().map(|(u, a)| (*u, *a)).collect();
        for (uuid, anger) in entries {
            if let Some(entity) = level.get_entity_by_uuid(uuid) {
                let suspect = Suspect::of(entity);
                match self
                    .anger_by_suspect
                    .iter_mut()
                    .find(|(s, _)| s.id == suspect.id)
                {
                    Some(e) => e.1 = anger,
                    None => self.anger_by_suspect.push((suspect, anger)),
                }
                self.suspects.push(suspect);
                self.anger_by_uuid.remove_during_iteration(&uuid);
            }
        }
    }

    /// `increaseAnger(entity, increment)`.
    pub fn increase_anger(&mut self, entity: &dyn Entity, increment: i32) -> i32 {
        let suspect = Suspect::of(entity);
        let new_suspect = self.anger_of(suspect.id).is_none();
        let mut current_anger = match self
            .anger_by_suspect
            .iter_mut()
            .find(|(s, _)| s.id == suspect.id)
        {
            Some(e) => {
                e.1 = MAX_ANGER.min(e.1.wrapping_add(increment));
                e.1
            }
            None => {
                let anger = MAX_ANGER.min(increment);
                self.anger_by_suspect.push((suspect, anger));
                anger
            }
        };
        if new_suspect {
            let serialized_anger = self.anger_by_uuid.remove(&suspect.uuid).unwrap_or(0);
            current_anger = current_anger.wrapping_add(serialized_anger);
            if let Some(e) = self
                .anger_by_suspect
                .iter_mut()
                .find(|(s, _)| s.id == suspect.id)
            {
                e.1 = current_anger;
            }
            self.suspects.push(suspect);
        }
        self.sort_and_update_highest_anger();
        current_anger
    }

    /// `clearAnger(entity)`.
    pub fn clear_anger(&mut self, entity: EntityId) {
        self.anger_by_suspect.retain(|(s, _)| s.id != entity);
        self.suspects.retain(|s| s.id != entity);
        self.sort_and_update_highest_anger();
    }

    /// `getTopSuspect()`: the first suspect the filter accepts.
    fn get_top_suspect(
        &self,
        level: &ServerLevel,
        filter: &dyn Fn(&dyn Entity) -> bool,
    ) -> Option<EntityId> {
        self.suspects
            .iter()
            .find(|s| {
                level
                    .get_entity_or_recently_removed(s.id)
                    .is_some_and(filter)
            })
            .map(|s| s.id)
    }

    /// `getActiveAnger(currentTarget)`.
    pub fn get_active_anger(&self, current_target: Option<EntityId>) -> i32 {
        match current_target {
            None => self.highest_anger,
            Some(target) => self.anger_of(target).unwrap_or(0),
        }
    }

    /// `getActiveEntity()`: the top suspect when it is a living entity.
    pub fn get_active_entity(
        &self,
        level: &ServerLevel,
        filter: &dyn Fn(&dyn Entity) -> bool,
    ) -> Option<EntityId> {
        self.get_top_suspect(level, filter).filter(|id| {
            level
                .get_entity_or_recently_removed(*id)
                .is_some_and(|e| e.living().is_some())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nbt_round_trip_keeps_unresolved_uuids() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let management = AngerManagement::with_conversion_delay(vec![(a, 7), (b, 150)], 2);
        let tag = management.to_nbt();
        let read = AngerManagement::from_nbt(&tag);
        let mut read: Vec<_> = read.anger_by_uuid.iter().map(|(u, a)| (*u, *a)).collect();
        read.sort();
        assert_eq!(read, vec![(a, 7), (b, 150)]);
    }

    #[test]
    fn invalid_pair_drops_the_whole_list() {
        let mut list = ListTag::new();
        let mut pair = CompoundTag::new();
        pair.put("uuid", Tag::IntArray(vec![0, 0, 0, 1]));
        pair.put_int("anger", -1);
        list.push(Tag::Compound(pair));
        let mut tag = CompoundTag::new();
        tag.put("suspects", Tag::List(list));
        assert!(AngerManagement::from_nbt(&tag).anger_by_uuid.is_empty());
    }

    #[test]
    fn empty_anger_saves_no_suspects_field() {
        assert!(AngerManagement::default().to_nbt().is_empty());
    }
}
