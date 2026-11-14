//! `ZombieVillager` (a `Zombie` of kind `ZombieKind::ZombieVillager`): the cure (golden apple on a weakened one, the
//! timer, the conversion back to a villager), the villager a zombie kills turning into one, the villager type of its
//! biome on spawn, and the villager data in the save data.

use super::{Zombie, ZombieGroupData, ZombieKind, base};
use crate::advancements::triggers::CriteriaTriggers;
use crate::define_accessor;
use crate::registry::{RegistryAccess, keys};
use crate::server::level::ServerLevel;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::gossip::GossipContainer;
use crate::world::entity::ai::village::ReputationEventType;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::mob;
use crate::world::entity::npc::villager::villager::Villager;
use crate::world::entity::npc::villager::villager_data::VillagerDataExt as _;
use crate::world::entity::npc::villager::villager_type;
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::synched_data::SynchedEntityDataBuilder;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::enchantment::enchantment_effect_components::EffectComponent;
use crate::world::item::enchantment_helper;
use crate::world::item::trading::{Merchant as _, MerchantOffers};
use crate::world::item::{InteractionHand, InteractionResult, ItemStackExt as _};
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, MobEffect, VillagerProfession, VillagerType};
use mc_nbt::{CompoundTag, Tag};
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
#[derive(Default)]
pub struct ZombieVillagerData {
    /// `villagerConversionTime`.
    pub villager_conversion_time: i32,
    /// `conversionStarter`.
    pub conversion_starter: Option<Uuid>,
    /// `gossips`.
    pub gossips: Option<GossipContainer>,
    /// `tradeOffers`.
    pub trade_offers: Option<MerchantOffers>,
    /// `villagerXp`.
    pub villager_xp: i32,
    /// A conversion read from the save data whose `startConverting` effects wait for the level
    /// ([`read_level_dependent_data`]).
    loaded_conversion: bool,
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
    VillagerData {
        r#type: VillagerType::PLAINS,
        profession: VillagerProfession::NONE,
        level: 1,
    }
}

/// `defineSynchedData` of `ZombieVillager`: the profession of the initial villager data is drawn from the entity's random
/// (`Entity.<init>`, before `LivingEntity` rolls its yaw).
pub fn define_synched_data(b: &mut SynchedEntityDataBuilder, _registries: &RegistryAccess) {
    let villager_data = initialize_zombie_villager_data(
        b.random
            .as_mut()
            .expect("the entity random is set while the data is defined"),
    );
    b.define(DATA_CONVERTING_ID, false);
    b.define(DATA_VILLAGER_DATA, villager_data);
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
    zombie
        .mob
        .living
        .entity
        .entity_data
        .set(DATA_VILLAGER_DATA, villager_data);
}

/// `getVillagerDataFinalized()`.
pub fn get_villager_data_finalized(zombie: &Zombie) -> bool {
    zombie
        .mob
        .living
        .entity
        .entity_data
        .get(DATA_VILLAGER_DATA_FINALIZED)
}

/// `setVillagerDataFinalized(villagerDataFinalized)`.
pub fn set_villager_data_finalized(zombie: &mut Zombie, finalized: bool) {
    zombie
        .mob
        .living
        .entity
        .entity_data
        .set(DATA_VILLAGER_DATA_FINALIZED, finalized);
}

/// `removeWhenFarAway(distSqr)`.
pub fn remove_when_far_away(zombie: &Zombie) -> bool {
    !is_converting(zombie) && data(zombie).villager_xp == 0
}

/// `getVoicePitch()`.
pub fn get_voice_pitch(zombie: &mut Zombie) -> f32 {
    let baby = zombie.is_baby();
    let random = &mut zombie.base_mut().random;
    if baby {
        (random.next_float() - random.next_float()) * 0.2 + 2.0
    } else {
        (random.next_float() - random.next_float()) * 0.2 + 1.0
    }
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
            while yy < y0 + SPECIAL_BLOCK_RADIUS && special_blocks_count < MAX_SPECIAL_BLOCKS_COUNT
            {
                let mut zz = z0 - SPECIAL_BLOCK_RADIUS;
                while zz < z0 + SPECIAL_BLOCK_RADIUS
                    && special_blocks_count < MAX_SPECIAL_BLOCKS_COUNT
                {
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
pub fn mob_interact(
    zombie: &mut Zombie,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    let in_hand = player.get_item_in_hand(hand);
    if in_hand.is(Item::GOLDEN_APPLE) {
        if zombie.has_effect(MobEffect::WEAKNESS) {
            mob::interact::with_hand_stack(player, hand, |stack, player| {
                let infinite = player.has_infinite_materials();
                stack.consume(1, infinite);
            });
            let time =
                zombie.base_mut().random.next_int_bounded(2401) + VILLAGER_CONVERSION_WAIT_MIN;
            start_converting(zombie, level, Some(player.uuid()), time);
            return InteractionResult::SUCCESS_SERVER;
        }
        return InteractionResult::CONSUME;
    }
    mob::interact::mob_interact(zombie, level, player, hand)
}

/// `startConverting(player, time)`.
pub fn start_converting(
    zombie: &mut Zombie,
    level: &mut ServerLevel,
    player: Option<Uuid>,
    time: i32,
) {
    {
        let d = data_mut(zombie);
        d.conversion_starter = player;
        d.villager_conversion_time = time;
    }
    zombie
        .mob
        .living
        .entity
        .entity_data
        .set(DATA_CONVERTING_ID, true);
    apply_conversion_effects(zombie, level, time);
    crate::world::entity::living_entity::util::broadcast_entity_event(level, zombie.id(), 16);
}

/// The effects of `startConverting`: the weakness ends, strength lasts as long as the cure.
fn apply_conversion_effects(zombie: &mut Zombie, level: &mut ServerLevel, time: i32) {
    zombie.remove_effect(level, MobEffect::WEAKNESS);
    let amplifier = (level.difficulty() as i32 - 1).min(0);
    zombie.add_effect(
        level,
        MobEffectInstance::of(MobEffect::STRENGTH, time, amplifier),
        None,
    );
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

/// `VillagerDataHolder.finalizeVillagerType(level, pos)`.
fn finalize_villager_type(zombie: &mut Zombie, level: &mut dyn ServerLevelAccessor, pos: BlockPos) {
    if !get_villager_data_finalized(zombie) {
        let biome = level
            .get_biome_id(pos)
            .and_then(|id| level.registry_access().registry(keys::BIOME)?.name_of(id.0))
            .map(|n| n.to_string());
        let ty = villager_type::by_biome(biome.as_deref());
        set_villager_data(zombie, get_villager_data(zombie).with_type(ty));
        set_villager_data_finalized(zombie, true);
    }
}

/// `finishConversion(level)`.
fn finish_conversion(zombie: &mut Zombie, level: &mut ServerLevel) {
    let params = mob::ConversionParams::single(zombie, level, false, false);
    mob::convert::convert_to_with_source(
        zombie,
        level,
        EntityType::VILLAGER,
        &params,
        EntitySpawnReason::Conversion,
        |from, villager, level| {
            let registries = level.registries.clone();
            let kept = mob::equipment::drop_preserved_equipment_where(from, level, |stack| {
                !enchantment_helper::has(&registries, stack, EffectComponent::prevent_armor_change)
            });
            for slot in kept {
                villager.set_slot(level, slot.get_index() + 300, from.get_item_by_slot(slot));
            }
            let silent = from.base().is_silent();
            let pos = from.block_position();
            let (villager_data, finalized, gossips, offers, xp, starter) = {
                let zombie = from.as_zombie().expect("a zombie villager");
                let d = data(zombie);
                (
                    get_villager_data(zombie),
                    get_villager_data_finalized(zombie),
                    d.gossips.clone(),
                    d.trade_offers.clone(),
                    d.villager_xp,
                    d.conversion_starter,
                )
            };
            {
                let v = villager
                    .as_any_mut()
                    .downcast_mut::<Villager>()
                    .expect("a villager");
                v.set_villager_data_finalized(finalized);
                v.set_villager_data(villager_data);
                if let Some(gossips) = &gossips {
                    v.set_gossips(gossips);
                }
                if let Some(offers) = offers {
                    v.set_offers(Some(offers));
                }
                v.set_villager_xp(xp);
            }
            let difficulty =
                ServerLevelAccessor::get_current_difficulty_at(&*level, villager.block_position());
            villager.finalize_spawn(level, difficulty, EntitySpawnReason::Conversion, None);
            if let Some(v) = villager.as_any_mut().downcast_mut::<Villager>() {
                v.refresh_brain(level);
            }
            if let Some(player) = starter.and_then(|uuid| level.get_player_by_uuid(uuid)) {
                let player_uuid = level.get_entity(player).map(|p| p.uuid());
                trigger_cured_zombie_villager(level, player, from, villager);
                if let Some(uuid) = player_uuid {
                    villager.on_reputation_event_from(
                        level,
                        ReputationEventType::ZOMBIE_VILLAGER_CURED,
                        uuid,
                    );
                }
            }
            villager.add_effect(
                level,
                MobEffectInstance::of(MobEffect::NAUSEA, 200, 0),
                None,
            );
            if !silent {
                level.level_event_impl(None, 1027, pos, 0);
            }
        },
    );
}

/// `Zombie.convertVillagerToZombieVillager(level, villager)`.
pub fn convert_villager_to_zombie_villager(
    zombie: &mut Zombie,
    level: &mut ServerLevel,
    villager: &mut dyn Entity,
) -> bool {
    let silent = zombie.base().is_silent();
    let pos = zombie.block_position();
    let params = mob::ConversionParams::single(villager, level, true, true);
    mob::convert::convert_to_with_source(
        villager,
        level,
        EntityType::ZOMBIE_VILLAGER,
        &params,
        EntitySpawnReason::Conversion,
        |from, converted, level| {
            let (finalized, villager_data, gossips, offers, xp) = {
                let v = from
                    .as_any_mut()
                    .downcast_mut::<Villager>()
                    .expect("a villager");
                (
                    v.get_villager_data_finalized(),
                    v.get_villager_data(),
                    v.get_gossips().copy(),
                    v.get_offers(level).clone(),
                    v.get_villager_xp(),
                )
            };
            let z = converted.as_zombie_mut().expect("a zombie villager");
            set_villager_data_finalized(z, finalized);
            let difficulty =
                ServerLevelAccessor::get_current_difficulty_at(&*level, z.block_position());
            z.finalize_spawn(
                level,
                difficulty,
                EntitySpawnReason::Conversion,
                Some(Box::new(ZombieGroupData::new(false, true))),
            );
            set_villager_data(z, villager_data);
            let d = data_mut(z);
            d.gossips = Some(gossips);
            d.trade_offers = Some(offers);
            d.villager_xp = xp;
            if !silent {
                level.level_event_impl(None, 1026, pos, 0);
            }
        },
    )
    .is_some()
}

/// `CriteriaTriggers.CURED_ZOMBIE_VILLAGER.trigger(player, zombie, villager)`.
fn trigger_cured_zombie_villager(
    level: &mut ServerLevel,
    player: EntityId,
    zombie: &dyn Entity,
    villager: &dyn Entity,
) {
    let matches = {
        let Some(p) = level
            .get_entity(player)
            .and_then(|e| e.as_any().downcast_ref::<ServerPlayer>())
        else {
            return;
        };
        CriteriaTriggers::CURED_ZOMBIE_VILLAGER.matches(&*level, p, zombie, villager)
    };
    level.with_player(player, |level, p| matches.award(level, p));
}

/// ZombieVillager part of `addAdditionalSaveData`.
pub fn add_additional_save_data(zombie: &Zombie, out: &mut CompoundTag) {
    let d = data(zombie);
    out.put(
        "VillagerData",
        Tag::Compound(get_villager_data(zombie).save()),
    );
    out.put_boolean("VillagerDataFinalized", get_villager_data_finalized(zombie));
    if let Some(offers) = &d.trade_offers
        && let Ok(tag) = mc_dfu::encode_nbt(&MerchantOffers::codec(), offers).into_result()
    {
        out.put("Offers", tag);
    }
    if let Some(gossips) = &d.gossips {
        out.put("Gossips", Tag::List(gossips.to_nbt()));
    }
    out.put_int(
        "ConversionTime",
        if is_converting(zombie) {
            d.villager_conversion_time
        } else {
            NOT_CONVERTING
        },
    );
    if let Some(starter) = d.conversion_starter {
        out.put_int_array(
            "ConversionPlayer",
            mc_core::uuid_util::uuid_to_int_array(&starter).to_vec(),
        );
    }
    out.put_int("Xp", d.villager_xp);
}

/// ZombieVillager part of `readAdditionalSaveData` (the effects of `startConverting` need the level:
/// [`read_level_dependent_data`]).
pub fn read_additional_save_data(
    zombie: &mut Zombie,
    _level: Option<&ServerLevel>,
    input: &CompoundTag,
) {
    let villager_data = input.get_compound("VillagerData").map(VillagerData::load);
    if input.get_boolean_or("VillagerDataFinalized", false) || villager_data.is_some() {
        set_villager_data_finalized(zombie, true);
        let villager_data = villager_data
            .unwrap_or_else(|| initialize_zombie_villager_data(&mut zombie.base_mut().random));
        zombie
            .mob
            .living
            .entity
            .entity_data
            .set(DATA_VILLAGER_DATA, villager_data);
    }
    let trade_offers = input.get("Offers").and_then(|t| {
        mc_dfu::parse_nbt(&MerchantOffers::codec(), t)
            .into_result()
            .ok()
    });
    let gossips = input.get_list("Gossips").map(GossipContainer::from_nbt);
    let conversion_time = input.get_int_or("ConversionTime", NOT_CONVERTING);
    let starter = (conversion_time != NOT_CONVERTING)
        .then(|| crate::world::entity::entity::read_uuid(input.get("ConversionPlayer")))
        .flatten();
    let converting = conversion_time != NOT_CONVERTING;
    {
        let d = data_mut(zombie);
        d.trade_offers = trade_offers;
        d.gossips = gossips;
        d.villager_conversion_time = conversion_time;
        d.conversion_starter = starter;
        d.loaded_conversion = converting;
        d.villager_xp = input.get_int_or("Xp", 0);
    }
    zombie
        .mob
        .living
        .entity
        .entity_data
        .set(DATA_CONVERTING_ID, converting);
}

/// The `startConverting` effects of a conversion read from the save data.
pub fn read_level_dependent_data(zombie: &mut Zombie, level: &mut ServerLevel) {
    if std::mem::take(&mut data_mut(zombie).loaded_conversion) {
        apply_conversion_effects(zombie, level, data(zombie).villager_conversion_time);
    }
}
