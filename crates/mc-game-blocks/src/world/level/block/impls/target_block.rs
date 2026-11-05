//! `net.minecraft.world.level.block.TargetBlock`.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_core::math::mth;
use mc_core::phys::{BlockHitResult, Vec3};

pub struct TargetBlock;

/// `TargetBlock.getRedstoneStrength`.
fn get_redstone_strength(hit: &BlockHitResult, hit_location: Vec3) -> i32 {
    let hit_direction = hit.direction;
    let dist_x = (mth::frac_f64(hit_location.x) - 0.5).abs();
    let dist_y = (mth::frac_f64(hit_location.y) - 0.5).abs();
    let dist_z = (mth::frac_f64(hit_location.z) - 0.5).abs();
    let distance = match hit_direction.get_axis() {
        Axis::Y => dist_x.max(dist_z),
        Axis::Z => dist_x.max(dist_y),
        _ => dist_y.max(dist_z),
    };
    1.max(mth::ceil_f64(15.0 * mth::clamp_f64((0.5 - distance) / 0.5, 0.0, 1.0)))
}

/// `TargetBlock.updateRedstoneOutput(level, state, hitResult, entity)`: `is_arrow` = the entity is an `AbstractArrow`.
pub fn update_redstone_output(level: &mut ServerLevel, state: BlockState, hit: &BlockHitResult, is_arrow: bool) -> i32 {
    let redstone_strength = get_redstone_strength(hit, hit.location);
    let duration = if is_arrow { 20 } else { 8 };
    if !level.has_scheduled_block_tick(hit.block_pos, state.block()) {
        // `setOutputPower`
        level.set_block_and_update(hit.block_pos, state.set(props::POWER, redstone_strength));
        level.schedule_tick_block(hit.block_pos, state.block(), duration);
    }
    redstone_strength
}

impl BlockBehaviour for TargetBlock {
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::POWER) != 0 {
            level.set_block_and_update(pos, state.set(props::POWER, 0));
        }
    }
    fn on_projectile_hit(&self, level: &mut ServerLevel, state: BlockState, hit: &BlockHitResult, projectile: &mut dyn mc_game_core::world::entity::projectile::Projectile) {
        let is_arrow = projectile.tracking_kind() == mc_game_core::world::entity::TrackingKind::Arrow;
        let output_strength = update_redstone_output(level, state, hit, is_arrow);
        if let Some(owner) = projectile.get_owner(level).filter(|o| o.is_player()).map(|o| o.id()) {
            level.with_player(owner, |level, player| {
                player.award_stat_once(level, mc_game_core::stats::stats::Stats::TARGET_HIT);
                mc_game_core::advancements::triggers::CriteriaTriggers::TARGET_BLOCK_HIT.trigger(level, player, projectile.as_dyn(), hit.location, output_strength);
            });
        }
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        state.get(props::POWER)
    }
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, _moved_by_piston: bool) {
        if !state.is_block(old_state.block()) && state.get(props::POWER) > 0 && !level.has_scheduled_block_tick(pos, state.block()) {
            level.set_block_flags(pos, state.set(props::POWER, 0), 18);
        }
    }
}

register_block_class!("TargetBlock", |_| Box::new(TargetBlock));
