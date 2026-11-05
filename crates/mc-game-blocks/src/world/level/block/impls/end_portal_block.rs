//! `net.minecraft.world.level.block.EndPortalBlock` (a `Portal`: overworld/anywhere -> End platform, End -> respawn).
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::MinecraftServer;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::server::players::dimension_change::{PostTeleportTransition, TeleportTransition};
use mc_game_core::server::players::respawn::find_respawn_position_and_use_spawn_block;
use mc_game_core::server::players::teleport_transition::adjusted_spawn_pos;
use mc_game_core::server::world_dimensions::end_key;
use mc_game_core::world::entity::inside_block_effects::InsideBlockEffectApplier;
use mc_game_core::world::entity::portal::Portal;
use mc_game_core::world::entity::{Entity, ServerPlayer};
use mc_game_core::world::level::levelgen::feature::end_platform_feature::EndPlatformFeature;
use mc_game_core::world::level::shapes::voxel_shape;
use mc_core::phys::Vec3;
use mc_core::phys::shapes::VoxelShape;
use mc_protocol::packets::play::{Relative, Relatives};

/// The `EndPortalBlock` instance entities register as their `Portal` (non-zero-sized: portals are compared by address).
pub struct EndPortalBlock {
    _unique: u8,
}

pub static END_PORTAL_BLOCK: EndPortalBlock = EndPortalBlock { _unique: 0 };

/// `ServerLevel.END_SPAWN_POINT`.
pub const END_SPAWN_POINT: BlockPos = BlockPos::new(100, 50, 0);

impl BlockBehaviour for EndPortalBlock {
    fn get_entity_inside_collision_shape(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, _entity: &dyn Entity) -> VoxelShape {
        voxel_shape(state.shape())
    }

    fn entity_inside(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, _effect_applier: &mut dyn InsideBlockEffectApplier, _is_precise: bool) {
        if entity.can_use_portal(false) {
            if level.dimension == end_key()
                && let Some(player) = entity.as_any_mut().downcast_mut::<ServerPlayer>()
                && !player.seen_credits
            {
                player.show_end_credits(level);
            } else {
                entity.set_as_inside_portal(level, &END_PORTAL_BLOCK, pos);
            }
        }
    }

    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> mc_game_core::world::item::ItemStack {
        mc_game_core::world::item::ItemStack::empty()
    }

    fn as_portal(&self) -> Option<&'static dyn Portal> {
        Some(&END_PORTAL_BLOCK)
    }

    fn can_be_replaced_by_fluid(&self, _state: BlockState, _fluid: mc_data::fluid::Fluid) -> bool {
        false
    }
}

impl Portal for EndPortalBlock {
    fn get_portal_destination(&self, server: &mut MinecraftServer, current_level: &mut ServerLevel, entity: &mut dyn Entity, _portal_entry_pos: BlockPos) -> Option<TeleportTransition> {
        let respawn_data = server.get_respawn_data();
        let from_end = current_level.dimension == end_key();
        let new_dimension = if from_end { respawn_data.dimension.clone() } else { end_key() };
        let spawn_block_pos = if from_end { respawn_data.pos } else { END_SPAWN_POINT };
        if new_dimension != current_level.dimension && server.level(&new_dimension).is_none() {
            return None;
        }
        let mut spawn_pos = Vec3::at_bottom_center_of(&spawn_block_pos);
        let (y_rot, x_rot, relatives);
        if !from_end {
            // `EndPlatformFeature.createEndPlatform(newLevel, BlockPos.containing(spawnPos).below(), true)`
            let platform_origin = BlockPos::containing(spawn_pos.x, spawn_pos.y, spawn_pos.z).below();
            match server.level_mut(&new_dimension) {
                Some(new_level) => EndPlatformFeature::create_end_platform(new_level, platform_origin, true),
                None => EndPlatformFeature::create_end_platform(current_level, platform_origin, true),
            }
            y_rot = Direction::West.to_y_rot();
            x_rot = 0.0;
            relatives = Relatives::DELTA.with(Relative::XRot);
            if entity.is_player() {
                spawn_pos = spawn_pos.subtract(Vec3::new(0.0, 1.0, 0.0));
            }
        } else {
            y_rot = respawn_data.yaw;
            x_rot = respawn_data.pitch;
            relatives = Relatives::DELTA.union(Relatives::ROTATION);
            if let Some(player) = entity.as_any().downcast_ref::<ServerPlayer>() {
                let config = player.respawn_config.clone();
                return Some(find_respawn_position_and_use_spawn_block(server, current_level, config.as_ref(), false, PostTeleportTransition::DO_NOTHING));
            }
            spawn_pos = adjusted_spawn_pos(server, current_level, &new_dimension, entity, spawn_block_pos);
        }
        Some(
            TeleportTransition::new(new_dimension, spawn_pos, Vec3::ZERO, y_rot, x_rot, PostTeleportTransition::PLAY_PORTAL_SOUND.then(PostTeleportTransition::PLACE_PORTAL_TICKET))
                .with_relatives(relatives),
        )
    }
}

register_block_class!("EndPortalBlock", |_| Box::new(EndPortalBlock { _unique: 0 }));
