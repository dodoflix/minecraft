//! `net.minecraft.world.level.block.AbstractBedBlock` (`BedBlock` and `StrawBedBlock` are told apart through the Java
//! class chain: a bed is a `StrawBedBlock` or a plain `BedBlock`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::stats::Stat;
use crate::world::attribute::BedRule;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::{BlockPlaceContext, InteractionResult, ItemStack};
use crate::world::level::explosion::ExplosionInteraction;
use mc_core::phys::{AABB, Vec3};
use mc_data::enums::BedPart;
use mc_data::{EntityType, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;
use mc_text::Component;

pub struct AbstractBedBlock;

/// `AbstractBedBlock.getNeighbourDirection`.
fn get_neighbour_direction(part: BedPart, facing: Direction) -> Direction {
    if part == BedPart::Foot { facing } else { facing.get_opposite() }
}

/// `block instanceof StrawBedBlock`.
fn is_straw_bed(state: BlockState) -> bool {
    helpers::is_instance_of(state, "StrawBedBlock")
}

/// `AbstractBedBlock.getBedRule(level, pos)`.
pub fn get_bed_rule(level: &ServerLevel, state: BlockState, _pos: BlockPos) -> BedRule {
    level.bed_rule(is_straw_bed(state))
}

/// `AbstractBedBlock.getSleptInBedStatType()`.
pub fn get_slept_in_bed_stat_type(state: BlockState) -> Stat {
    if is_straw_bed(state) { crate::stats::stats::Stats::SLEEP_IN_STRAW_BED } else { crate::stats::stats::Stats::SLEEP_IN_BED }
}

/// `AbstractBedBlock.getConnectedDirection(state)`.
pub fn get_connected_direction(state: BlockState) -> Direction {
    let facing = state.get(props::HORIZONTAL_FACING);
    if state.get(props::BED_PART) == BedPart::Head { facing.get_opposite() } else { facing }
}

/// `AbstractBedBlock.getSleepHeight(state, level, pos)` (the block's own override for `StrawBedBlock`).
pub fn get_sleep_height(state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> Option<f64> {
    if !helpers::is_instance_of(state, "AbstractBedBlock") {
        return None;
    }
    let (state, pos) = if is_straw_bed(state) && state.get(props::BED_PART) == BedPart::Head {
        // `StrawBedBlock.getSleepHeight`: the head lies on the foot part's shape
        let laying_on_pos = pos.relative(get_connected_direction(state));
        let laying_on = level.get_block_state(laying_on_pos);
        if !laying_on.is_block(state.block()) || laying_on.get(props::BED_PART) != BedPart::Foot {
            return None;
        }
        (laying_on, laying_on_pos)
    } else {
        (state, pos)
    };
    let shape = state.get_shape(level, pos);
    if shape.is_empty() { None } else { Some(shape.max(Axis::Y)) }
}

/// `AbstractBedBlock.onStopSleeping(level, pos)`.
pub fn on_stop_sleeping(level: &mut ServerLevel, pos: BlockPos) {
    let state = level.get_block_state(pos);
    if get_bed_rule(level, state, pos).destroy_on_leave {
        destroy_on_leave(level, state, pos);
    }
}

/// `StrawBedBlock.destroyBed` (the only `destroyOnLeave`; `BedBlock.destroyOnLeave` does nothing).
fn destroy_on_leave(level: &mut ServerLevel, state: BlockState, pos: BlockPos) {
    if is_straw_bed(state) {
        destroy_straw_bed(level, pos);
    }
}

fn destroy_straw_bed(level: &mut ServerLevel, pos: BlockPos) {
    level.play_sound(None, pos, SoundEvent::BLOCK_STRAW_BED_BREAK_LEAVE, SoundSource::Blocks, 1.0, 1.0);
    level.set_block_and_update(pos, air());
}

/// `BedBlock.destroyOnUse` / `StrawBedBlock.destroyOnUse`.
fn destroy_on_use(state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer) -> InteractionResult {
    if is_straw_bed(state) {
        destroy_straw_bed(level, pos);
        return InteractionResult::SUCCESS_SERVER;
    }
    level.remove_block(pos, false);
    let other_pos = pos.relative(state.get(props::HORIZONTAL_FACING).get_opposite());
    if level.get_block_state(other_pos).is_block(state.block()) {
        level.remove_block(other_pos, false);
    }
    let boom_pos = Vec3::at_center_of(&pos);
    let damage_source = level.damage_sources().bad_respawn_point_explosion(boom_pos);
    let params = Explode::new(None, boom_pos.x, boom_pos.y, boom_pos.z, 5.0, ExplosionInteraction::Block).with_fire(true).with_damage_source(Some(damage_source), None);
    let participants: &mut [&mut dyn Entity] = &mut [player];
    level.explode(params, participants);
    InteractionResult::SUCCESS_SERVER
}

/// `kickVillagerOutOfBed(level, pos)`: the first sleeping villager in the bed block wakes up.
fn kick_villager_out_of_bed(level: &mut ServerLevel, pos: BlockPos) -> bool {
    let bb = AABB::new(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()), f64::from(pos.x() + 1), f64::from(pos.y() + 1), f64::from(pos.z() + 1));
    let villagers = level.get_entities_of_class(&bb, |e| e.get_type() == EntityType::VILLAGER, |e| e.living().is_some_and(|l| l.is_sleeping()));
    match villagers.first() {
        Some(&id) => {
            level.with_entity(id, |level, villager| villager.stop_sleeping(level));
            true
        }
        None => false,
    }
}

/// `AbstractBedBlock.findStandUpPosition(type, level, pos, forward, yaw)` (shared with `LivingEntity.stopSleeping`).
pub use crate::world::entity::living_entity::sleeping::find_stand_up_position;

impl BlockBehaviour for AbstractBedBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == get_neighbour_direction(state.get(props::BED_PART), state.get(props::HORIZONTAL_FACING)) {
            if neighbour_state.is_block(state.block()) && neighbour_state.get(props::BED_PART) != state.get(props::BED_PART) {
                state.set(props::OCCUPIED, neighbour_state.get(props::OCCUPIED))
            } else {
                air()
            }
        } else {
            state
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        let part = state.get(props::BED_PART);
        let other_state = level.get_block_state(pos.relative(get_neighbour_direction(part, state.get(props::HORIZONTAL_FACING))));
        if !(other_state.is_block(state.block()) && other_state.get(props::BED_PART) != part) {
            return InteractionResult::CONSUME;
        }
        let (state, pos) = if part != BedPart::Head {
            let head_pos = pos.relative(state.get(props::HORIZONTAL_FACING));
            (level.get_block_state(head_pos), head_pos)
        } else {
            (state, pos)
        };
        let bed_rule = get_bed_rule(level, state, pos);
        if bed_rule.destroy_on_use {
            if let Some(message) = bed_rule.error_message.clone() {
                player.send_overlay_message(message);
            }
            destroy_on_use(state, level, pos, player)
        } else if state.get(props::OCCUPIED) {
            if !kick_villager_out_of_bed(level, pos) {
                player.send_overlay_message(Component::translatable("block.minecraft.bed.occupied", Vec::new()));
            }
            InteractionResult::SUCCESS_SERVER
        } else {
            if let Err(problem) = player.start_sleep_in_bed(level, state, &bed_rule, pos)
                && let Some(message) = problem.message
            {
                player.send_overlay_message(message);
            }
            InteractionResult::SUCCESS_SERVER
        }
    }
    fn player_will_destroy(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer) -> BlockState {
        if player.player.abilities.instabuild {
            let part = state.get(props::BED_PART);
            if part == BedPart::Foot {
                let head_pos = pos.relative(get_neighbour_direction(part, state.get(props::HORIZONTAL_FACING)));
                let head_state = level.get_block_state(head_pos);
                if head_state.is_block(state.block()) && head_state.get(props::BED_PART) == BedPart::Head {
                    level.set_block_flags(head_pos, air(), 35);
                    level.level_event(Some(player.base_id()), 2001, head_pos, head_state.id() as i32);
                }
            }
        }
        crate::server::level::block_break::player_will_destroy(level, pos, state, player)
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let facing = context.get_horizontal_direction();
        let relative = context.get_clicked_pos().relative(facing);
        if level.get_block_state(relative).can_be_replaced_by(context) && level.get_world_border().is_within_bounds_block(&relative) {
            Some(block.default_state().set(props::HORIZONTAL_FACING, facing))
        } else {
            None
        }
    }
    fn set_placed_by(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _placer: Option<&mut ServerPlayer>, _stack: &ItemStack) {
        let other_pos = pos.relative(state.get(props::HORIZONTAL_FACING));
        level.set_block_and_update(other_pos, state.set(props::BED_PART, BedPart::Head));
    }
    fn get_seed(&self, state: BlockState, pos: BlockPos) -> i64 {
        let source_pos = pos.relative_n(state.get(props::HORIZONTAL_FACING), if state.get(props::BED_PART) == BedPart::Head { 0 } else { 1 });
        mc_core::math::mth::get_seed_xyz(source_pos.x(), pos.y(), source_pos.z())
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: mc_data::block::PathComputationType) -> bool {
        false
    }
    /// The bed part of `ServerPlayer.findRespawnAndUseSpawnBlock`.
    fn find_respawn_position(&self, level: &mut ServerLevel, state: BlockState, pos: BlockPos, _forced: bool, _consume_spawn_block: bool, yaw: f32) -> Option<Option<Vec3>> {
        if !get_bed_rule(level, state, pos).can_set_spawn(level) {
            return None;
        }
        Some(find_stand_up_position(EntityType::PLAYER, level, pos, state.get(props::HORIZONTAL_FACING), yaw))
    }
}

register_block_class!("AbstractBedBlock", |_| Box::new(AbstractBedBlock));
