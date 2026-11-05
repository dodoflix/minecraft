//! `net.minecraft.world.entity.AreaEffectCloud` (server side: radius/duration bookkeeping and the potion application;
//! the particles of `clientTick` are client work).

use super::dimensions::EntityDimensions;
use super::effect::MobEffectInstance;
use super::factory::{ENTITY_FACTORIES, EntityRegistration};
use super::{Entity, EntityData, EntityId, entity};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::effect::{EffectCause, MobEffectExt, potion_contents};
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_data::EntityType;
use mc_data::enums::Pose;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::PotionContents;
use mc_protocol::packets::play::ParticleOptions;
use std::any::Any;
use std::collections::HashMap;
use uuid::Uuid;

const TIME_BETWEEN_APPLICATIONS: i32 = 5;
const MAX_RADIUS: f32 = 32.0;
const MINIMAL_RADIUS: f32 = 0.5;
pub const DEFAULT_WIDTH: f32 = 6.0;
pub const HEIGHT: f32 = 0.5;
pub const INFINITE_DURATION: i32 = -1;
pub const DEFAULT_LINGERING_DURATION: i32 = 600;

define_accessor!(pub DATA_RADIUS: Float = AreaEffectCloud);
define_accessor!(pub DATA_WAITING: Boolean = AreaEffectCloud);
define_accessor!(pub DATA_PARTICLE: Particle = AreaEffectCloud);

/// `DEFAULT_PARTICLE`: `ColorParticleOption.create(ENTITY_EFFECT, -1)`.
fn default_particle() -> ParticleOptions {
    ParticleOptions::Color { particle_type: mc_data::ParticleType::ENTITY_EFFECT, color: -1 }
}

pub struct AreaEffectCloud {
    base: EntityData,
    custom_particle: Option<ParticleOptions>,
    potion_contents: PotionContents,
    potion_duration_scale: f32,
    /// `victims`: entity -> game tick (`tickCount`) at which it can be affected again.
    victims: HashMap<EntityId, i32>,
    duration: i32,
    wait_time: i32,
    reapplication_delay: i32,
    duration_on_use: i32,
    radius_on_use: f32,
    radius_per_tick: f32,
    owner: Option<Uuid>,
}

fn empty_contents() -> PotionContents {
    PotionContents { potion: None, custom_color: None, custom_effects: Vec::new(), custom_name: None }
}

impl AreaEffectCloud {
    /// `AreaEffectCloud(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_RADIUS, 3.0);
            b.define(DATA_WAITING, false);
            b.define(DATA_PARTICLE, default_particle());
        });
        base.no_physics = true;
        Self {
            base,
            custom_particle: None,
            potion_contents: empty_contents(),
            potion_duration_scale: 1.0,
            victims: HashMap::new(),
            duration: -1,
            wait_time: 20,
            reapplication_delay: 20,
            duration_on_use: 0,
            radius_on_use: 0.0,
            radius_per_tick: 0.0,
            owner: None,
        }
    }

    /// `AreaEffectCloud(level, x, y, z)`.
    pub fn at(level: &ServerLevel, x: f64, y: f64, z: f64) -> Self {
        let mut cloud = Self::new(EntityType::AREA_EFFECT_CLOUD, level.next_entity_id());
        cloud.set_pos(x, y, z);
        cloud
    }

    /// `setRadius(radius)`.
    pub fn set_radius(&mut self, radius: f32) {
        if self.base.entity_data.set(DATA_RADIUS, mth::clamp_f32(radius, 0.0, MAX_RADIUS)) {
            self.on_synced_data_updated(DATA_RADIUS.id());
        }
    }

    pub fn get_radius(&self) -> f32 {
        self.base.entity_data.get(DATA_RADIUS)
    }

    pub fn set_potion_contents(&mut self, contents: PotionContents) {
        self.potion_contents = contents;
        self.update_particle();
    }

    pub fn get_potion_contents(&self) -> &PotionContents {
        &self.potion_contents
    }

    pub fn set_custom_particle(&mut self, particle: Option<ParticleOptions>) {
        self.custom_particle = particle;
        self.update_particle();
    }

    pub fn set_potion_duration_scale(&mut self, scale: f32) {
        self.potion_duration_scale = scale;
    }

    // ==== [ImThrowables] additions ====

    /// `applyComponentsFromItemStack(stack)`: `Entity` takes the custom name and data, the cloud the potion contents
    /// and the duration scale.
    pub fn apply_components_from_item_stack(&mut self, stack: &crate::world::item::ItemStack) {
        use mc_protocol::components::{CUSTOM_DATA, CUSTOM_NAME, POTION_CONTENTS, POTION_DURATION_SCALE};
        if let Some(name) = stack.get(&CUSTOM_NAME) {
            self.base.set_custom_name(Some(name.clone()));
        }
        if let Some(data) = stack.get(&CUSTOM_DATA) {
            self.base.custom_data = data.tag().clone();
        }
        if let Some(contents) = stack.get(&POTION_CONTENTS) {
            self.set_potion_contents(contents.clone());
        }
        if let Some(scale) = stack.get(&POTION_DURATION_SCALE) {
            self.potion_duration_scale = *scale;
        }
    }

    /// `updateParticle()`.
    fn update_particle(&mut self) {
        let particle = match &self.custom_particle {
            Some(p) => p.clone(),
            None => {
                let color = potion_contents::get_color(&self.potion_contents) | 0xFF00_0000u32 as i32;
                ParticleOptions::Color { particle_type: mc_data::ParticleType::ENTITY_EFFECT, color }
            }
        };
        self.base.entity_data.set(DATA_PARTICLE, particle);
    }

    /// `addEffect(effect)`: `potionContents.withEffectAdded(effect)`.
    pub fn add_effect(&mut self, effect: &MobEffectInstance) {
        let mut contents = self.potion_contents.clone();
        contents.custom_effects.push(mc_protocol::components::MobEffectInstance {
            effect: effect.effect,
            duration: effect.duration,
            amplifier: effect.amplifier,
            ambient: effect.ambient,
            visible: effect.visible,
            show_icon: effect.show_icon,
            hidden_effect: None,
        });
        self.set_potion_contents(contents);
    }

    pub fn get_particle(&self) -> ParticleOptions {
        self.base.entity_data.get(DATA_PARTICLE)
    }

    fn set_waiting(&mut self, waiting: bool) {
        self.base.entity_data.set(DATA_WAITING, waiting);
    }

    pub fn is_waiting(&self) -> bool {
        self.base.entity_data.get(DATA_WAITING)
    }

    pub fn get_duration(&self) -> i32 {
        self.duration
    }
    pub fn set_duration(&mut self, duration: i32) {
        self.duration = duration;
    }
    pub fn get_radius_on_use(&self) -> f32 {
        self.radius_on_use
    }
    pub fn set_radius_on_use(&mut self, v: f32) {
        self.radius_on_use = v;
    }
    pub fn get_radius_per_tick(&self) -> f32 {
        self.radius_per_tick
    }
    pub fn set_radius_per_tick(&mut self, v: f32) {
        self.radius_per_tick = v;
    }
    pub fn get_duration_on_use(&self) -> i32 {
        self.duration_on_use
    }
    pub fn set_duration_on_use(&mut self, v: i32) {
        self.duration_on_use = v;
    }
    pub fn get_wait_time(&self) -> i32 {
        self.wait_time
    }
    pub fn set_wait_time(&mut self, v: i32) {
        self.wait_time = v;
    }
    pub fn set_owner(&mut self, owner: Option<&dyn Entity>) {
        self.owner = owner.map(|o| o.uuid());
    }
    pub fn get_owner<'l>(&self, level: &'l ServerLevel) -> Option<&'l dyn Entity> {
        level.get_entity_by_uuid(self.owner?).filter(|e| e.living().is_some())
    }

    /// `serverTick(serverLevel)`.
    fn server_tick(&mut self, level: &mut ServerLevel) {
        let tick_count = self.base.tick_count;
        if self.duration != -1 && tick_count - self.wait_time >= self.duration {
            self.discard();
            return;
        }
        let is_waiting = self.is_waiting();
        let should_wait = tick_count < self.wait_time;
        if is_waiting != should_wait {
            self.set_waiting(should_wait);
        }
        if should_wait {
            return;
        }
        let mut radius = self.get_radius();
        if self.radius_per_tick != 0.0 {
            radius += self.radius_per_tick;
            if radius < MINIMAL_RADIUS {
                self.discard();
                return;
            }
            self.set_radius(radius);
        }
        if tick_count % TIME_BETWEEN_APPLICATIONS != 0 {
            return;
        }
        self.victims.retain(|_, until| tick_count < *until);
        if !potion_contents::has_effects(&self.potion_contents) {
            self.victims.clear();
            return;
        }
        let mut all_effects = Vec::new();
        potion_contents::for_each_effect(&self.potion_contents, self.potion_duration_scale, |e| all_effects.push(e));
        let entities = level.get_entities_of_class(&self.base.bounding_box(), |e| e.living().is_some(), |_| true);
        let owner_id = self.get_owner(level).map(|o| o.id());
        for id in entities {
            if self.victims.contains_key(&id) {
                continue;
            }
            let Some(entity) = level.get_entity(id) else { continue };
            if !entity.is_affected_by_potions() || !all_effects.iter().any(|e| entity.can_be_affected(level, e)) {
                continue;
            }
            let xd = entity.base().x() - self.base.x();
            let zd = entity.base().z() - self.base.z();
            let dist = xd * xd + zd * zd;
            if dist <= f64::from(radius) * f64::from(radius) {
                self.victims.insert(id, tick_count + self.reapplication_delay);
                self.apply_effects(level, id, owner_id, &all_effects);
                if self.radius_on_use != 0.0 {
                    radius += self.radius_on_use;
                    if radius < MINIMAL_RADIUS {
                        self.discard();
                        return;
                    }
                    self.set_radius(radius);
                }
                if self.duration_on_use != 0 && self.duration != -1 {
                    self.duration += self.duration_on_use;
                    if self.duration <= 0 {
                        self.discard();
                        return;
                    }
                }
            }
        }
    }

    /// The effect loop for one victim. The owner (a live entity of the level) is lent out of the level storage for the
    /// call so it can be passed by reference next to `&mut level`; when the victim is the owner it is its own cause.
    fn apply_effects(&self, level: &mut ServerLevel, victim: EntityId, owner_id: Option<EntityId>, effects: &[MobEffectInstance]) {
        let owner_is_victim = owner_id == Some(victim);
        let lent_owner = owner_id.filter(|_| !owner_is_victim).and_then(|o| level.take_entity(o).map(|e| (o, e)));
        level.with_entity(victim, |level, entity| {
            let owner_cause = match (&lent_owner, owner_is_victim) {
                (Some((_, owner)), _) => Some(EffectCause::Entity(owner.as_ref())),
                (None, true) => Some(EffectCause::Mob),
                (None, false) => None,
            };
            for effect in effects {
                if effect.effect.is_instantaneous() {
                    effect.effect.apply_instantaneous_effect(level, Some(EffectCause::Entity(self)), owner_cause, entity, effect.amplifier, 0.5);
                } else {
                    entity.add_effect(level, effect.clone(), Some(self.base.id));
                }
            }
        });
        if let Some((id, owner)) = lent_owner {
            level.put_back_entity(id, owner);
        }
    }
}

impl Entity for AreaEffectCloud {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.base_tick(level);
        self.server_tick(level);
    }

    fn refresh_dimensions(&mut self) {
        let p = self.base.position();
        let old_dim = self.base.dimensions.clone();
        let new_dim = self.get_dimensions(Pose::Standing);
        self.base.eye_height = new_dim.eye_height;
        self.base.dimensions = new_dim;
        let _ = old_dim;
        self.reapply_position();
        self.set_pos(p.x, p.y, p.z);
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_RADIUS.id() {
            self.refresh_dimensions();
        }
        if accessor_id == super::entity::DATA_POSE.id() {
            self.refresh_dimensions();
        }
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        EntityDimensions::scalable(self.get_radius() * 2.0, HEIGHT)
    }

    fn hurt_server(&mut self, _level: &mut ServerLevel, _source: &crate::world::damagesource::DamageSource, _amount: f32) -> bool {
        false
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        out.put_int("Age", self.base.tick_count);
        out.put_int("Duration", self.duration);
        out.put_int("WaitTime", self.wait_time);
        out.put_int("ReapplicationDelay", self.reapplication_delay);
        out.put_int("DurationOnUse", self.duration_on_use);
        out.put_float("RadiusOnUse", self.radius_on_use);
        out.put_float("RadiusPerTick", self.radius_per_tick);
        out.put_float("Radius", self.get_radius());
        if let Some(tag) = self.custom_particle.as_ref().and_then(particle_to_nbt) {
            out.put("custom_particle", tag);
        }
        if let Some(owner) = self.owner {
            out.put_int_array("Owner", mc_core::uuid_util::uuid_to_int_array(&owner).to_vec());
        }
        if self.potion_contents != empty_contents()
            && let Ok(tag) = mc_dfu::encode_nbt(&PotionContents::codec(), &self.potion_contents).into_result()
        {
            out.put("potion_contents", tag);
        }
        if self.potion_duration_scale != 1.0 {
            out.put_float("potion_duration_scale", self.potion_duration_scale);
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.base.tick_count = input.get_int_or("Age", 0);
        self.duration = input.get_int_or("Duration", -1);
        self.wait_time = input.get_int_or("WaitTime", 20);
        self.reapplication_delay = input.get_int_or("ReapplicationDelay", 20);
        self.duration_on_use = input.get_int_or("DurationOnUse", 0);
        self.radius_on_use = input.get_float_or("RadiusOnUse", 0.0);
        self.radius_per_tick = input.get_float_or("RadiusPerTick", 0.0);
        self.set_radius(input.get_float_or("Radius", 3.0));
        self.owner = entity::read_uuid(input.get("Owner"));
        self.set_custom_particle(input.get("custom_particle").and_then(particle_from_nbt));
        let contents = input
            .get("potion_contents")
            .and_then(|t| mc_dfu::parse_nbt(&PotionContents::codec(), t).into_result().ok())
            .unwrap_or_else(empty_contents);
        self.set_potion_contents(contents);
        self.potion_duration_scale = input.get_float_or("potion_duration_scale", 1.0);
    }
}

/// `ParticleTypes.CODEC` encode for the option kinds without item/vector payload (`{type, ...fields}`); other kinds
/// (dust, trails, vibrations, ...) are not saved.
fn particle_to_nbt(particle: &ParticleOptions) -> Option<Tag> {
    let mut tag = CompoundTag::new();
    match particle {
        ParticleOptions::Simple(t) => tag.put_string("type", t.name()),
        ParticleOptions::Color { particle_type, color } => {
            tag.put_string("type", particle_type.name());
            tag.put_int("color", *color);
        }
        ParticleOptions::Power { particle_type, power } => {
            tag.put_string("type", particle_type.name());
            tag.put_float("power", *power);
        }
        ParticleOptions::Block { particle_type, state } => {
            tag.put_string("type", particle_type.name());
            tag.put("block_state", Tag::Compound(crate::world::level::levelgen::structure::templatesystem::structure_template::write_block_state(*state)));
        }
        _ => {
            log::warn!("AreaEffectCloud: custom particle kind not saved");
            return None;
        }
    }
    Some(Tag::Compound(tag))
}

fn particle_from_nbt(tag: &Tag) -> Option<ParticleOptions> {
    let tag = tag.as_compound()?;
    let ty = mc_data::ParticleType::by_name(tag.get_string("type")?)?;
    if let Some(color) = tag.get("color").and_then(|t| t.as_number()) {
        return Some(ParticleOptions::Color { particle_type: ty, color: color.int_value() });
    }
    if ty == mc_data::ParticleType::DRAGON_BREATH {
        return Some(ParticleOptions::Power { particle_type: ty, power: tag.get_float_or("power", 1.0) });
    }
    if let Some(state) = tag.get_compound("block_state") {
        let state = crate::world::level::levelgen::structure::templatesystem::structure_template::read_block_state(state);
        return Some(ParticleOptions::Block { particle_type: ty, state });
    }
    Some(ParticleOptions::Simple(ty))
}

#[distributed_slice(ENTITY_FACTORIES)]
static AREA_EFFECT_CLOUD_FACTORY: EntityRegistration =
    EntityRegistration { entity_type: EntityType::AREA_EFFECT_CLOUD, create: |ty, id, _registries| Box::new(AreaEffectCloud::new(ty, id)) };
