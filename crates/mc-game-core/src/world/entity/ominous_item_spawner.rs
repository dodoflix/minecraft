//! `net.minecraft.world.entity.OminousItemSpawner`: spawned above a player by an ominous trial spawner, it drops its item
//! 60-120 ticks later.
//! shortcut: items that are `ProjectileItem`s (lingering potions, arrows, ...) are dropped as plain `ItemEntity`s until
//! `world::item::projectile_item` exists; wire `spawnProjectile` there.

use super::entity::{self, Entity, EntityData};
use super::factory::{ENTITY_FACTORIES, EntityRegistration};
use super::item::ItemEntity;
use super::EntityId;
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::item::{ItemStack, ItemStackExt, load_item_stack};
use crate::world::level::LevelAccessor;
use linkme::distributed_slice;
use mc_core::random::RandomSource;
use mc_data::{EntityType, GameEvent, SoundEvent};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::packets::play::level::SoundSource;
use std::any::Any;

define_accessor!(pub DATA_ITEM: ItemStack = OminousItemSpawner);

/// `TICKS_BEFORE_ABOUT_TO_SPAWN_SOUND`.
const TICKS_BEFORE_ABOUT_TO_SPAWN_SOUND: i64 = 36;

pub struct OminousItemSpawner {
    base: EntityData,
    spawn_item_after_ticks: i64,
}

impl OminousItemSpawner {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM, ItemStack::empty());
        });
        base.no_physics = true;
        Self { base, spawn_item_after_ticks: 0 }
    }

    /// `OminousItemSpawner.create(level, item)`.
    pub fn create(level: &mut ServerLevel, item: ItemStack) -> Self {
        let mut spawner = Self::new(EntityType::OMINOUS_ITEM_SPAWNER, level.next_entity_id());
        spawner.spawn_item_after_ticks = i64::from(level.random().next_int_between_inclusive(60, 120));
        spawner.set_item(item);
        spawner
    }

    pub fn get_item(&self) -> ItemStack {
        self.base.entity_data.get(DATA_ITEM)
    }

    fn set_item(&mut self, item: ItemStack) {
        self.base.entity_data.set(DATA_ITEM, item);
    }

    /// `spawnItem()`.
    fn spawn_item(&mut self, level: &mut ServerLevel) {
        let item = self.get_item();
        if item.is_empty() {
            return;
        }
        let p = self.base.position();
        let spawned = ItemEntity::with_stack(level, p.x, p.y, p.z, item);
        let spawned_id = spawned.id();
        level.add_fresh_entity(Box::new(spawned));
        let block_pos = self.base.block_position();
        level.level_event(None, 3021, block_pos, 1);
        level.game_event_from_entity(Some(spawned_id), GameEvent::ENTITY_PLACE, p);
        self.set_item(ItemStack::empty());
    }
}

impl Entity for OminousItemSpawner {
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
        if i64::from(self.base.tick_count) == self.spawn_item_after_ticks - TICKS_BEFORE_ABOUT_TO_SPAWN_SOUND {
            let pos = self.base.block_position();
            level.play_sound(None, pos, SoundEvent::BLOCK_TRIAL_SPAWNER_ABOUT_TO_SPAWN_ITEM, SoundSource::Neutral, 1.0, 1.0);
        }
        if i64::from(self.base.tick_count) >= self.spawn_item_after_ticks {
            self.spawn_item(level);
            self.kill();
        }
    }

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        false
    }
    fn could_accept_passenger(&self) -> bool {
        false
    }
    fn is_ignoring_block_triggers(&self) -> bool {
        true
    }
    fn hurt_server(&mut self, _level: &mut ServerLevel, _source: &crate::world::damagesource::DamageSource, _amount: f32) -> bool {
        false
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        let item = self.get_item();
        if !item.is_empty() {
            let mut t = CompoundTag::new();
            item.save_into(&mut t);
            out.put("item", Tag::Compound(t));
        }
        out.put_long("spawn_item_after_ticks", self.spawn_item_after_ticks);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        let item = input.get_compound("item").and_then(load_item_stack).unwrap_or_else(ItemStack::empty);
        self.set_item(item);
        self.spawn_item_after_ticks = input.get_long_or("spawn_item_after_ticks", 0);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static OMINOUS_ITEM_SPAWNER_FACTORY: EntityRegistration =
    EntityRegistration { entity_type: EntityType::OMINOUS_ITEM_SPAWNER, create: |ty, id, _registries| Box::new(OminousItemSpawner::new(ty, id)) };
