//! `AbstractThrownPotion` with its two concrete classes `ThrownSplashPotion` and `ThrownLingeringPotion`
//! (one struct, `kind` selects the class: entity type, default item and `onHitAsPotion`).

use super::super::throwable::{self, DATA_ITEM_STACK};
use super::super::util::compute_margin;
use super::super::{EntityHitResult, HitResult, Projectile, ProjectileData, on_hit_base};
use crate::server::level::ServerLevel;
use crate::world::effect::{EffectCause, MobEffectExt, potion_contents};
use crate::world::entity::area_effect_cloud::AreaEffectCloud;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::entity::TOTAL_AIR_SUPPLY;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::{Entity, EntityData, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::block::impls::{abstract_candle_block, campfire_block};
use crate::world::level::{BlockGetter, LevelAccessor};
use crate::{impl_throwable_entity, impl_throwable_projectile};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::phys::{AABB, BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{EntityType, Item, Potion, props};
use mc_nbt::CompoundTag;
use mc_protocol::components::{POTION_CONTENTS, POTION_DURATION_SCALE, PotionContents};

/// `AbstractThrownPotion.SPLASH_RANGE_SQ`.
const SPLASH_RANGE_SQ: f64 = 16.0;

/// Which concrete class of `AbstractThrownPotion` an entity is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PotionThrowKind {
    Splash,
    Lingering,
}

impl PotionThrowKind {
    fn entity_type(self) -> EntityType {
        match self {
            Self::Splash => EntityType::SPLASH_POTION,
            Self::Lingering => EntityType::LINGERING_POTION,
        }
    }

    /// `getDefaultItem()`.
    fn default_item(self) -> Item {
        match self {
            Self::Splash => Item::SPLASH_POTION,
            Self::Lingering => Item::LINGERING_POTION,
        }
    }
}

pub struct AbstractThrownPotion {
    base: EntityData,
    projectile: ProjectileData,
    kind: PotionThrowKind,
}

impl AbstractThrownPotion {
    /// `ThrownSplashPotion(type, level)` / `ThrownLingeringPotion(type, level)`.
    pub fn new(kind: PotionThrowKind, id: EntityId) -> Self {
        let base = EntityData::new(kind.entity_type(), id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM_STACK, ItemStack::new(kind.default_item()));
        });
        Self { base, projectile: ProjectileData::default(), kind }
    }

    /// `Thrown*Potion(level, owner, itemStack)`.
    pub fn with_owner(kind: PotionThrowKind, level: &ServerLevel, owner: &dyn Entity, item: &ItemStack) -> Self {
        let mut s = Self::new(kind, level.next_entity_id());
        throwable::position_at_owner(&mut s, owner);
        throwable::set_item(&mut s.base, item);
        s.set_owner(Some(owner));
        s
    }

    /// `Thrown*Potion(level, x, y, z, itemStack)`.
    pub fn at(kind: PotionThrowKind, level: &ServerLevel, x: f64, y: f64, z: f64, item: &ItemStack) -> Self {
        let mut s = Self::new(kind, level.next_entity_id());
        s.set_pos(x, y, z);
        throwable::set_item(&mut s.base, item);
        s
    }

    pub fn kind(&self) -> PotionThrowKind {
        self.kind
    }

    /// `potionItemStack.getOrDefault(POTION_CONTENTS, EMPTY)`.
    fn contents(&self) -> PotionContents {
        throwable::get_item(&self.base).get(&POTION_CONTENTS).cloned().unwrap_or(PotionContents { potion: None, custom_color: None, custom_effects: Vec::new(), custom_name: None })
    }

    /// `affectEntitiesAround(level, potion)`.
    fn affect_entities_around(&self, level: &mut ServerLevel, potion: &PotionContents) {
        let hurts_water_sensitive = potion_is_tag(potion, "hurts_water_sensitive_entities");
        let extinguishes = potion_is_tag(potion, "extinguishes_entities");
        let aabb = self.base.bounding_box().inflate(4.0, 2.0, 4.0);
        if hurts_water_sensitive || extinguishes {
            let living = level.get_entities_of_class(&aabb, |e| e.living().is_some(), |e| e.is_sensitive_to_water() || e.is_on_fire());
            for id in living {
                let Some(entity) = level.get_entity(id) else { continue };
                let dist = self.base.distance_to_sqr_vec(entity.position());
                if dist < SPLASH_RANGE_SQ {
                    if hurts_water_sensitive && entity.is_sensitive_to_water() {
                        let source = level.damage_sources().indirect_magic(self.as_dyn(), self.get_owner(level));
                        level.with_entity(id, |level, e| e.hurt_server(level, &source, 1.0));
                    }
                    if extinguishes {
                        level.with_entity(id, |level, e| {
                            if e.is_on_fire() && e.is_alive() {
                                e.extinguish_fire(level);
                            }
                        });
                    }
                }
            }
        }
        if potion_is_tag(potion, "rehydrates_axolotls") {
            for id in level.get_entities_of_class(&aabb, |e| e.get_type() == EntityType::AXOLOTL, |_| true) {
                level.with_entity(id, |_, e| e.rehydrate());
            }
        }
    }

    /// `douseFire(pos)`.
    fn douse_fire(&self, level: &mut ServerLevel, pos: BlockPos) {
        let state = level.get_block_state(pos);
        if state.is_tag("fire") {
            level.destroy_block(pos, false, Some(self.base.id), 512);
        } else if abstract_candle_block::is_lit(state) {
            abstract_candle_block::extinguish(None, state, level, pos);
        } else if campfire_block::CampfireBlock::is_lit_campfire(state) {
            level.level_event(None, 1009, pos, 0);
            campfire_block::douse(self.projectile.owner_id, level, pos);
            level.set_block_and_update(pos, state.set(props::LIT, false));
        }
    }

    /// `ThrownSplashPotion.onHitAsPotion(level, potionItem, hitResult)`.
    fn splash_on_hit_as_potion(&self, level: &mut ServerLevel, potion_item: &ItemStack, hit: &HitResult) {
        let contents = potion_item.get(&POTION_CONTENTS).cloned().unwrap_or(PotionContents { potion: None, custom_color: None, custom_effects: Vec::new(), custom_name: None });
        let duration_scale = potion_item.get(&POTION_DURATION_SCALE).copied().unwrap_or(1.0);
        let mob_effects = potion_contents::get_all_effects(&contents);
        let potion_aabb: AABB = self.base.bounding_box().move_vec(hit.get_location().subtract(self.base.position()));
        let effect_aabb = potion_aabb.inflate(4.0, 2.0, 4.0);
        let entities = level.get_entities_of_class(&effect_aabb, |e| e.living().is_some(), |_| true);
        let margin = compute_margin(self);
        if entities.is_empty() {
            return;
        }
        let effect_source = self.get_effect_source(level);
        let owner_id = self.get_owner(level).map(|o| o.id());
        for id in entities {
            let Some(entity) = level.get_entity(id) else { continue };
            if !entity.is_affected_by_potions() {
                continue;
            }
            let dist = potion_aabb.distance_to_sqr_aabb(&entity.get_bounding_box().inflate_scalar(f64::from(margin)));
            if dist >= SPLASH_RANGE_SQ {
                continue;
            }
            let scale = 1.0 - dist.sqrt() / 4.0;
            // the owner is lent out of the level so it can be passed next to `&mut level` as an effect cause
            let owner_is_victim = owner_id == Some(id);
            let lent_owner = owner_id.filter(|_| !owner_is_victim).and_then(|o| level.take_entity(o).map(|e| (o, e)));
            level.with_entity(id, |level, entity| {
                let owner_cause = match (&lent_owner, owner_is_victim) {
                    (Some((_, owner)), _) => Some(EffectCause::Entity(owner.as_ref())),
                    (None, true) => Some(EffectCause::Mob),
                    (None, false) => None,
                };
                for instance in &mob_effects {
                    let effect = instance.effect;
                    if effect.is_instantaneous() {
                        effect.apply_instantaneous_effect(level, Some(EffectCause::Entity(self)), owner_cause, entity, instance.amplifier, scale);
                    } else {
                        let duration = instance.map_duration(|d| (scale * f64::from(d) * f64::from(duration_scale) + 0.5) as i32);
                        let new_effect = MobEffectInstance::with_flags(effect, duration, instance.amplifier, instance.ambient, instance.visible);
                        if !new_effect.ends_within(20) {
                            entity.add_effect(level, new_effect, Some(effect_source));
                        }
                    }
                }
            });
            if let Some((owner_entity_id, owner)) = lent_owner {
                level.put_back_entity(owner_entity_id, owner);
            }
        }
    }

    /// `ThrownLingeringPotion.onHitAsPotion(level, potionItem, hitResult)`.
    fn lingering_on_hit_as_potion(&self, level: &mut ServerLevel, potion_item: &ItemStack, hit: &HitResult) {
        let (x, y, z) = match hit {
            HitResult::Entity(e) => match level.get_entity(e.entity) {
                Some(target) => (target.base().x(), target.base().y(), target.base().z()),
                None => (self.base.x(), self.base.y(), self.base.z()),
            },
            HitResult::Block(_) => (self.base.x(), self.base.y(), self.base.z()),
        };
        let mut cloud = AreaEffectCloud::at(level, x, y, z);
        if let Some(owner) = self.get_owner(level).filter(|o| o.living().is_some()) {
            cloud.set_owner(Some(owner));
        }
        cloud.set_radius(3.0);
        cloud.set_radius_on_use(-0.5);
        cloud.set_duration(600);
        cloud.set_wait_time(10);
        let radius = cloud.get_radius();
        let duration = cloud.get_duration();
        cloud.set_radius_per_tick(-radius / duration as f32);
        cloud.apply_components_from_item_stack(potion_item);
        level.add_fresh_entity(Box::new(cloud));
    }
}

/// `PotionContents.is(TagKey<Potion>)`.
fn potion_is_tag(contents: &PotionContents, tag: &str) -> bool {
    potion_contents::is_potion_without_custom_effects(contents) && contents.potion.is_some_and(|p: Potion| mc_data::tags::get::<Potion>(tag).is_some_and(|t| t.contains(p)))
}

/// `Potion.hasInstantEffects()`.
fn has_instant_effects(potion: Potion) -> bool {
    potion_contents::potion_effects(potion).iter().any(|e| e.effect.is_instantaneous())
}

impl Projectile for AbstractThrownPotion {
    impl_throwable_projectile!();

    fn on_hit_block(&mut self, level: &mut ServerLevel, hit: &BlockHitResult) {
        // `super.onHitBlock`: `state.onProjectileHit`
        let state = level.get_block_state(hit.block_pos);
        if let Some(this) = self.as_projectile_mut() {
            crate::world::level::block::behaviour(state.block()).on_projectile_hit(level, state, hit, this);
        }
        let potion_item = throwable::get_item(&self.base);
        let hit_direction: Direction = hit.direction;
        let block_effect_pos = hit.block_pos.relative(hit_direction);
        let contents = potion_item.get(&POTION_CONTENTS).cloned();
        if contents.is_some_and(|c| potion_is_tag(&c, "douses_fire")) {
            self.douse_fire(level, block_effect_pos);
            self.douse_fire(level, block_effect_pos.relative(hit_direction.get_opposite()));
            for direction in Direction::HORIZONTAL {
                self.douse_fire(level, block_effect_pos.relative(direction));
            }
        }
    }

    fn on_hit_entity(&mut self, _level: &mut ServerLevel, _hit: &EntityHitResult) {}

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        on_hit_base(self, level, hit);
        let potion_item = throwable::get_item(&self.base);
        let potion = self.contents();
        self.affect_entities_around(level, &potion);
        if potion_contents::has_effects(&potion) {
            match self.kind {
                PotionThrowKind::Splash => self.splash_on_hit_as_potion(level, &potion_item, hit),
                PotionThrowKind::Lingering => self.lingering_on_hit_as_potion(level, &potion_item, hit),
            }
        }
        let pos = self.block_position();
        if potion.potion.is_some_and(has_instant_effects) {
            level.level_event(None, 2007, pos, potion_contents::get_color(&potion));
            if !self.base.is_silent() {
                level.level_event(None, 1054, pos, 0);
            }
        } else {
            level.level_event(None, 2002, pos, potion_contents::get_color(&potion));
            if !self.base.is_silent() {
                level.level_event(None, 1053, pos, 0);
            }
        }
        self.discard();
    }
}

impl Entity for AbstractThrownPotion {
    impl_throwable_entity!(0.05, away);

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        throwable::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        throwable::read_additional_save_data(self, input, self.kind.default_item());
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SPLASH_POTION_FACTORY: EntityRegistration =
    EntityRegistration { entity_type: EntityType::SPLASH_POTION, create: |_, level| Box::new(AbstractThrownPotion::new(PotionThrowKind::Splash, level.next_entity_id())) };

#[distributed_slice(ENTITY_FACTORIES)]
static LINGERING_POTION_FACTORY: EntityRegistration =
    EntityRegistration { entity_type: EntityType::LINGERING_POTION, create: |_, level| Box::new(AbstractThrownPotion::new(PotionThrowKind::Lingering, level.next_entity_id())) };

#[allow(dead_code)]
fn _unused(_: Vec3, _: BlockState) {}
