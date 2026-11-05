//! `SpawnEggItem`: spawning the egg's mob (`useOn`, `use` on liquids) and `spawnOffspringFromSpawnEgg` (baby spawn).

use super::{InteractionHand, InteractionResult, ItemBehaviour, ItemStack, ItemStackExt, UseOnContext, player_pov_hit_result};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::spawn_config::{apply_default_stack_config, create_at};
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::level::block::BlockStateExt;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, GameEvent};
use mc_protocol::components::ENTITY_DATA;
use mc_protocol::packets::play::Stat;

/// `SpawnEggItem.getType(itemStack)`.
pub fn get_type(stack: &ItemStack) -> Option<EntityType> {
    stack.get(&ENTITY_DATA).map(|d| d.ty)
}

/// `SpawnEggItem.spawnsEntity(itemStack, type)`.
pub fn spawns_entity(stack: &ItemStack, ty: EntityType) -> bool {
    get_type(stack) == Some(ty)
}

/// `type.canSpawn(level)`.
fn can_spawn(level: &ServerLevel, ty: EntityType) -> bool {
    level.can_spawn_entity_type(ty)
}

/// `SpawnEggItem.spawnMob(type, user, itemStack, level, spawnPos, tryMoveDown, movedUp)`; `stack` is consumed on success.
fn spawn_mob(ty: EntityType, user: &ServerPlayer, stack: &mut ItemStack, level: &mut ServerLevel, spawn_pos: BlockPos, try_move_down: bool, moved_up: bool) -> InteractionResult {
    let config_stack = stack.clone();
    let created = create_at(
        level,
        ty,
        Some(&mut |level: &mut ServerLevel, entity: &mut dyn Entity| apply_default_stack_config(level, entity, &config_stack, Some(user))),
        spawn_pos,
        EntitySpawnReason::SpawnItemUse,
        try_move_down,
        moved_up,
    );
    let Some(entity) = created else { return InteractionResult::FAIL };
    let id = entity.id();
    level.add_fresh_entity_with_passengers(vec![entity]);
    // `EntityType.spawn`: `mob.playAmbientSound()`
    level.with_entity(id, |level, entity| {
        if entity.is_mob() {
            entity.play_ambient_sound(level);
        }
    });
    stack.consume(1, user.player.abilities.instabuild);
    level.game_event(GameEvent::ENTITY_PLACE, Vec3::at_center_of(&spawn_pos), &GameEventContext::of_entity(Some(user.base_id())));
    InteractionResult::SUCCESS
}

/// `SpawnEggItem.spawnOffspringFromSpawnEgg(player, parent, type, level, pos, spawnEggStack)`.
pub fn spawn_offspring_from_spawn_egg(player: &ServerPlayer, parent: &mut dyn Entity, ty: EntityType, level: &mut ServerLevel, pos: Vec3, stack: &mut ItemStack) -> Option<EntityId> {
    if !spawns_entity(stack, ty) {
        return None;
    }
    let parent_is_ageable = parent.mob().is_some_and(|m| m.ageable.is_some());
    let mut offspring = if parent_is_ageable {
        let parent_id = parent.id();
        parent.get_breed_offspring(level, parent_id)?
    } else {
        factory::create(ty, level, EntitySpawnRequest { reason: EntitySpawnReason::SpawnItemUse, ignore_checks: false })?
    };
    crate::world::entity::mob::ageable::set_baby(offspring.as_mut(), true);
    if !offspring.is_baby() {
        return None;
    }
    offspring.snap_to(pos.x, pos.y, pos.z, 0.0, 0.0);
    offspring.apply_implicit_components(stack);
    let id = offspring.id();
    level.add_fresh_entity_with_passengers(vec![offspring]);
    stack.consume(1, player.player.abilities.instabuild);
    Some(id)
}

pub struct SpawnEggItem;

impl ItemBehaviour for SpawnEggItem {
    fn use_on(&self, context: &mut UseOnContext, level: &mut ServerLevel, player: &mut ServerPlayer) -> InteractionResult {
        let Some(ty) = get_type(&context.item_stack) else { return InteractionResult::FAIL };
        if !can_spawn(level, ty) {
            return InteractionResult::FAIL;
        }
        let pos = context.get_clicked_pos();
        let clicked_face = context.get_clicked_face();
        let block_state = level.get_block_state(pos);
        let spawn_pos = if block_state.get_collision_shape(&*level, pos).is_empty() { pos } else { pos.relative(clicked_face) };
        let moved_up = pos != spawn_pos && clicked_face == Direction::Up;
        spawn_mob(ty, player, &mut context.item_stack, level, spawn_pos, true, moved_up)
    }

    fn use_item(&self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let mut stack = player.get_item_in_hand(hand);
        let hit = player_pov_hit_result(level, player, true);
        if hit.miss {
            return InteractionResult::PASS;
        }
        let Some(ty) = get_type(&stack) else { return InteractionResult::FAIL };
        if !can_spawn(level, ty) {
            return InteractionResult::FAIL;
        }
        let pos = hit.block_pos;
        if !level.get_block_state(pos).block().data().classes.contains(&"LiquidBlock") {
            return InteractionResult::PASS;
        }
        if super::may_interact(level, player, pos) && super::may_use_item_at(level, player, pos, hit.direction, &stack) {
            let result = spawn_mob(ty, player, &mut stack, level, pos, false, false);
            player.set_item_in_hand(hand, stack.clone());
            if result == InteractionResult::SUCCESS {
                player.award_stat_once(level, Stat::Used(stack.item()));
            }
            result
        } else {
            InteractionResult::FAIL
        }
    }
}

register_item_class!("SpawnEggItem", |_| Box::new(SpawnEggItem));
