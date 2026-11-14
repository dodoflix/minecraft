//! `ContainerOpenersCounter` + `ChestLidController`: the open-count state machine shared by chests, ender chests and
//! barrels (shulker boxes keep their own counter in the block entity).
//!
//! The block entity holds its `Level` in vanilla; here every operation gets the level (the block entity is out of its
//! chunk while it runs, see `ServerLevel::with_block_entity`) and the abstract methods of the Java class are the
//! [`OpenersHost`] callbacks.

use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::gameevent::GameEventContext;
use mc_core::math::mth;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::GameEvent;
use mc_data::block::BlockState;

/// The abstract methods of `ContainerOpenersCounter` (anonymous subclasses in `ChestBlockEntity`, `BarrelBlockEntity`,
/// `EnderChestBlockEntity`).
pub trait OpenersHost {
    fn on_open(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState);
    fn on_close(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState);
    fn opener_count_changed(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        previous: i32,
        current: i32,
    );
    /// `isOwnContainer(player)` for a player entity (`ContainerUser.hasContainerOpen(counter, pos)`).
    fn is_own_container(&self, player: &dyn Entity, pos: BlockPos) -> bool;
}

/// `ContainerOpenersCounter`.
#[derive(Clone, Debug, Default)]
pub struct ContainerOpenersCounter {
    open_count: i32,
    max_interaction_range: f64,
}

impl ContainerOpenersCounter {
    /// `incrementOpeners(entity, level, pos, blockState, maxInteractionRange)`.
    pub fn increment_openers(
        &mut self,
        host: &dyn OpenersHost,
        entity: EntityId,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        max_interaction_range: f64,
    ) {
        let previous = self.open_count;
        self.open_count += 1;
        if previous == 0 {
            host.on_open(level, pos, state);
            level.game_event_at_block(
                GameEvent::CONTAINER_OPEN,
                pos,
                &GameEventContext::of_entity(Some(entity)),
            );
            schedule_recheck(level, pos, state);
        }
        host.opener_count_changed(level, pos, state, previous, self.open_count);
        self.max_interaction_range = max_interaction_range.max(self.max_interaction_range);
    }

    /// `decrementOpeners(entity, level, pos, blockState)`.
    pub fn decrement_openers(
        &mut self,
        host: &dyn OpenersHost,
        entity: EntityId,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
    ) {
        let previous = self.open_count;
        self.open_count -= 1;
        if self.open_count == 0 {
            host.on_close(level, pos, state);
            level.game_event_at_block(
                GameEvent::CONTAINER_CLOSE,
                pos,
                &GameEventContext::of_entity(Some(entity)),
            );
            self.max_interaction_range = 0.0;
        }
        host.opener_count_changed(level, pos, state, previous, self.open_count);
    }

    /// `getEntitiesWithContainerOpen(level, pos)` (the entities that are `ContainerUser`s, not spectating and own the
    /// container: players through the host, the other users by their own `hasContainerOpen`).
    pub fn get_entities_with_container_open(
        &self,
        host: &dyn OpenersHost,
        level: &ServerLevel,
        pos: BlockPos,
    ) -> Vec<EntityId> {
        let range = self.max_interaction_range + 4.0;
        let search_box = AABB::from_block_pos(&pos).inflate_scalar(range);
        level.get_entities(None, &search_box, |entity| {
            if entity.is_spectator() {
                false
            } else if entity.is_player() {
                host.is_own_container(entity, pos)
            } else {
                entity.container_user_has_open(level, pos).unwrap_or(false)
            }
        })
    }

    /// `recheckOpeners(level, pos, blockState)`: the scheduled `tick` of the blocks.
    pub fn recheck_openers(
        &mut self,
        host: &dyn OpenersHost,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
    ) {
        let users = self.get_entities_with_container_open(host, level, pos);
        self.max_interaction_range = 0.0;
        for id in &users {
            if let Some(range) = level.get_entity(*id).and_then(container_interaction_range) {
                self.max_interaction_range = range.max(self.max_interaction_range);
            }
        }
        let open_count = users.len() as i32;
        let prev_count = self.open_count;
        if prev_count != open_count {
            let is_open = open_count != 0;
            let was_open = prev_count != 0;
            if is_open && !was_open {
                host.on_open(level, pos, state);
                level.game_event_at_block(
                    GameEvent::CONTAINER_OPEN,
                    pos,
                    &GameEventContext::of_entity(None),
                );
            } else if !is_open {
                host.on_close(level, pos, state);
                level.game_event_at_block(
                    GameEvent::CONTAINER_CLOSE,
                    pos,
                    &GameEventContext::of_entity(None),
                );
            }
            self.open_count = open_count;
        }
        host.opener_count_changed(level, pos, state, prev_count, open_count);
        if open_count > 0 {
            schedule_recheck(level, pos, state);
        }
    }

    /// `getOpenerCount()`.
    pub fn get_opener_count(&self) -> i32 {
        self.open_count
    }
}

/// `ContainerUser.getContainerInteractionRange()` (`Player.blockInteractionRange()`, `CopperGolem`: 3).
pub fn container_interaction_range(entity: &dyn Entity) -> Option<f64> {
    entity
        .as_any()
        .downcast_ref::<crate::world::entity::ServerPlayer>()
        .map(|p| {
            p.player
                .living
                .get_attribute_value(mc_data::Attribute::BLOCK_INTERACTION_RANGE)
        })
        .or_else(|| entity.container_user_interaction_range())
}

fn schedule_recheck(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    level.schedule_tick_block(pos, state.block(), 5);
}

/// `ChestLidController`.
#[derive(Clone, Debug, Default)]
pub struct ChestLidController {
    should_be_open: bool,
    openness: f32,
    o_openness: f32,
}

impl ChestLidController {
    pub fn tick_lid(&mut self) {
        self.o_openness = self.openness;
        if !self.should_be_open && self.openness > 0.0 {
            self.openness = (self.openness - 0.1).max(0.0);
        } else if self.should_be_open && self.openness < 1.0 {
            self.openness = (self.openness + 0.1).min(1.0);
        }
    }

    pub fn get_openness(&self, a: f32) -> f32 {
        mth::lerp_f32(a, self.o_openness, self.openness)
    }

    pub fn should_be_open(&mut self, should_be_open: bool) {
        self.should_be_open = should_be_open;
    }
}

/// `player.containerMenu instanceof ChestMenu` whose container is (or contains, for a double chest) the block entity at
/// `pos` (the `isOwnContainer` of chests and barrels).
pub fn player_has_chest_menu_open_on(entity: &dyn Entity, pos: BlockPos) -> bool {
    use crate::world::inventory::ContainerKey;
    use mc_data::MenuType;
    let Some(player) = entity
        .as_any()
        .downcast_ref::<crate::world::entity::ServerPlayer>()
    else {
        return false;
    };
    let base = player.player.menus.current().base();
    matches!(
        base.menu_type(),
        Some(
            MenuType::GENERIC_9X1
                | MenuType::GENERIC_9X2
                | MenuType::GENERIC_9X3
                | MenuType::GENERIC_9X4
                | MenuType::GENERIC_9X5
                | MenuType::GENERIC_9X6
        )
    ) && base
        .slots
        .iter()
        .any(|s| matches!(&s.container, ContainerKey::BlockEntity(p) if *p == pos))
}

/// `player.getEnderChestInventory().isActiveChest(enderChest)`.
pub fn player_has_ender_chest_active_at(entity: &dyn Entity, pos: BlockPos) -> bool {
    entity
        .as_any()
        .downcast_ref::<crate::world::entity::ServerPlayer>()
        .is_some_and(|p| p.player.ender_chest.is_active_chest(pos))
}
