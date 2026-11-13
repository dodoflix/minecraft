//! `net.minecraft.world.entity.projectile.arrow.ThrownTrident`: loyalty return, channeling through the weapon's
//! enchantment effects, riptide is the item's business (`TridentItem`).

use super::{AbstractArrow, AbstractArrowData, OnHitEntityOutcome, Pickup};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::player::ServerPlayer;
use crate::world::entity::projectile::{
    EntityHitResult, Projectile, ProjectileData, ProjectileDeflection,
};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::item::ItemStack;
use crate::world::item::enchantment::actors::Actors;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use std::any::Any;

define_accessor!(pub ID_LOYALTY: Byte = ThrownTrident);
define_accessor!(pub ID_FOIL: Boolean = ThrownTrident);

pub struct ThrownTrident {
    base: EntityData,
    projectile: ProjectileData,
    arrow: AbstractArrowData,
    dealt_damage: bool,
    pub client_side_return_trident_tick_count: i32,
    /// The loyalty is derived from the pickup stack through the enchantments (needs the level): after a load it is
    /// recomputed by `read_level_dependent_data` before the trident is added.
    loyalty_pending: bool,
}

impl ThrownTrident {
    /// `ThrownTrident(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            super::define_synched_data(b);
            b.define(ID_LOYALTY, 0);
            b.define(ID_FOIL, false);
        });
        Self {
            base,
            projectile: ProjectileData::default(),
            arrow: AbstractArrowData::new(
                SoundEvent::ITEM_TRIDENT_HIT_GROUND,
                ItemStack::new(Item::TRIDENT),
            ),
            dealt_damage: false,
            client_side_return_trident_tick_count: 0,
            loyalty_pending: false,
        }
    }

    /// `ThrownTrident(level, owner, tridentItem)`.
    pub fn from_owner(
        level: &mut ServerLevel,
        owner: &dyn Entity,
        trident_item: ItemStack,
    ) -> Self {
        let mut trident = Self::new(EntityType::TRIDENT, level.next_entity_id());
        super::init_from_owner(&mut trident, level, owner, trident_item.clone(), None);
        trident.init_loyalty_and_foil(level, &trident_item);
        trident
    }

    /// `ThrownTrident(level, x, y, z, tridentItem)`: the trident is also the weapon.
    pub fn at(level: &mut ServerLevel, x: f64, y: f64, z: f64, trident_item: ItemStack) -> Self {
        let mut trident = Self::new(EntityType::TRIDENT, level.next_entity_id());
        super::init_at(
            &mut trident,
            level,
            x,
            y,
            z,
            trident_item.clone(),
            Some(&trident_item),
        );
        trident.init_loyalty_and_foil(level, &trident_item);
        trident
    }

    fn init_loyalty_and_foil(&mut self, level: &mut ServerLevel, trident_item: &ItemStack) {
        let loyalty = self.get_loyalty_from_item(level, trident_item);
        self.base.entity_data.set(ID_LOYALTY, loyalty);
        self.base.entity_data.set(
            ID_FOIL,
            crate::world::item::behaviour(trident_item.item()).is_foil(trident_item),
        );
    }

    /// `getLoyaltyFromItem(tridentItem)`.
    fn get_loyalty_from_item(&mut self, level: &mut ServerLevel, trident_item: &ItemStack) -> i8 {
        let id = self.base.id;
        let acceleration =
            crate::world::item::enchantment_helper::get_trident_return_to_owner_acceleration(
                level,
                &mut Actors::of(self.as_dyn_mut()),
                trident_item,
                id,
            );
        mth::clamp_i32(acceleration, 0, 127) as i8
    }

    /// `isFoil()`.
    pub fn is_foil(&self) -> bool {
        self.base.entity_data.get(ID_FOIL)
    }

    pub fn dealt_damage(&self) -> bool {
        self.dealt_damage
    }

    pub fn loyalty(&self) -> i8 {
        self.base.entity_data.get(ID_LOYALTY)
    }

    /// `isAcceptibleReturnOwner()`.
    fn is_acceptible_return_owner(&self, level: &ServerLevel) -> bool {
        match self.get_owner(level) {
            Some(owner) => {
                owner.is_alive()
                    && (owner.as_any().downcast_ref::<ServerPlayer>().is_none()
                        || !owner.is_spectator())
            }
            None => false,
        }
    }
}

impl Projectile for ThrownTrident {
    fn projectile_data(&self) -> &ProjectileData {
        &self.projectile
    }
    fn projectile_data_mut(&mut self) -> &mut ProjectileData {
        &mut self.projectile
    }
    crate::impl_abstract_arrow_projectile!();
}

impl AbstractArrow for ThrownTrident {
    fn arrow_data(&self) -> &AbstractArrowData {
        &self.arrow
    }
    fn arrow_data_mut(&mut self) -> &mut AbstractArrowData {
        &mut self.arrow
    }
    fn as_abstract_arrow(&self) -> &dyn AbstractArrow {
        self
    }
    fn as_abstract_arrow_mut(&mut self) -> &mut dyn AbstractArrow {
        self
    }
    fn get_default_pickup_item(&self) -> ItemStack {
        ItemStack::new(Item::TRIDENT)
    }
    fn get_default_hit_ground_sound_event(&self) -> SoundEvent {
        SoundEvent::ITEM_TRIDENT_HIT_GROUND
    }
    fn get_water_inertia(&self) -> f32 {
        0.99
    }

    fn tick_despawn(&mut self) {
        if self.arrow.pickup != Pickup::Allowed || self.loyalty() <= 0 {
            super::tick_despawn_base(self);
        }
    }

    fn find_hit_entities(&self, level: &ServerLevel, from: Vec3, to: Vec3) -> Vec<EntityHitResult> {
        let hit = if self.dealt_damage {
            None
        } else {
            super::find_hit_entity(self, level, from, to)
        };
        hit.into_iter().collect()
    }

    fn arrow_on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
        use crate::world::item::enchantment_helper as helper;
        let victim = hit.entity;
        let mut dmg = 8.0f32;
        let source = {
            let owner = self.get_owner(level);
            level
                .damage_sources()
                .trident(self.as_dyn(), Some(owner.unwrap_or(self.as_dyn())))
        };
        let weapon = self.get_weapon_item();
        if let Some(weapon) = &weapon {
            dmg = helper::modify_damage(
                level,
                &mut Actors::of(self.as_dyn_mut()),
                weapon,
                victim,
                &source,
                dmg,
            );
        }
        self.dealt_damage = true;
        let mut outcome = OnHitEntityOutcome::default();
        level.with_entity(victim, |level, target| {
            outcome.was_hurt = level.lend(self.as_dyn_mut(), |level| {
                target.hurt_server(level, &source, dmg)
            });
            if outcome.was_hurt {
                let broke = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let flag = broke.clone();
                helper::do_post_attack_effects_with_item_source_on_break(
                    level,
                    &mut Actors::of(&mut *target).and(self.as_dyn_mut()),
                    victim,
                    &source,
                    weapon.as_ref(),
                    Some(Box::new(move |_, _| {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed)
                    })),
                );
                if broke.load(std::sync::atomic::Ordering::Relaxed) {
                    self.kill();
                }
                if target.living().is_some() {
                    super::do_knockback(self, level, target, &source);
                    self.do_post_hurt_effects(level, target);
                }
            }
            outcome.receives_side_effects =
                target.projectile_receives_side_effects_on_hit(outcome.was_hurt);
        });
        if outcome.receives_side_effects {
            self.play_sound(level, SoundEvent::ITEM_TRIDENT_HIT, 1.0, 1.0);
            let owner = self.projectile.owner;
            self.deflect(
                level,
                ProjectileDeflection::Reverse,
                Some(victim),
                owner,
                false,
                Vec3::new(0.02, 0.2, 0.02),
            );
        }
    }

    fn hit_block_enchantment_effects(
        &mut self,
        level: &mut ServerLevel,
        hit: &BlockHitResult,
        weapon: ItemStack,
    ) {
        let (weapon, broke) = super::run_on_hit_block(self, level, hit, weapon);
        // `getWeaponItem()` is the pickup stack: the live object the effects modify
        self.arrow.pickup_item_stack = weapon;
        if broke {
            self.kill();
        }
    }

    fn try_pickup(&mut self, player: &mut ServerPlayer) -> bool {
        if super::try_pickup_base(self, player) {
            return true;
        }
        if self.is_no_physics() && self.owned_by(player) {
            let mut stack = self.get_pickup_item();
            let infinite = player.has_infinite_materials();
            return player.player.inventory.add(&mut stack, infinite);
        }
        false
    }
}

impl Entity for ThrownTrident {
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
    crate::impl_abstract_arrow_entity!();

    fn get_weapon_item(&self) -> Option<ItemStack> {
        Some(self.arrow.pickup_item_stack.clone())
    }

    fn read_level_dependent_data(&mut self, level: &mut ServerLevel) {
        if self.loyalty_pending {
            self.loyalty_pending = false;
            let stack = self.arrow.pickup_item_stack.clone();
            let loyalty = self.get_loyalty_from_item(level, &stack);
            self.base.entity_data.set(ID_LOYALTY, loyalty);
        }
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        if self.arrow.in_ground_time > 4 {
            self.dealt_damage = true;
        }
        let loyalty = i32::from(self.loyalty());
        let owner_id = self.get_owner(level).map(|o| o.id());
        if loyalty > 0 && (self.dealt_damage || self.is_no_physics()) && owner_id.is_some() {
            if !self.is_acceptible_return_owner(level) {
                if self.arrow.pickup == Pickup::Allowed {
                    crate::world::entity::item::spawn_at_location(
                        level,
                        self.as_dyn(),
                        self.get_pickup_item(),
                        Vec3::new(0.0, f64::from(0.1f32), 0.0),
                    );
                }
                self.discard();
            } else {
                let (owner_eye, owner_is_player, owner_width) = {
                    let owner = self.get_owner(level).expect("owner checked above");
                    (
                        owner.base().get_eye_position(),
                        owner.is_player(),
                        owner.base().get_bb_width(),
                    )
                };
                if !owner_is_player
                    && self.position().distance_to(owner_eye) < f64::from(owner_width) + 1.0
                {
                    self.discard();
                    return;
                }
                self.set_no_physics(true);
                let vec = owner_eye.subtract(self.position());
                let p = self.position();
                self.base
                    .set_pos_raw(p.x, p.y + vec.y * 0.015 * f64::from(loyalty), p.z);
                let accel = 0.05 * f64::from(loyalty);
                let delta = self
                    .base
                    .delta_movement()
                    .scale(0.95)
                    .add(vec.normalize().scale(accel));
                self.base.set_delta_movement(delta);
                if self.client_side_return_trident_tick_count == 0 {
                    self.play_sound(level, SoundEvent::ITEM_TRIDENT_RETURN, 10.0, 1.0);
                }
                self.client_side_return_trident_tick_count += 1;
            }
        }
        super::tick(self, level);
    }

    fn player_touch(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        if self.owned_by(player) || self.get_owner(level).is_none() {
            super::player_touch(self, level, player);
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        super::add_additional_save_data(self, out);
        out.put_boolean("DealtDamage", self.dealt_damage);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        super::read_additional_save_data(self, input);
        self.dealt_damage = input.get_boolean_or("DealtDamage", false);
        self.loyalty_pending = true;
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static TRIDENT_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::TRIDENT,
    create: |ty, id, _registries| Box::new(ThrownTrident::new(ty, id)),
};
