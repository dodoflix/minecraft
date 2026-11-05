//! `ZombieVillager` (a [`Zombie`](super::Zombie) of kind [`ZombieKind::ZombieVillager`](super::ZombieKind::ZombieVillager)):
//! cured with a golden apple + weakness, converts back to a villager.

use super::{Zombie, ZombieKind, base};
use crate::define_accessor;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::mob;
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::synched_data::SynchedEntityDataBuilder;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStackExt as _};
use crate::world::item::trading::MerchantOffers;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{Item, MobEffect, VillagerProfession, VillagerType};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::VillagerData;
use uuid::Uuid;

define_accessor!(pub DATA_CONVERTING_ID: Boolean = ZombieVillager);
define_accessor!(pub DATA_VILLAGER_DATA: VillagerData = ZombieVillager);
define_accessor!(pub DATA_VILLAGER_DATA_FINALIZED: Boolean = ZombieVillager);

pub const VILLAGER_CONVERSION_WAIT_MIN: i32 = 3600;
pub const VILLAGER_CONVERSION_WAIT_MAX: i32 = 6000;
pub const MAX_SPECIAL_BLOCKS_COUNT: i32 = 14;
pub const SPECIAL_BLOCK_RADIUS: i32 = 4;
const NOT_CONVERTING: i32 = -1;

/// The fields `ZombieVillager` adds to `Zombie`.
pub struct ZombieVillagerData {
    /// `villagerConversionTime`.
    pub villager_conversion_time: i32,
    /// `conversionStarter`.
    pub conversion_starter: Option<Uuid>,
    /// `tradeOffers`.
    pub trade_offers: Option<MerchantOffers>,
    /// `villagerXp`.
    pub villager_xp: i32,
}

impl Default for ZombieVillagerData {
    fn default() -> Self {
        Self { villager_conversion_time: 0, conversion_starter: None, trade_offers: None, villager_xp: 0 }
    }
}

fn data(zombie: &Zombie) -> &ZombieVillagerData {
    match &zombie.kind {
        ZombieKind::ZombieVillager(d) => d,
        _ => unreachable!("not a zombie villager"),
    }
}

fn data_mut(zombie: &mut Zombie) -> &mut ZombieVillagerData {
    match &mut zombie.kind {
        ZombieKind::ZombieVillager(d) => d,
        _ => unreachable!("not a zombie villager"),
    }
}

/// `Villager.createDefaultVillagerData()`.
pub fn create_default_villager_data() -> VillagerData {
    VillagerData { r#type: VillagerType::PLAINS, profession: VillagerProfession::NONE, level: 1 }
}

/// `defineSynchedData` of `ZombieVillager` (the profession draw of `initializeZombieVillagerData` needs the entity's random:
/// [`init`] redefines the value right after construction).
pub fn define_synched_data(b: &mut SynchedEntityDataBuilder, _registries: &RegistryAccess) {
    b.define(DATA_CONVERTING_ID, false);
    b.define(DATA_VILLAGER_DATA, create_default_villager_data());
    b.define(DATA_VILLAGER_DATA_FINALIZED, false);
}

/// `initializeZombieVillagerData(random)`: the default data with a random profession of the registry.
fn initialize_zombie_villager_data(random: &mut dyn RandomSource) -> VillagerData {
    let mut villager_data = create_default_villager_data();
    let count = VillagerProfession::iter().len() as i32;
    if count > 0 {
        let index = random.next_int_bounded(count) as u32;
        if let Some(profession) = VillagerProfession::from_id(index) {
            villager_data.profession = profession;
        }
    }
    villager_data
}

/// The body of the `ZombieVillager` constructor (its `defineSynchedData` ran inside `Entity.<init>`).
pub fn init(zombie: &mut Zombie, _registries: &RegistryAccess) {
    let villager_data = initialize_zombie_villager_data(&mut zombie.mob.living.entity.random);
    zombie.mob.living.entity.entity_data.redefine_initial(DATA_VILLAGER_DATA, villager_data);
}

/// `isConverting()`.
pub fn is_converting(zombie: &Zombie) -> bool {
    zombie.mob.living.entity.entity_data.get(DATA_CONVERTING_ID)
}

/// `getVillagerData()`.
pub fn get_villager_data(zombie: &Zombie) -> VillagerData {
    zombie.mob.living.entity.entity_data.get(DATA_VILLAGER_DATA)
}

/// `setVillagerData(villagerData)`.
pub fn set_villager_data(zombie: &mut Zombie, villager_data: VillagerData) {
    let current = get_villager_data(zombie);
    if current.profession != villager_data.profession {
        data_mut(zombie).trade_offers = None;
    }
    zombie.mob.living.entity.entity_data.set(DATA_VILLAGER_DATA, villager_data);
}

/// `getVillagerDataFinalized()`.
pub fn get_villager_data_finalized(zombie: &Zombie) -> bool {
    zombie.mob.living.entity.entity_data.get(DATA_VILLAGER_DATA_FINALIZED)
}

/// `setVillagerDataFinalized(villagerDataFinalized)`.
pub fn set_villager_data_finalized(zombie: &mut Zombie, finalized: bool) {
    zombie.mob.living.entity.entity_data.set(DATA_VILLAGER_DATA_FINALIZED, finalized);
}

/// `removeWhenFarAway(distSqr)`.
pub fn remove_when_far_away(zombie: &Zombie) -> bool {
    !is_converting(zombie) && data(zombie).villager_xp == 0
}

/// `getVoicePitch()`.
pub fn get_voice_pitch(zombie: &mut Zombie) -> f32 {
    let baby = zombie.is_baby();
    let random = &mut zombie.base_mut().random;
    if baby { (random.next_float() - random.next_float()) * 0.2 + 2.0 } else { (random.next_float() - random.next_float()) * 0.2 + 1.0 }
}

/// `tick()` before `super.tick()`: the conversion countdown.
pub fn pre_tick(zombie: &mut Zombie, level: &mut ServerLevel) {
    if zombie.is_alive() && is_converting(zombie) {
        let amount = get_conversion_progress(zombie, level);
        data_mut(zombie).villager_conversion_time -= amount;
        if data(zombie).villager_conversion_time <= 0 {
            finish_conversion(zombie, level);
        }
    }
}

/// `getConversionProgress()`.
fn get_conversion_progress(zombie: &mut Zombie, level: &ServerLevel) -> i32 {
    use crate::world::level::BlockGetter;
    use crate::world::level::block::state::BlockStateExt;
    let mut amount = 1;
    if zombie.base_mut().random.next_float() < 0.01 {
        let mut special_blocks_count = 0;
        let x0 = zombie.base().x() as i32;
        let y0 = zombie.base().y() as i32;
        let z0 = zombie.base().z() as i32;
        let mut xx = x0 - SPECIAL_BLOCK_RADIUS;
        while xx < x0 + SPECIAL_BLOCK_RADIUS && special_blocks_count < MAX_SPECIAL_BLOCKS_COUNT {
            let mut yy = y0 - SPECIAL_BLOCK_RADIUS;
            while yy < y0 + SPECIAL_BLOCK_RADIUS && special_blocks_count < MAX_SPECIAL_BLOCKS_COUNT {
                let mut zz = z0 - SPECIAL_BLOCK_RADIUS;
                while zz < z0 + SPECIAL_BLOCK_RADIUS && special_blocks_count < MAX_SPECIAL_BLOCKS_COUNT {
                    let state = level.get_block_state(BlockPos::new(xx, yy, zz));
                    if state.is_tag("speeds_up_zombie_villager_curing") {
                        if zombie.base_mut().random.next_float() < 0.3 {
                            amount += 1;
                        }
                        special_blocks_count += 1;
                    }
                    zz += 1;
                }
                yy += 1;
            }
            xx += 1;
        }
    }
    amount
}

/// `mobInteract(player, hand)`.
pub fn mob_interact(zombie: &mut Zombie, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
    let in_hand = player.get_item_in_hand(hand);
    if in_hand.is(Item::GOLDEN_APPLE) {
        if zombie.has_effect(MobEffect::WEAKNESS) {
            mob::interact::with_hand_stack(player, hand, |stack, player| {
                let infinite = player.has_infinite_materials();
                stack.consume(1, infinite);
            });
            let time = zombie.base_mut().random.next_int_bounded(2401) + VILLAGER_CONVERSION_WAIT_MIN;
            start_converting(zombie, level, Some(player.uuid()), time);
            return InteractionResult::SUCCESS_SERVER;
        }
        return InteractionResult::CONSUME;
    }
    mob::interact::mob_interact(zombie, level, player, hand)
}

/// `startConverting(player, time)`.
pub fn start_converting(zombie: &mut Zombie, level: &mut ServerLevel, player: Option<Uuid>, time: i32) {
    {
        let d = data_mut(zombie);
        d.conversion_starter = player;
        d.villager_conversion_time = time;
    }
    zombie.mob.living.entity.entity_data.set(DATA_CONVERTING_ID, true);
    zombie.remove_effect(level, MobEffect::WEAKNESS);
    let amplifier = (level.difficulty() as i32 - 1).min(0);
    zombie.add_effect(level, MobEffectInstance::of(MobEffect::STRENGTH, time, amplifier), None);
    crate::world::entity::living_entity::util::broadcast_entity_event(level, zombie.id(), 16);
}

/// `finalizeSpawn(level, difficulty, spawnReason, groupData)`.
pub fn finalize_spawn(
    zombie: &mut Zombie,
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
    reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    finalize_villager_type(zombie, level, zombie.block_position());
    base::finalize_spawn(zombie, level, difficulty, reason, group_data)
}

// ==== PENDING(Villagers): the functions below need the Villager class (MobsScope.Villagers) ====

/// `VillagerDataHolder.finalizeVillagerType(level, pos)`.
fn finalize_villager_type(_zombie: &mut Zombie, _level: &mut dyn ServerLevelAccessor, _pos: BlockPos) {}

/// `finishConversion(level)`.
fn finish_conversion(_zombie: &mut Zombie, _level: &mut ServerLevel) {}

/// `Zombie.convertVillagerToZombieVillager(level, villager)`.
pub fn convert_villager_to_zombie_villager(_zombie: &mut Zombie, _level: &mut ServerLevel, _villager: &mut dyn Entity) -> bool {
    false
}

/// ZombieVillager part of `addAdditionalSaveData`.
pub fn add_additional_save_data(_zombie: &Zombie, _out: &mut CompoundTag) {}

/// ZombieVillager part of `readAdditionalSaveData`.
pub fn read_additional_save_data(_zombie: &mut Zombie, _input: &CompoundTag) {}
