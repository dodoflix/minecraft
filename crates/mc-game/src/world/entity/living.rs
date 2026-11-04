//! `LivingEntity` data and its save/load (`addAdditionalSaveData` / `readAdditionalSaveData`).

use super::attributes::AttributeMap;
use super::effect::MobEffectInstance;
use super::entity::{DATA_POSE, EntityData, FLAG_FALL_FLYING};
use super::synched_data::SynchedEntityDataBuilder;
use crate::define_accessor;
use indexmap::IndexMap;
use mc_core::math::mth;
use mc_core::random::RandomSource;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::enums::Pose;
use mc_data::{Attribute, EntityType, MobEffect};
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::packets::play::ParticleOptions;

pub const LIVING_ENTITY_FLAG_IS_USING: i32 = 1;
pub const LIVING_ENTITY_FLAG_OFF_HAND: i32 = 2;
pub const LIVING_ENTITY_FLAG_SPIN_ATTACK: i32 = 4;
pub const DEATH_DURATION: i32 = 20;
pub const DEFAULT_BASE_GRAVITY: f64 = 0.08;

define_accessor!(pub DATA_LIVING_ENTITY_FLAGS: Byte = LivingEntity);
define_accessor!(pub DATA_HEALTH_ID: Float = LivingEntity);
define_accessor!(pub DATA_EFFECT_PARTICLES: Particles = LivingEntity);
define_accessor!(pub DATA_EFFECT_AMBIENCE_ID: Boolean = LivingEntity);
define_accessor!(pub DATA_ARROW_COUNT_ID: Int = LivingEntity);
define_accessor!(pub DATA_STINGER_COUNT_ID: Int = LivingEntity);
define_accessor!(pub SLEEPING_POS_ID: OptionalBlockPos = LivingEntity);

/// Fields of vanilla `LivingEntity` (the part that is state, not logic).
pub struct LivingEntityData {
    pub entity: EntityData,
    pub attributes: AttributeMap,
    pub active_effects: IndexMap<MobEffect, MobEffectInstance>,
    pub effects_dirty: bool,
    pub remove_arrow_time: i32,
    pub remove_stinger_time: i32,
    pub hurt_time: i32,
    pub hurt_duration: i32,
    pub death_time: i32,
    pub damage_cooldown_time: i32,
    pub attack_strength_ticker: i32,
    pub item_swap_ticker: i32,
    pub y_body_rot: f32,
    pub y_body_rot_o: f32,
    pub y_head_rot: f32,
    pub y_head_rot_o: f32,
    pub last_hurt_by_player_memory_time: i32,
    pub dead: bool,
    pub no_action_time: i32,
    pub last_hurt: f32,
    pub jumping: bool,
    pub xxa: f32,
    pub yya: f32,
    pub zza: f32,
    pub lerp_y_head_rot: f64,
    pub lerp_head_steps: i32,
    pub speed: f32,
    pub no_jump_delay: i32,
    pub absorption_amount: f32,
    pub use_item_remaining: i32,
    pub fall_fly_ticks: i32,
    pub auto_spin_attack_ticks: i32,
    pub auto_spin_attack_dmg: f32,
    pub swim_amount: f32,
    pub swim_amount_o: f32,
    pub last_pos: Option<BlockPos>,
    pub current_impulse_context_reset_grace_time: i32,
    /// `currentImpulseImpactPos` (saved as `current_explosion_impact_pos`).
    pub current_impulse_impact_pos: Option<Vec3>,
    pub last_hurt_by_mob_timestamp: i32,
    pub last_hurt_mob_timestamp: i32,
    /// `lastHurtByPlayer` / `lastHurtByMob` references (UUIDs, `EntityReference`).
    pub last_hurt_by_player: Option<uuid::Uuid>,
    pub last_hurt_by_mob: Option<uuid::Uuid>,
}

impl LivingEntityData {
    /// `LivingEntity(type, level)`. `define_synched_data` adds the subclasses' accessors.
    pub fn new(entity_type: EntityType, id: super::EntityId, max_air_supply: i32, define_synched_data: impl FnOnce(&mut SynchedEntityDataBuilder)) -> Self {
        let mut entity = EntityData::new(entity_type, id, max_air_supply, |b| {
            b.define(DATA_LIVING_ENTITY_FLAGS, 0);
            b.define(DATA_EFFECT_PARTICLES, Vec::<ParticleOptions>::new());
            b.define(DATA_EFFECT_AMBIENCE_ID, false);
            b.define(DATA_ARROW_COUNT_ID, 0);
            b.define(DATA_STINGER_COUNT_ID, 0);
            b.define(DATA_HEALTH_ID, 1.0);
            b.define(SLEEPING_POS_ID, None);
            define_synched_data(b);
        });
        entity.blocks_building = true;
        let attributes = AttributeMap::new(entity_type);
        let mut data = Self {
            entity,
            attributes,
            active_effects: IndexMap::new(),
            effects_dirty: true,
            remove_arrow_time: 0,
            remove_stinger_time: 0,
            hurt_time: 0,
            hurt_duration: 0,
            death_time: 0,
            damage_cooldown_time: 0,
            attack_strength_ticker: 0,
            item_swap_ticker: 0,
            y_body_rot: 0.0,
            y_body_rot_o: 0.0,
            y_head_rot: 0.0,
            y_head_rot_o: 0.0,
            last_hurt_by_player_memory_time: 0,
            dead: false,
            no_action_time: 0,
            last_hurt: 0.0,
            jumping: false,
            xxa: 0.0,
            yya: 0.0,
            zza: 0.0,
            lerp_y_head_rot: 0.0,
            lerp_head_steps: 0,
            speed: 0.0,
            no_jump_delay: 0,
            absorption_amount: 0.0,
            use_item_remaining: 0,
            fall_fly_ticks: 0,
            auto_spin_attack_ticks: 0,
            auto_spin_attack_dmg: 0.0,
            swim_amount: 0.0,
            swim_amount_o: 0.0,
            last_pos: None,
            current_impulse_context_reset_grace_time: 0,
            current_impulse_impact_pos: None,
            last_hurt_by_mob_timestamp: 0,
            last_hurt_mob_timestamp: 0,
            last_hurt_by_player: None,
            last_hurt_by_mob: None,
        };
        let max_health = data.get_max_health();
        data.set_health(max_health);
        data.entity.set_pos_raw_and_box(0.0, 0.0, 0.0); // reapplyPosition()
        let y_rot = data.entity.random.next_float() * (std::f64::consts::PI * 2.0) as f32;
        data.entity.set_y_rot(y_rot);
        data.y_head_rot = data.entity.y_rot();
        data
    }

    pub fn get_attribute_value(&mut self, attribute: Attribute) -> f64 {
        self.attributes.get_value(attribute)
    }

    /// `getMaxHealth()`.
    pub fn get_max_health(&mut self) -> f32 {
        self.attributes.get_value(Attribute::MAX_HEALTH) as f32
    }

    /// `getHealth()`.
    pub fn get_health(&self) -> f32 {
        self.entity.entity_data.get(DATA_HEALTH_ID)
    }

    /// `setHealth(health)`: clamped to `0..=maxHealth`.
    pub fn set_health(&mut self, health: f32) {
        let max = self.get_max_health();
        self.entity.entity_data.set(DATA_HEALTH_ID, mth::clamp_f32(health, 0.0, max));
    }

    /// `isDeadOrDying()`.
    pub fn is_dead_or_dying(&self) -> bool {
        self.get_health() <= 0.0
    }

    pub fn get_living_entity_flag(&self, flag: i32) -> bool {
        (self.entity.entity_data.get(DATA_LIVING_ENTITY_FLAGS) as i32 & flag) != 0
    }

    pub fn set_living_entity_flag(&mut self, flag: i32, value: bool) {
        let cur = self.entity.entity_data.get(DATA_LIVING_ENTITY_FLAGS) as i32;
        let new = if value { cur | flag } else { cur & !flag };
        self.entity.entity_data.set(DATA_LIVING_ENTITY_FLAGS, new as i8);
    }

    /// `isFallFlying()`.
    pub fn is_fall_flying(&self) -> bool {
        self.entity.get_shared_flag(FLAG_FALL_FLYING)
    }

    pub fn get_sleeping_pos(&self) -> Option<BlockPos> {
        self.entity.entity_data.get(SLEEPING_POS_ID)
    }

    pub fn set_sleeping_pos(&mut self, pos: BlockPos) {
        self.entity.entity_data.set(SLEEPING_POS_ID, Some(pos));
    }

    pub fn clear_sleeping_pos(&mut self) {
        self.entity.entity_data.set(SLEEPING_POS_ID, None);
    }

    pub fn is_sleeping(&self) -> bool {
        self.get_sleeping_pos().is_some()
    }
}

fn put_vec3(out: &mut CompoundTag, key: &str, v: Vec3) {
    let mut list = ListTag::new();
    list.push(Tag::double(v.x));
    list.push(Tag::double(v.y));
    list.push(Tag::double(v.z));
    out.put(key, list);
}

/// `LivingEntity.addAdditionalSaveData` (the `Brain` and `equipment` keys are written by the callers that own
/// those parts: mobs / the equipment holder; `locator_bar_icon` is unmodelled).
pub fn add_additional_save_data(living: &LivingEntityData, absorption_amount: f32, out: &mut CompoundTag) {
    out.put_float("Health", living.get_health());
    out.put_short("HurtTime", living.hurt_time as i16);
    out.put_short("DeathTime", living.death_time as i16);
    out.put_float("AbsorptionAmount", absorption_amount);
    out.put_int("current_impulse_context_reset_grace_time", living.current_impulse_context_reset_grace_time);
    if let Some(pos) = living.current_impulse_impact_pos {
        put_vec3(out, "current_explosion_impact_pos", pos);
    }
    out.put("attributes", living.attributes.pack());
    if !living.active_effects.is_empty() {
        let mut list = ListTag::new();
        for e in living.active_effects.values() {
            list.push(Tag::Compound(e.to_nbt()));
        }
        out.put("active_effects", list);
    }
    out.put_boolean("FallFlying", living.is_fall_flying());
    if let Some(pos) = living.get_sleeping_pos() {
        out.put_int_array("sleeping_pos", vec![pos.x(), pos.y(), pos.z()]);
    }
    if let Some(uuid) = living.last_hurt_by_player {
        out.put_int_array("last_hurt_by_player", mc_core::uuid_util::uuid_to_int_array(&uuid).to_vec());
        out.put_int("last_hurt_by_player_memory_time", living.last_hurt_by_player_memory_time);
    }
    if let Some(uuid) = living.last_hurt_by_mob {
        out.put_int_array("last_hurt_by_mob", mc_core::uuid_util::uuid_to_int_array(&uuid).to_vec());
        out.put_int("ticks_since_last_hurt_by_mob", living.entity.tick_count.wrapping_sub(living.last_hurt_by_mob_timestamp));
    }
}

/// `BlockPos.CODEC` (int array of 3).
pub fn read_block_pos(tag: Option<&Tag>) -> Option<BlockPos> {
    match tag? {
        Tag::IntArray(v) if v.len() == 3 => Some(BlockPos::new(v[0], v[1], v[2])),
        Tag::List(l) if l.len() == 3 => {
            let n = |i: usize| l.get(i).and_then(|t| t.as_number()).map(|n| n.int_value());
            Some(BlockPos::new(n(0)?, n(1)?, n(2)?))
        }
        _ => None,
    }
}

/// `LivingEntity.readAdditionalSaveData` (state part; `setPosToBed` / `stopSleeping` follow-ups need the level
/// and run when the entity first ticks, see `LivingEntity.baseTick` on `firstTick`).
pub fn read_additional_save_data(living: &mut LivingEntityData, input: &CompoundTag) -> f32 {
    let absorption = input.get_float_or("AbsorptionAmount", 0.0);
    if let Some(list) = input.get_list("attributes") {
        living.attributes.apply(list);
    }
    living.active_effects.clear();
    if let Some(list) = input.get_list("active_effects") {
        for tag in list.iter() {
            if let Some(effect) = tag.as_compound().and_then(MobEffectInstance::from_nbt) {
                living.active_effects.insert(effect.effect, effect);
                living.effects_dirty = true;
            }
        }
    }
    let max_health = living.get_max_health();
    let health = input.get_float_or("Health", max_health);
    living.set_health(health);
    living.hurt_time = i32::from(input.get_short_or("HurtTime", 0));
    living.death_time = i32::from(input.get_short_or("DeathTime", 0));
    living.entity.set_shared_flag(FLAG_FALL_FLYING, input.get_boolean_or("FallFlying", false));
    match read_block_pos(input.get("sleeping_pos")) {
        Some(pos) => {
            living.set_sleeping_pos(pos);
            living.entity.entity_data.set(DATA_POSE, Pose::Sleeping);
        }
        None => living.clear_sleeping_pos(),
    }
    living.last_hurt_by_player = super::entity::read_uuid(input.get("last_hurt_by_player"));
    living.last_hurt_by_player_memory_time = input.get_int_or("last_hurt_by_player_memory_time", 0);
    living.last_hurt_by_mob = super::entity::read_uuid(input.get("last_hurt_by_mob"));
    living.last_hurt_by_mob_timestamp = living.entity.tick_count.wrapping_sub(input.get_int_or("ticks_since_last_hurt_by_mob", 0));
    living.current_impulse_context_reset_grace_time = input.get_int_or("current_impulse_context_reset_grace_time", 0);
    living.current_impulse_impact_pos = super::entity::read_vec3(input.get("current_explosion_impact_pos"));
    absorption
}
