//! `net.minecraft.world.level.block.CaveVines` (interface shared by `CaveVinesBlock` and `CaveVinesPlantBlock`).
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::item::{InteractionResult, ItemStack};
use crate::world::level::LevelAccessor;
use crate::world::level::block::helpers;
use crate::world::level::storage::loot::block_drops::block_interact_drops;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::item::Item;
use mc_data::props;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;

/// `new ItemStack(Items.GLOW_BERRIES)`.
pub fn glow_berries() -> ItemStack {
    ItemStack::new(Item::GLOW_BERRIES)
}

/// `CaveVines.use(sourceEntity, state, level, pos)`.
pub fn use_(
    source_entity: &dyn Entity,
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
) -> InteractionResult {
    if !state.get(props::BERRIES) {
        return InteractionResult::PASS;
    }
    let drops = block_interact_drops(
        level,
        "minecraft:harvest/cave_vine",
        pos,
        state,
        None,
        Some(source_entity),
    );
    for stack in drops {
        helpers::pop_resource(level, pos, stack);
    }
    let pitch = mth::random_between(level.random(), 0.8, 1.2);
    level.play_sound(
        None,
        pos,
        SoundEvent::BLOCK_CAVE_VINES_PICK_BERRIES,
        SoundSource::Blocks,
        1.0,
        pitch,
    );
    let new_state = state.set(props::BERRIES, false);
    level.set_block_flags(pos, new_state, 2);
    level.game_event_from_entity_ref_in_state(
        source_entity,
        Some(new_state),
        GameEvent::BLOCK_CHANGE,
        mc_core::phys::Vec3::at_center_of(&pos),
    );
    InteractionResult::SUCCESS
}
