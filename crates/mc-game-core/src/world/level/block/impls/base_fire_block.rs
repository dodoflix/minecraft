//! `net.minecraft.world.level.block.BaseFireBlock` (the static helpers; `FireBlock`/`SoulFireBlock` extend it).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::portal::PortalShape;
use mc_core::direction::Plane;

/// `BaseFireBlock.getState(level, pos)`: soul fire on soul-fire base blocks, else `FireBlock.getStateForPlacement`.
pub fn get_state(level: &dyn BlockGetter, pos: BlockPos) -> BlockState {
    if super::soul_fire_block::can_survive_on_block(level.get_block_state(pos.below())) {
        Block::SOUL_FIRE.default_state()
    } else {
        super::fire_block::get_state_for_placement(level, pos)
    }
}

/// `BaseFireBlock.entityInside` (`fire_damage`: 1.0 for fire, 2.0 for soul fire).
pub fn entity_inside(
    effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
    fire_damage: f32,
) {
    use crate::world::entity::inside_block_effects::InsideBlockEffectType;
    effect_applier.apply(InsideBlockEffectType::ClearFreeze);
    effect_applier.apply(InsideBlockEffectType::FireIgnite);
    effect_applier.run_after(
        InsideBlockEffectType::FireIgnite,
        Box::new(move |entity, level| {
            let source = level.damage_sources().in_fire();
            entity.hurt_server(level, &source, fire_damage);
        }),
    );
}

/// `BaseFireBlock.playerWillDestroy`: the fire-extinguish sound event, then `Block.playerWillDestroy`.
pub fn player_will_destroy(
    state: BlockState,
    level: &mut crate::server::level::ServerLevel,
    pos: BlockPos,
    player: &mut crate::world::entity::ServerPlayer,
) -> BlockState {
    level.level_event(None, 1009, pos, 0);
    crate::server::level::block_break::player_will_destroy(level, pos, state, player)
}

/// `BaseFireBlock.inPortalDimension(level)`.
fn in_portal_dimension(level: &ServerLevel) -> bool {
    let d = level.dimension.to_string();
    d == "minecraft:overworld" || d == "minecraft:the_nether"
}

/// `BaseFireBlock.onPlace`: a fire inside a valid empty frame lights the portal (`FireBlock` then schedules its tick).
pub fn on_place(state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState) {
    if !old_state.is_block(state.block()) {
        if in_portal_dimension(level)
            && let Some(shape) = PortalShape::find_empty_portal_shape(&*level, pos, Axis::X)
        {
            shape.create_portal_blocks(level);
            return;
        }

        if !state.can_survive(&*level, pos) {
            level.remove_block(pos, false);
        }
    }
}

/// `BaseFireBlock.canBePlacedAt(level, pos, forwardDirection)`.
pub fn can_be_placed_at(
    level: &mut ServerLevel,
    pos: BlockPos,
    forward_direction: Direction,
) -> bool {
    let state = level.get_block_state(pos);
    if !state.is_air() {
        false
    } else {
        get_state(&*level, pos).can_survive(&*level, pos)
            || is_portal(level, pos, forward_direction)
    }
}

fn is_portal(level: &mut ServerLevel, pos: BlockPos, forward_direction: Direction) -> bool {
    if !in_portal_dimension(level) {
        return false;
    }
    for face in Direction::ALL {
        let state = level.get_block_state(pos.relative(face));
        if crate::world::level::portal::portal_shape::is_frame(state) {
            let preferred_axis = if forward_direction.get_axis().is_horizontal() {
                forward_direction.get_counter_clockwise().get_axis()
            } else {
                Plane::Horizontal.get_random_axis(level.random())
            };
            return PortalShape::find_empty_portal_shape(&*level, pos, preferred_axis).is_some();
        }
    }
    false
}
