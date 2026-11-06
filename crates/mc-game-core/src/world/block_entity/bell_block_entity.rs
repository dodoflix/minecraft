//! `BellBlockEntity`: shake animation state, the nearby-entity snapshot taken when the bell is rung (villager
//! `HEARD_BELL_TIME` memory) and raid resonation (raiders within range glow once the resonation ended).

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::server::level::ServerLevel;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::LevelAccessor;
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, EntityType, MobEffect, SoundEvent};
use mc_protocol::packets::play::SoundSource;

pub struct BellBlockEntity {
    base: BlockEntityData,
    last_ring_timestamp: i64,
    pub ticks: i32,
    pub shaking: bool,
    pub click_direction: Option<Direction>,
    /// `nearbyEntities` (`null` until the first ring).
    nearby_entities: Option<Vec<EntityId>>,
    resonating: bool,
    resonation_ticks: i32,
}

impl BellBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::BELL, pos, state),
            last_ring_timestamp: 0,
            ticks: 0,
            shaking: false,
            click_direction: None,
            nearby_entities: None,
            resonating: false,
            resonation_ticks: 0,
        }
    }

    /// `BellBlockEntity.onHit(clickDirection)`.
    pub fn on_hit(&mut self, level: &mut ServerLevel, click_direction: Direction) {
        let bell_pos = self.get_block_pos();
        self.click_direction = Some(click_direction);
        if self.shaking {
            self.ticks = 0;
        } else {
            self.shaking = true;
        }
        level.block_event(
            bell_pos,
            self.get_block_state().block(),
            1,
            click_direction.get_3d_data_value() as i32,
        );
    }

    /// `updateEntities()`.
    fn update_entities(&mut self, level: &mut ServerLevel) {
        let block_pos = self.get_block_pos();
        let game_time = level.get_game_time();
        if game_time > self.last_ring_timestamp + 60 || self.nearby_entities.is_none() {
            self.last_ring_timestamp = game_time;
            let aabb = AABB::from_block_pos(&block_pos).inflate_scalar(48.0);
            self.nearby_entities =
                Some(level.get_entities_of_class(&aabb, |e| e.living().is_some(), |_| true));
        }
        for id in self.nearby_entities.clone().unwrap_or_default() {
            if is_alive_and_loaded(level, id)
                && let Some(entity) = level.get_entity(id)
                && block_pos.closer_to_center_than(&entity.position(), 32.0)
            {
                // `entity.getBrain().setMemory(MemoryModuleType.HEARD_BELL_TIME, gameTime)`
                hear_bell(level, id, game_time);
            }
        }
    }

    /// `areRaidersNearby(bellPos, nearbyEntities)`.
    fn are_raiders_nearby(&self, level: &ServerLevel) -> bool {
        let pos = self.get_block_pos();
        self.nearby_entities.iter().flatten().any(|&id| {
            is_alive_and_loaded(level, id)
                && level.get_entity(id).is_some_and(|e| {
                    pos.closer_to_center_than(&e.position(), 32.0) && is_raider(e.get_type())
                })
        })
    }

    /// `makeRaidersGlow(level, pos, nearbyEntities)`.
    fn make_raiders_glow(&self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        for &id in self.nearby_entities.iter().flatten() {
            if is_raider_within_range(level, pos, id) {
                glow(level, id);
            }
        }
    }
}

/// `entity.isAlive() && !entity.isRemoved()` of an entity that may have left the level since the snapshot.
fn is_alive_and_loaded(level: &ServerLevel, id: EntityId) -> bool {
    level
        .get_entity(id)
        .is_some_and(|e| e.is_alive() && !e.is_removed())
}

fn is_raider(ty: EntityType) -> bool {
    mc_data::tags::get::<EntityType>("raiders").is_some_and(|t| t.contains(ty))
}

/// `isRaiderWithinRange(blockPos, entity)`.
fn is_raider_within_range(level: &ServerLevel, pos: BlockPos, id: EntityId) -> bool {
    is_alive_and_loaded(level, id)
        && level.get_entity(id).is_some_and(|e| {
            pos.closer_to_center_than(&e.position(), 48.0) && is_raider(e.get_type())
        })
}

impl BlockEntity for BellBlockEntity {
    block_entity_base!();

    fn trigger_event(&mut self, level: &mut ServerLevel, b0: i32, b1: i32) -> bool {
        if b0 == 1 {
            self.update_entities(level);
            self.resonation_ticks = 0;
            self.click_direction = Some(Direction::from_3d_data_value(b1));
            self.ticks = 0;
            self.shaking = true;
            true
        } else {
            false
        }
    }

    fn is_ticking(&self) -> bool {
        true
    }

    /// `BellBlockEntity.serverTick`.
    fn server_tick(&mut self, level: &mut ServerLevel) {
        if self.shaking {
            self.ticks += 1;
        }
        if self.ticks >= 50 {
            self.shaking = false;
            self.ticks = 0;
        }
        let pos = self.get_block_pos();
        if self.ticks >= 5 && self.resonation_ticks == 0 && self.are_raiders_nearby(level) {
            self.resonating = true;
            level.play_sound(
                None,
                pos,
                SoundEvent::BLOCK_BELL_RESONATE,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
        if self.resonating {
            if self.resonation_ticks < 40 {
                self.resonation_ticks += 1;
            } else {
                self.make_raiders_glow(level);
                self.resonating = false;
            }
        }
    }
}

/// `entity.getBrain().setMemory(MemoryModuleType.HEARD_BELL_TIME, gameTime)`.
fn hear_bell(level: &mut ServerLevel, id: EntityId, game_time: i64) {
    if let Some(entity) = level.get_entity_mut(id) {
        entity.set_heard_bell_time(game_time);
    }
}

/// `raider.addEffect(new MobEffectInstance(MobEffects.GLOWING, 60))`.
fn glow(level: &mut ServerLevel, id: EntityId) {
    level.with_entity(id, |level, entity| {
        entity.add_effect(
            level,
            MobEffectInstance::of(MobEffect::GLOWING, 60, 0),
            None,
        );
    });
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static BELL: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::BELL,
    create: |pos, state| Box::new(BellBlockEntity::new(pos, state)),
};
