//! `net.minecraft.world.entity.projectile.arrow.Arrow`: the (tipped) arrow.

use super::{AbstractArrow, AbstractArrowData};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::effect::potion_contents;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::projectile::{Projectile, ProjectileData};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::item::{ItemStack, ItemStackExt as _};
use linkme::distributed_slice;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{POTION_CONTENTS, POTION_DURATION_SCALE, PotionContents};
use std::any::Any;

define_accessor!(pub ID_EFFECT_COLOR: Int = Arrow);

const EXPOSED_POTION_DECAY_TIME: i32 = 600;
const NO_EFFECT_COLOR: i32 = -1;
/// `EVENT_POTION_PUFF`.
const EVENT_POTION_PUFF: i8 = 0;

/// `PotionContents.EMPTY`.
pub fn empty_potion_contents() -> PotionContents {
    PotionContents { potion: None, custom_color: None, custom_effects: Vec::new(), custom_name: None }
}

pub struct Arrow {
    base: EntityData,
    projectile: ProjectileData,
    arrow: AbstractArrowData,
}

impl Arrow {
    /// `Arrow(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            super::define_synched_data(b);
            b.define(ID_EFFECT_COLOR, NO_EFFECT_COLOR);
        });
        Self { base, projectile: ProjectileData::default(), arrow: AbstractArrowData::new(SoundEvent::ENTITY_ARROW_HIT, ItemStack::new(Item::ARROW)) }
    }

    /// `Arrow(level, x, y, z, pickupItemStack, firedFromWeapon)`.
    pub fn at(level: &mut ServerLevel, x: f64, y: f64, z: f64, pickup_item_stack: ItemStack, fired_from_weapon: Option<&ItemStack>) -> Self {
        let mut arrow = Self::new(EntityType::ARROW, level.next_entity_id());
        super::init_at(&mut arrow, level, x, y, z, pickup_item_stack, fired_from_weapon);
        arrow.update_color();
        arrow
    }

    /// `Arrow(level, owner, pickupItemStack, firedFromWeapon)`.
    pub fn from_owner(level: &mut ServerLevel, owner: &dyn Entity, pickup_item_stack: ItemStack, fired_from_weapon: Option<&ItemStack>) -> Self {
        let mut arrow = Self::new(EntityType::ARROW, level.next_entity_id());
        super::init_from_owner(&mut arrow, level, owner, pickup_item_stack, fired_from_weapon);
        arrow.update_color();
        arrow
    }

    fn get_potion_contents(&self) -> PotionContents {
        self.arrow.pickup_item_stack.get(&POTION_CONTENTS).cloned().unwrap_or_else(empty_potion_contents)
    }

    fn get_potion_duration_scale(&self) -> f32 {
        self.arrow.pickup_item_stack.get(&POTION_DURATION_SCALE).copied().unwrap_or(1.0)
    }

    fn set_potion_contents(&mut self, contents: PotionContents) {
        self.arrow.pickup_item_stack.set(POTION_CONTENTS, contents);
        self.update_color();
    }

    /// `updateColor()`.
    fn update_color(&mut self) {
        let contents = self.get_potion_contents();
        let color = if contents == empty_potion_contents() { -1 } else { potion_contents::get_color(&contents) };
        self.base.entity_data.set(ID_EFFECT_COLOR, color);
    }

    /// `addEffect(effect)`: `withEffectAdded`.
    pub fn add_effect(&mut self, effect: mc_protocol::components::MobEffectInstance) {
        let mut contents = self.get_potion_contents();
        contents.custom_effects.push(effect);
        self.set_potion_contents(contents);
    }

    /// `getColor()`.
    pub fn get_color(&self) -> i32 {
        self.base.entity_data.get(ID_EFFECT_COLOR)
    }
}

impl Projectile for Arrow {
    fn projectile_data(&self) -> &ProjectileData {
        &self.projectile
    }
    fn projectile_data_mut(&mut self) -> &mut ProjectileData {
        &mut self.projectile
    }
    crate::impl_abstract_arrow_projectile!();
}

impl AbstractArrow for Arrow {
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
        ItemStack::new(Item::ARROW)
    }

    fn set_pickup_item_stack(&mut self, stack: ItemStack) {
        super::set_pickup_item_stack_base(self, stack);
        self.update_color();
    }

    fn do_post_hurt_effects(&mut self, level: &mut ServerLevel, mob: &mut dyn Entity) {
        let effect_source = self.get_effect_source(level);
        let contents = self.get_potion_contents();
        let scale = self.get_potion_duration_scale();
        potion_contents::for_each_effect(&contents, scale, |effect| {
            mob.add_effect(level, effect, Some(effect_source));
        });
    }
}

impl Entity for Arrow {
    fn base(&self) -> &EntityData {
        &self.base
    }

    fn player_touch(&mut self, level: &mut ServerLevel, player: &mut crate::world::entity::ServerPlayer) {
        super::player_touch(self, level, player);
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
        self.arrow.fired_from_weapon.clone()
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        super::tick(self, level);
        // server side of `Arrow.tick`: the exposed potion arrow decays into a plain one
        if self.is_in_ground() && self.arrow.in_ground_time != 0 && self.get_potion_contents() != empty_potion_contents() && self.arrow.in_ground_time >= EXPOSED_POTION_DECAY_TIME {
            level.broadcast_entity_event(self.base.id, EVENT_POTION_PUFF);
            self.set_pickup_item_stack(ItemStack::new(Item::ARROW));
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        super::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        super::read_additional_save_data(self, input);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ARROW_FACTORY: EntityRegistration = EntityRegistration { entity_type: EntityType::ARROW, create: |ty, id, _registries| Box::new(Arrow::new(ty, id)) };
