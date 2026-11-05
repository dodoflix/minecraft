//! `net.minecraft.world.level.block.BellBlock`.
use super::face_attached_horizontal_directional_block as face_attached;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BellBlockEntity;
use crate::world::entity::{EntityId, ServerPlayer};
use crate::stats::stats::Stats;
use crate::world::entity::projectile::Projectile;
use crate::world::item::{BlockPlaceContext, InteractionResult, ItemStack};
use crate::world::level::explosion::{Explosion, default_on_explosion_hit};
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::redstone::Orientation;
use crate::world::level::signal_getter;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_data::block::PathComputationType;
use mc_data::enums::BellAttachType;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::packets::play::SoundSource;

pub struct BellBlock;

fn get_connected_direction(state: BlockState) -> Direction {
    match state.get(props::BELL_ATTACHMENT) {
        BellAttachType::Floor => Direction::Up,
        BellAttachType::Ceiling => Direction::Down,
        _ => state.get(props::HORIZONTAL_FACING).get_opposite(),
    }
}

/// `BellBlock.onHit(level, state, hitResult, player, requireHitFromCorrectSide)` (`player` = the ringing player's id).
/// Returns `(proper hit, did ring)`; the caller awards `Stats.BELL_RING` to the player when the bell rang.
pub fn on_hit(level: &mut ServerLevel, state: BlockState, hit: &BlockHitResult, player: Option<EntityId>, require_hit_from_correct_side: bool) -> (bool, bool) {
    let direction = hit.direction;
    let block_pos = hit.block_pos;
    let proper_hit = !require_hit_from_correct_side || is_proper_hit(state, direction, hit.location.y - block_pos.y() as f64);
    if proper_hit {
        let did_ring = attempt_to_ring(level, player, block_pos, Some(direction));
        (true, did_ring)
    } else {
        (false, false)
    }
}

/// `isProperHit(state, clickedDirection, clickY)`.
fn is_proper_hit(state: BlockState, clicked_direction: Direction, click_y: f64) -> bool {
    if clicked_direction.get_axis() != Axis::Y && !(click_y > 0.8124f32 as f64) {
        let facing = state.get(props::HORIZONTAL_FACING);
        match state.get(props::BELL_ATTACHMENT) {
            BellAttachType::Floor => facing.get_axis() == clicked_direction.get_axis(),
            BellAttachType::SingleWall | BellAttachType::DoubleWall => facing.get_axis() != clicked_direction.get_axis(),
            BellAttachType::Ceiling => true,
        }
    } else {
        false
    }
}

/// `BellBlock.attemptToRing(ringingEntity, level, pos, direction)`.
pub fn attempt_to_ring(level: &mut ServerLevel, ringing_entity: Option<EntityId>, pos: BlockPos, direction: Option<Direction>) -> bool {
    if level.get_block_entity(pos).is_some_and(|be| be.as_any().is::<BellBlockEntity>()) {
        let direction = direction.unwrap_or_else(|| level.get_block_state(pos).get(props::HORIZONTAL_FACING));
        level.with_block_entity(pos, |level, be| {
            if let Some(bell) = be.as_any_mut().downcast_mut::<BellBlockEntity>() {
                bell.on_hit(level, direction);
            }
        });
        level.play_sound(None, pos, SoundEvent::BLOCK_BELL_USE, SoundSource::Blocks, 2.0, 1.0);
        level.game_event_from_entity(ringing_entity, GameEvent::BLOCK_CHANGE, Vec3::at_center_of(&pos));
        true
    } else {
        false
    }
}

impl BlockBehaviour for BellBlock {
    fn neighbor_changed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block, _orientation: Option<Orientation>, _moved_by_piston: bool) {
        let signal = signal_getter::has_neighbor_signal(&*level, pos);
        if signal != state.get(props::POWERED) {
            if signal {
                attempt_to_ring(level, None, pos, None);
            }
            level.set_block_and_update(pos, state.set(props::POWERED, signal));
        }
    }
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, _pos: BlockPos, player: &mut ServerPlayer, hit: &BlockHitResult) -> InteractionResult {
        let (proper, did_ring) = on_hit(level, state, hit, Some(player.base_id()), true);
        if did_ring {
            player.award_stat_once(level, Stats::BELL_RING);
        }
        if proper { InteractionResult::SUCCESS } else { InteractionResult::PASS }
    }
    fn on_projectile_hit(&self, level: &mut ServerLevel, state: BlockState, hit: &BlockHitResult, projectile: &mut dyn Projectile) {
        let owner = projectile.get_owner(level).filter(|o| o.is_player()).map(|o| o.id());
        let (_, did_ring) = on_hit(level, state, hit, owner, true);
        if did_ring && let Some(id) = owner {
            level.with_player(id, |level, player| player.award_stat_once(level, Stats::BELL_RING));
        }
    }
    fn on_explosion_hit(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, explosion: &dyn Explosion, on_hit: &mut dyn FnMut(ItemStack, BlockPos)) {
        if explosion.can_trigger_blocks() {
            attempt_to_ring(level, None, pos, None);
        }
        default_on_explosion_hit(state, level, pos, explosion, on_hit);
    }
    fn trigger_event(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, b0: i32, b1: i32) -> bool {
        helpers::base_entity_trigger_event(level, pos, b0, b1)
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let clicked_face = context.get_clicked_face();
        let pos = context.get_clicked_pos();
        let axis = clicked_face.get_axis();
        if axis == Axis::Y {
            let state = block
                .default_state()
                .set(props::BELL_ATTACHMENT, if clicked_face == Direction::Down { BellAttachType::Ceiling } else { BellAttachType::Floor })
                .set(props::HORIZONTAL_FACING, context.get_horizontal_direction());
            if state.can_survive(&*level, pos) {
                return Some(state);
            }
        } else {
            let sturdy = |level: &ServerLevel, side: Direction| {
                let p = pos.relative(side);
                level.get_block_state(p).is_face_sturdy_at(level, p, side.get_opposite())
            };
            let double_attached = axis == Axis::X && sturdy(&*level, Direction::West) && sturdy(&*level, Direction::East)
                || axis == Axis::Z && sturdy(&*level, Direction::North) && sturdy(&*level, Direction::South);
            let mut state = block
                .default_state()
                .set(props::HORIZONTAL_FACING, clicked_face.get_opposite())
                .set(props::BELL_ATTACHMENT, if double_attached { BellAttachType::DoubleWall } else { BellAttachType::SingleWall });
            if state.can_survive(&*level, pos) {
                return Some(state);
            }
            let can_attach_below = level.get_block_state(pos.below()).is_face_sturdy_at(&*level, pos.below(), Direction::Up);
            state = state.set(props::BELL_ATTACHMENT, if can_attach_below { BellAttachType::Floor } else { BellAttachType::Ceiling });
            if state.can_survive(&*level, pos) {
                return Some(state);
            }
        }
        None
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let connection_dir = get_connected_direction(state).get_opposite();
        if connection_dir == Direction::Up {
            helpers::can_support_center(level, pos.above(), Direction::Down)
        } else {
            face_attached::can_attach(level, pos, connection_dir)
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        let attachment = state.get(props::BELL_ATTACHMENT);
        let connected_direction = get_connected_direction(state).get_opposite();
        if connected_direction == direction && !state.can_survive(&*level, pos) && attachment != BellAttachType::DoubleWall {
            return air();
        }
        if direction.get_axis() == state.get(props::HORIZONTAL_FACING).get_axis() {
            if attachment == BellAttachType::DoubleWall && !neighbour_state.is_face_sturdy_at(&*level, neighbour_pos, direction) {
                return state.set(props::BELL_ATTACHMENT, BellAttachType::SingleWall).set(props::HORIZONTAL_FACING, direction.get_opposite());
            }
            if attachment == BellAttachType::SingleWall
                && connected_direction.get_opposite() == direction
                && neighbour_state.is_face_sturdy_at(&*level, neighbour_pos, state.get(props::HORIZONTAL_FACING))
            {
                return state.set(props::BELL_ATTACHMENT, BellAttachType::DoubleWall);
            }
        }
        state
    });
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BellBlock", |_| Box::new(BellBlock));
