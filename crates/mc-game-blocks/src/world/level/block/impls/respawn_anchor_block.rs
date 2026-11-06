//! `net.minecraft.world.level.block.RespawnAnchorBlock`.
use mc_core::phys::Vec3;
use mc_data::block::BlockState as State;
use mc_data::fluid::FluidState;
use mc_data::{EntityType, Item};
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::server::level::level_explode::Explode;
use mc_game_core::world::entity::dismount_helper::find_safe_dismount_location;
use mc_game_core::world::entity::respawn::respawn_data_of;
use mc_game_core::world::entity::{Entity, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::explosion::{
    Explosion, ExplosionDamageCalculator, ExplosionInteraction, default_block_explosion_resistance,
};
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_protocol::packets::play::level::SoundSource;

pub struct RespawnAnchorBlock;

/// `RESPAWN_HORIZONTAL_OFFSETS` / `RESPAWN_OFFSETS`.
const RESPAWN_HORIZONTAL_OFFSETS: [(i32, i32, i32); 8] = [
    (0, 0, -1),
    (-1, 0, 0),
    (0, 0, 1),
    (1, 0, 0),
    (-1, 0, -1),
    (1, 0, -1),
    (-1, 0, 1),
    (1, 0, 1),
];

fn respawn_offsets() -> Vec<(i32, i32, i32)> {
    let mut offsets: Vec<(i32, i32, i32)> = RESPAWN_HORIZONTAL_OFFSETS.to_vec();
    offsets.extend(
        RESPAWN_HORIZONTAL_OFFSETS
            .iter()
            .map(|&(x, y, z)| (x, y - 1, z)),
    );
    offsets.extend(
        RESPAWN_HORIZONTAL_OFFSETS
            .iter()
            .map(|&(x, y, z)| (x, y + 1, z)),
    );
    offsets.push((0, 1, 0));
    offsets
}

/// `RespawnAnchorBlock.findStandUpPosition(type, level, pos)`.
pub fn find_stand_up_position(
    entity_type: EntityType,
    level: &ServerLevel,
    pos: BlockPos,
) -> Option<Vec3> {
    find_stand_up_position_checked(entity_type, level, pos, true)
        .or_else(|| find_stand_up_position_checked(entity_type, level, pos, false))
}

fn find_stand_up_position_checked(
    entity_type: EntityType,
    level: &ServerLevel,
    pos: BlockPos,
    check_dangerous: bool,
) -> Option<Vec3> {
    respawn_offsets().into_iter().find_map(|(x, y, z)| {
        find_safe_dismount_location(entity_type, level, pos.offset(x, y, z), check_dangerous)
    })
}

/// `canBeCharged(state)`.
fn can_be_charged(state: State) -> bool {
    state.get(props::RESPAWN_ANCHOR_CHARGES) < 4
}

/// `RespawnAnchorBlock.canSetSpawn(level, pos)`.
pub fn can_set_spawn(level: &ServerLevel, pos: BlockPos) -> bool {
    level.attribute_bool(
        mc_game_core::world::attribute::attributes::RESPAWN_ANCHOR_WORKS,
        pos,
    )
}

/// `RespawnAnchorBlock.charge(sourceEntity, level, pos, state)`.
pub fn charge(
    source_entity: Option<mc_game_core::world::entity::EntityId>,
    level: &mut ServerLevel,
    pos: BlockPos,
    state: State,
) {
    let new_state = state.set(
        props::RESPAWN_ANCHOR_CHARGES,
        state.get(props::RESPAWN_ANCHOR_CHARGES) + 1,
    );
    level.set_block_and_update(pos, new_state);
    level.game_event_at_block(
        mc_data::GameEvent::BLOCK_CHANGE,
        pos,
        &GameEventContext::of(source_entity, Some(new_state)),
    );
    level.play_sound(
        None,
        pos,
        mc_data::SoundEvent::BLOCK_RESPAWN_ANCHOR_CHARGE,
        SoundSource::Blocks,
        1.0,
        1.0,
    );
}

/// `getScaledChargeLevel(state, maximum)`.
pub fn get_scaled_charge_level(state: State, maximum: i32) -> i32 {
    mc_core::math::mth::floor_f64(f64::from(
        state.get(props::RESPAWN_ANCHOR_CHARGES) as f32 / 4.0 * maximum as f32,
    ))
}

/// `isWaterThatWouldFlow(pos, level)`.
fn is_water_that_would_flow(pos: BlockPos, level: &ServerLevel) -> bool {
    let fluid = level.get_fluid_state(pos);
    if !fluid.is_tag("water") {
        false
    } else if fluid.is_source() {
        true
    } else {
        let amount = f32::from(fluid.amount());
        if amount < 2.0 {
            false
        } else {
            !level.get_fluid_state(pos.below()).is_tag("water")
        }
    }
}

/// The anonymous `ExplosionDamageCalculator` of `explode`.
struct AnchorExplosionCalculator {
    pos: BlockPos,
    in_water: bool,
}

impl ExplosionDamageCalculator for AnchorExplosionCalculator {
    fn get_block_explosion_resistance(
        &self,
        _explosion: &dyn Explosion,
        _level: &dyn BlockGetter,
        test_pos: BlockPos,
        block: State,
        fluid: FluidState,
    ) -> Option<f32> {
        if test_pos == self.pos && self.in_water {
            Some(Block::WATER.data().explosion_resistance)
        } else {
            default_block_explosion_resistance(block, fluid)
        }
    }
}

/// `explode(state, level, pos)`; `player` (taken out of the level) takes part in the explosion.
fn explode(level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer) {
    level.remove_block(pos, false);
    let any_water_neighbors = Direction::HORIZONTAL
        .iter()
        .any(|d| is_water_that_would_flow(pos.relative(*d), level));
    let in_water = any_water_neighbors || level.get_fluid_state(pos.above()).is_tag("water");
    let calculator = AnchorExplosionCalculator { pos, in_water };
    let boom_pos = Vec3::at_center_of(&pos);
    let damage_source = level.damage_sources().bad_respawn_point_explosion(boom_pos);
    let params = Explode::new(
        None,
        boom_pos.x,
        boom_pos.y,
        boom_pos.z,
        5.0,
        ExplosionInteraction::Block,
    )
    .with_fire(true)
    .with_damage_source(Some(damage_source), Some(&calculator));
    let participants: &mut [&mut dyn Entity] = &mut [player];
    level.explode(params, participants);
}

impl BlockBehaviour for RespawnAnchorBlock {
    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: State,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if stack.is(Item::GLOWSTONE) && can_be_charged(state) {
            charge(Some(player.base_id()), level, pos, state);
            let mut held = stack.clone();
            mc_game_core::world::item::ItemStackExt::consume(
                &mut held,
                1,
                player.has_infinite_materials(),
            );
            player.set_item_in_hand(hand, held);
            InteractionResult::SUCCESS
        } else if hand == InteractionHand::MainHand
            && player
                .get_item_in_hand(InteractionHand::OffHand)
                .is(Item::GLOWSTONE)
            && can_be_charged(state)
        {
            InteractionResult::PASS
        } else {
            InteractionResult::TRY_WITH_EMPTY_HAND
        }
    }

    fn use_without_item(
        &self,
        state: State,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if state.get(props::RESPAWN_ANCHOR_CHARGES) == 0 {
            return InteractionResult::PASS;
        }
        if !can_set_spawn(level, pos) {
            explode(level, pos, player);
            return InteractionResult::SUCCESS_SERVER;
        }
        let new_config = mc_game_core::world::entity::respawn::RespawnConfig {
            respawn_data: respawn_data_of(level.dimension.clone(), pos, 0.0, 0.0),
            forced: false,
        };
        if !new_config.is_same_position(player.respawn_config.as_ref()) {
            player.set_respawn_position(Some(new_config), true);
            level.play_sound(
                None,
                pos,
                mc_data::SoundEvent::BLOCK_RESPAWN_ANCHOR_SET_SPAWN,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
            return InteractionResult::SUCCESS_SERVER;
        }
        InteractionResult::CONSUME
    }

    fn get_analog_output_signal(
        &self,
        state: State,
        _level: &ServerLevel,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        get_scaled_charge_level(state, 15)
    }

    /// The anchor part of `ServerPlayer.findRespawnAndUseSpawnBlock`.
    fn find_respawn_position(
        &self,
        level: &mut ServerLevel,
        state: State,
        pos: BlockPos,
        forced: bool,
        consume_spawn_block: bool,
        _yaw: f32,
    ) -> Option<Option<Vec3>> {
        let charges = state.get(props::RESPAWN_ANCHOR_CHARGES);
        if !((forced || charges > 0) && can_set_spawn(level, pos)) {
            return None;
        }
        let stand_up = find_stand_up_position(EntityType::PLAYER, level, pos);
        if !forced && consume_spawn_block && stand_up.is_some() {
            level.set_block_and_update(pos, state.set(props::RESPAWN_ANCHOR_CHARGES, charges - 1));
        }
        Some(stand_up)
    }
}

register_block_class!("RespawnAnchorBlock", |_| Box::new(RespawnAnchorBlock));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respawn_offsets_are_the_25_vanilla_offsets_in_order() {
        let offsets = respawn_offsets();
        assert_eq!(offsets.len(), 25);
        assert_eq!(offsets[0], (0, 0, -1));
        assert_eq!(offsets[8], (0, -1, -1)); // the horizontal ring one block lower, then one block higher
        assert_eq!(offsets[16], (0, 1, -1));
        assert_eq!(offsets[24], (0, 1, 0));
    }

    #[test]
    fn scaled_charge_level_matches_floor_of_charge_over_four() {
        let state = Block::RESPAWN_ANCHOR.default_state();
        let levels: Vec<i32> = (0..=4)
            .map(|c| get_scaled_charge_level(state.set(props::RESPAWN_ANCHOR_CHARGES, c), 15))
            .collect();
        assert_eq!(levels, vec![0, 3, 7, 11, 15]);
    }
}
