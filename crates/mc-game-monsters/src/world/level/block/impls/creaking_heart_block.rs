//! `net.minecraft.world.level.block.CreakingHeartBlock`.
use crate::world::block_entity::CreakingHeartBlockEntity;
use mc_data::enums::CreakingHeartState;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::rotated_pillar_block::rotate_pillar;
use mc_game_core::{register_block_class, update_shape};

pub struct CreakingHeartBlock;

/// `hasRequiredLogs(state, level, pos)`.
pub fn has_required_logs(state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
    let axis = state.get(props::AXIS);
    Direction::VALUES
        .into_iter()
        .filter(|d| d.get_axis() == axis)
        .all(|dir| {
            let neighbour = level.get_block_state(pos.relative(dir));
            neighbour.is_tag("pale_oak_logs") && neighbour.get(props::AXIS) == axis
        })
}

/// `updateState(state, level, pos)`.
fn update_state(state: BlockState, level: &ServerLevel, pos: BlockPos) -> BlockState {
    let has_logs = has_required_logs(state, level, pos);
    let disabled = state.get(props::CREAKING_HEART_STATE) == CreakingHeartState::Uprooted;
    if has_logs && disabled {
        state.set(
            props::CREAKING_HEART_STATE,
            if level.attribute_bool(
                mc_game_core::world::attribute::attributes::CREAKING_ACTIVE,
                pos,
            ) {
                CreakingHeartState::Awake
            } else {
                CreakingHeartState::Dormant
            },
        )
    } else {
        state
    }
}

/// `tryAwardExperience(player, state, level, pos)`.
fn try_award_experience(
    prevents_block_drops: bool,
    spectator: bool,
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
) {
    if !prevents_block_drops && !spectator && state.get(props::NATURAL) {
        let amount = mc_core::math::mth::random_between_inclusive(level.random(), 20, 24);
        helpers::pop_experience(level, pos, amount);
    }
}

impl BlockBehaviour for CreakingHeartBlock {
    fn on_explosion_hit(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        explosion: &dyn mc_game_core::world::level::explosion::Explosion,
        on_hit: &mut dyn FnMut(mc_game_core::world::item::ItemStack, BlockPos),
    ) {
        if explosion.should_affect_blocklike_entities()
            && let Some(source) = explosion.server_damage_source()
            && level
                .with_block_entity(pos, |level, be| {
                    be.as_any_mut()
                        .downcast_mut::<CreakingHeartBlockEntity>()
                        .map(|heart| heart.remove_protector(level, Some(source)))
                        .is_some()
                })
                .unwrap_or(false)
        {
            let player = explosion
                .get_indirect_source_entity()
                .and_then(|id| level.get_entity(id))
                .and_then(|e| {
                    e.as_any()
                        .downcast_ref::<mc_game_core::world::entity::ServerPlayer>()
                })
                .map(|p| (p.prevents_block_drops(), p.is_spectator()));
            if let Some((prevents_block_drops, spectator)) = player {
                try_award_experience(prevents_block_drops, spectator, state, level, pos);
            }
        }
        mc_game_core::world::level::explosion::default_on_explosion_hit(
            state, level, pos, explosion, on_hit,
        );
    }

    fn player_will_destroy(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut mc_game_core::world::entity::ServerPlayer,
    ) -> BlockState {
        let damage_source = level.damage_sources().player_attack(player);
        let is_heart = level
            .with_block_entity(pos, |level, be| {
                be.as_any_mut()
                    .downcast_mut::<CreakingHeartBlockEntity>()
                    .map(|heart| heart.remove_protector(level, Some(&damage_source)))
                    .is_some()
            })
            .unwrap_or(false);
        if is_heart {
            try_award_experience(
                player.prevents_block_drops(),
                player.is_spectator(),
                state,
                level,
                pos,
            );
        }
        mc_game_core::server::level::block_break::player_will_destroy(level, pos, state, player)
    }

    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(update_state(
            block
                .default_state()
                .set(props::AXIS, context.get_clicked_face().get_axis()),
            level,
            context.get_clicked_pos(),
        ))
    }

    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
            state
        }
    );

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let new_state = update_state(state, level, pos);
        if new_state != state {
            level.set_block_and_update(pos, new_state);
        }
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        rotate_pillar(state, rotation)
    }

    /// `Containers.updateNeighboursAfterDestroy(state, level, pos)`.
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        level.update_neighbour_for_output_signal(pos, state.block());
    }

    fn get_analog_output_signal(
        &self,
        state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        if state.get(props::CREAKING_HEART_STATE) == CreakingHeartState::Uprooted {
            0
        } else {
            level
                .get_block_entity(pos)
                .and_then(|be| be.as_any().downcast_ref::<CreakingHeartBlockEntity>())
                .map_or(0, CreakingHeartBlockEntity::get_analog_output_signal)
        }
    }
}

register_block_class!("CreakingHeartBlock", |_| Box::new(CreakingHeartBlock));
