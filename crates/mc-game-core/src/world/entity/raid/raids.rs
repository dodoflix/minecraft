//! `net.minecraft.world.entity.raid.Raids` (`data/minecraft/raids.dat` of each dimension) and the `ServerLevel` accessors
//! (`getRaids`, `getRaidAt`, `isRaided`) over it.

use super::raid::{Raid, RaidMeta, VALID_RAID_RADIUS_SQR};
use super::raider;
use crate::impl_saved_data;
use crate::server::level::ServerLevel;
use crate::server::level::fastutil::LongOpenHashMap;
use crate::util::codec::{DataResult, DecodeResult};
use crate::world::entity::{Entity as _, ServerPlayer};
use crate::world::level::saveddata::{SavedData, SavedDataType, decode_codec, encode_codec};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_dfu::{Codec, INT};
use mc_nbt::Tag;
use std::sync::LazyLock;

/// A raid in the map. `raid` is `None` while it is checked out (it is being ticked or changed together with the level);
/// the center/activity stay readable for `getNearbyRaid`.
struct RaidSlot {
    raid: Option<Raid>,
    meta: RaidMeta,
}

/// `Raids`.
pub struct Raids {
    dirty: bool,
    raid_map: LongOpenHashMap<RaidSlot>,
    next_id: i32,
    tick: i32,
}

impl_saved_data!(Raids);

impl Default for Raids {
    fn default() -> Self {
        // `Raids()` constructor: `setDirty()`
        Raids {
            dirty: true,
            raid_map: LongOpenHashMap::new_int(),
            next_id: 1,
            tick: 0,
        }
    }
}

static RAIDS_TYPE: LazyLock<SavedDataType<Raids>> = LazyLock::new(|| {
    SavedDataType::from_codec(
        Identifier::parse("minecraft:raids").expect("static id"),
        Raids::default,
        Raids::codec(),
        mc_datafix::DataFixTypes::SAVED_DATA_RAIDS,
    )
});

/// `Raids.RaidWithId`.
struct RaidWithId {
    id: i32,
    raid: Raid,
}

/// The serialized shape of [`Raids`] that is decoded.
struct PackedRaids {
    raids: Option<Vec<RaidWithId>>,
    next_id: i32,
    tick: i32,
}

/// `Raids.TYPE`.
pub fn raids_type() -> &'static SavedDataType<Raids> {
    &RAIDS_TYPE
}

/// `Raids.getNearbyRaid(pos, maxDistSqr)` over `(id, center, active)` entries in map order: the id of the closest active
/// raid strictly closer than `max_dist_sqr` (the first of equally close ones).
pub fn nearby_raid_id(
    metas: impl Iterator<Item = RaidMeta>,
    pos: BlockPos,
    max_dist_sqr: i32,
) -> Option<i32> {
    let mut closest = None;
    let mut closest_distance_sqr = f64::from(max_dist_sqr);
    for meta in metas {
        let distance = meta.center.dist_sqr(&pos);
        if meta.active && distance < closest_distance_sqr {
            closest = Some(meta.id);
            closest_distance_sqr = distance;
        }
    }
    closest
}

pub(super) fn set_dirty(level: &mut ServerLevel) {
    level.get_raids_mut().set_dirty();
}

impl Raids {
    /// `get(raidId)`: `None` for an unknown raid or while it is checked out.
    pub fn get(&self, raid_id: i32) -> Option<&Raid> {
        self.raid_map
            .get(i64::from(raid_id))
            .and_then(|s| s.raid.as_ref())
    }

    pub fn get_mut(&mut self, raid_id: i32) -> Option<&mut Raid> {
        self.raid_map
            .get_mut(i64::from(raid_id))
            .and_then(|s| s.raid.as_mut())
    }

    /// Whether a raid with this id exists (also while checked out).
    pub fn contains(&self, raid_id: i32) -> bool {
        self.raid_map.contains_key(i64::from(raid_id))
    }

    /// `getId(raid)` is the raid's own id here.
    pub fn get_id(&self, raid: &Raid) -> Option<i32> {
        self.contains(raid.id()).then(|| raid.id())
    }

    fn view(&self) -> Vec<RaidMeta> {
        self.raid_map
            .keys()
            .into_iter()
            .filter_map(|k| self.raid_map.get(k))
            .map(|s| s.meta)
            .collect()
    }

    /// Every raid's id in map iteration order.
    pub fn ids(&self) -> Vec<i32> {
        self.raid_map.keys().into_iter().map(|k| k as i32).collect()
    }

    /// `getNearbyRaid(pos, maxDistSqr)` (the id).
    pub fn get_nearby_raid(&self, pos: BlockPos, max_dist_sqr: i32) -> Option<i32> {
        nearby_raid_id(self.view().into_iter(), pos, max_dist_sqr)
    }

    /// `getRaidCentersInChunk(chunkPos)`.
    pub fn get_raid_centers_in_chunk(&self, chunk_x: i32, chunk_z: i32) -> Vec<BlockPos> {
        // `raidMap.values().stream()`
        self.raid_map
            .keys_in_stream_order()
            .into_iter()
            .filter_map(|k| self.raid_map.get(k))
            .map(|s| s.meta.center)
            .filter(|c| c.x() >> 4 == chunk_x && c.z() >> 4 == chunk_z)
            .collect()
    }

    /// Checks the raid out (leaving its meta behind).
    fn check_out(&mut self, raid_id: i32) -> Option<Raid> {
        self.raid_map
            .get_mut(i64::from(raid_id))
            .and_then(|s| s.raid.take())
    }

    fn check_in(&mut self, raid: Raid) {
        if let Some(slot) = self.raid_map.get_mut(i64::from(raid.id())) {
            slot.meta = raid.meta();
            slot.raid = Some(raid);
        }
    }

    fn put(&mut self, id: i32, mut raid: Raid) {
        raid.set_id(id);
        let meta = raid.meta();
        self.raid_map.insert(
            i64::from(id),
            RaidSlot {
                raid: Some(raid),
                meta,
            },
        );
    }

    fn get_unique_id(&mut self) -> i32 {
        self.next_id = self.next_id.wrapping_add(1);
        self.next_id
    }

    // ---- saved data ----------------------------------------------------------------------------------------------

    /// `Raids.CODEC`.
    fn codec() -> Codec<Raids> {
        // `RaidWithId.CODEC`
        let entry = mc_dfu::record!(RaidWithId {
            id: INT.field_of("id"),
            raid: Raid::map_codec(),
        })
        .codec();
        let packed = mc_dfu::record!(PackedRaids {
            raids: entry.list_of().optional_field_of("raids"),
            next_id: INT.field_of("next_id"),
            tick: INT.field_of("tick"),
        })
        .codec();
        // The raids own their state, which the map cannot lend: the encoder writes the same shape directly.
        Codec::from_fns(
            move |ops, raids: &Raids, prefix| {
                // `optionalFieldOf("raids", List.of())` omits an empty list; `int2ObjectEntrySet().stream()`
                let mut list = ops.list_builder();
                let mut any = false;
                for key in raids.raid_map.keys_in_stream_order() {
                    let Some(raid) = raids.raid_map.get(key).and_then(|slot| slot.raid.as_ref())
                    else {
                        continue;
                    };
                    let mut entry = ops.map_builder();
                    entry.add_result("id", INT.encode_start(ops, &(key as i32)));
                    Raid::map_codec().encode(ops, raid, &mut entry);
                    list.add_result(entry.build(&ops.empty()));
                    any = true;
                }
                let mut root = ops.map_builder();
                if any {
                    root.add_result("raids", list.build(&ops.empty()));
                }
                root.add_result("next_id", INT.encode_start(ops, &raids.next_id));
                root.add_result("tick", INT.encode_start(ops, &raids.tick));
                root.build(&prefix)
            },
            move |ops, input| {
                packed.decode(ops, input).map(|(packed, rest)| {
                    let mut raids = Raids {
                        dirty: false,
                        raid_map: LongOpenHashMap::new_int(),
                        next_id: packed.next_id,
                        tick: packed.tick,
                    };
                    for entry in packed.raids.unwrap_or_default() {
                        raids.put(entry.id, entry.raid);
                    }
                    (raids, rest)
                })
            },
        )
    }

    #[expect(
        clippy::result_large_err,
        reason = "DecodeResult<T> carries the partial value by value (DataResult.error(msg, partial))"
    )]
    fn decode(tag: &Tag) -> DecodeResult<Raids> {
        decode_codec(&Raids::codec(), tag)
    }

    fn encode(&self) -> DataResult<Tag> {
        encode_codec(&Raids::codec(), self)
    }

    // ---- ticking --------------------------------------------------------------------------------------------------

    /// `createOrExtendRaid(player, raidPosition)`: the id of the raid.
    pub fn create_or_extend_raid(
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        raid_position: BlockPos,
    ) -> Option<i32> {
        if player.is_spectator() {
            return None;
        }
        if !level.raids() {
            return None;
        }
        if !level.attribute_bool(
            crate::world::attribute::attributes::CAN_START_RAID,
            raid_position,
        ) {
            return None;
        }
        let posses = super::raid_world::occupied_village_pois(level, raid_position, 64);
        let mut count = 0;
        let mut pos_totals = Vec3::ZERO;
        for p in &posses {
            let pos = *p;
            pos_totals = pos_totals.add(Vec3::new(
                f64::from(pos.x()),
                f64::from(pos.y()),
                f64::from(pos.z()),
            ));
            count += 1;
        }
        let raid_center_pos = if count > 0 {
            pos_totals = pos_totals.scale(1.0 / f64::from(count));
            BlockPos::containing_position(&pos_totals)
        } else {
            raid_position
        };

        let id = match level.get_raid_at(raid_center_pos) {
            Some(raid) => raid.id(),
            None => {
                let difficulty = level.difficulty();
                let mut raid = Raid::new(raid_center_pos, difficulty);
                let raids = level.get_raids_mut();
                // `!raid.isStarted() && !raidMap.containsValue(raid)`: a new raid is never started
                let id = raids.get_unique_id();
                raid.set_id(id);
                raids.put(id, raid);
                id
            }
        };
        level.with_raid(id, |level, raid| {
            if !raid.is_started() || raid.get_raid_omen_level() < raid.get_max_raid_omen_level() {
                raid.absorb_raid_omen(level, player);
            }
        });
        level.get_raids_mut().set_dirty();
        Some(id)
    }
}

/// `Raids.canJoinRaid(raider)`.
pub fn can_join_raid(raider: &dyn crate::world::entity::Entity) -> bool {
    raider.is_alive()
        && raider::can_join_raid(raider)
        && raider
            .living()
            .is_some_and(|l| l.no_action_time <= super::raid::MAX_NO_ACTION_TIME)
}

/// `Raids.tick(level)`.
pub fn tick_raids(level: &mut ServerLevel) {
    let ids = {
        let raids = level.get_raids_mut();
        raids.tick = raids.tick.wrapping_add(1);
        raids.ids()
    };
    for id in ids {
        let rule = level.raids();
        let Some(mut raid) = level.get_raids_mut().check_out(id) else {
            continue;
        };
        if !rule {
            raid.stop();
        }
        if raid.is_stopped() {
            let raids = level.get_raids_mut();
            raids.raid_map.remove(i64::from(id));
            raids.set_dirty();
            // a raider keeps the stopped raid but `Raids.getId` no longer finds it: no `RaidId` when it is saved
            for raider in raid.get_all_raiders() {
                level.try_with_entity(raider, |_, e| raider::mark_raid_ended(e));
            }
        } else {
            let view = level.get_raids_mut().view();
            raid.tick(level, &view);
            level.get_raids_mut().check_in(raid);
        }
    }
    let raids = level.get_raids_mut();
    if raids.tick % 200 == 0 {
        raids.set_dirty();
    }
}

impl ServerLevel {
    /// Loads (or creates) the raids of this dimension (`this.raids = getDataStorage().computeIfAbsent(Raids.TYPE)`).
    pub fn install_raids(&mut self) {
        self.get_raids_mut();
    }

    /// The contents of `raids.dat`: the raids as they are saved.
    pub fn raids_saved_form(&mut self) -> DataResult<Tag> {
        self.get_raids_mut().encode()
    }

    /// Replaces the raids with their saved form read back, as a restart of the server does: the new raids hold no raider until
    /// each raider is loaded again.
    pub fn reload_raids_from_save(&mut self) -> DataResult<()> {
        let saved = self.raids_saved_form()?;
        let fresh = Raids::decode(&saved).map_err(|partial| partial.error)?;
        self.chunk_source.data_storage.set(raids_type(), fresh);
        Ok(())
    }

    /// `getRaids()`.
    pub fn get_raids_mut(&mut self) -> &mut Raids {
        self.chunk_source
            .data_storage
            .compute_if_absent(raids_type())
    }

    /// `getRaids()` for shared readers; `None` until the raids were loaded ([`ServerLevel::install_raids`]).
    pub fn get_raids(&self) -> Option<&Raids> {
        self.chunk_source.data_storage.peek(raids_type())
    }

    /// `getRaidAt(pos)`: the closest active raid within 96 blocks. `None` while that raid is checked out.
    pub fn get_raid_at(&self, pos: BlockPos) -> Option<&Raid> {
        let raids = self.get_raids()?;
        raids
            .get_nearby_raid(pos, VALID_RAID_RADIUS_SQR)
            .and_then(|id| raids.get(id))
    }

    /// `isRaided(pos)`.
    pub fn is_raided(&self, pos: BlockPos) -> bool {
        self.get_raids()
            .and_then(|r| r.get_nearby_raid(pos, VALID_RAID_RADIUS_SQR))
            .is_some()
    }

    /// `Raids.get(raidId)`.
    pub fn get_raid(&self, raid_id: i32) -> Option<&Raid> {
        self.get_raids()?.get(raid_id)
    }

    /// Runs `f` with the raid checked out of the level's raids (so `f` may use the level mutably); `None` when there is no
    /// such raid or it is already checked out. The raid's `Raids` meta is refreshed afterwards.
    pub fn with_raid<R>(
        &mut self,
        raid_id: i32,
        f: impl FnOnce(&mut ServerLevel, &mut Raid) -> R,
    ) -> Option<R> {
        let mut raid = self.get_raids_mut().check_out(raid_id)?;
        let result = f(self, &mut raid);
        self.get_raids_mut().check_in(raid);
        Some(result)
    }
}
