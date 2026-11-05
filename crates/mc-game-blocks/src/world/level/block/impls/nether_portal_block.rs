//! `net.minecraft.world.level.block.NetherPortalBlock`.
use super::prelude::*;
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::players::dimension_change::{PostTeleportTransition, TeleportTransition};
use crate::util::block_util::{self, FoundRectangle};
use crate::world::entity::Entity;
use crate::world::entity::portal::{Portal, PortalTransition};
use crate::world::level::block::BlockStateExt;
use crate::world::level::portal::PortalShape;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_protocol::packets::play::Relatives;

/// The marker byte keeps the type non-zero-sized: `PortalProcessor.isSamePortal` compares portal addresses, and distinct
/// zero-sized statics may share one.
pub struct NetherPortalBlock(u8);

/// The `Portal` instance entities hold in their `PortalProcessor`.
pub static NETHER_PORTAL: NetherPortalBlock = NetherPortalBlock(0);

impl BlockBehaviour for NetherPortalBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        let update_axis = direction.get_axis();
        let axis = state.get(props::HORIZONTAL_AXIS);
        let wrong_axis = axis != update_axis && update_axis.is_horizontal();
        if !wrong_axis && !neighbour_state.is_block(state.block()) && !PortalShape::find_any_shape(&*level, pos, axis).is_complete() {
            air()
        } else {
            state
        }
    });
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        use mc_world::level_storage::Difficulty;
        let _ = state;
        if level.is_spawning_monsters() && level.difficulty() != Difficulty::Peaceful && level.attribute_bool(crate::world::attribute::attributes::NETHER_PORTAL_SPAWNS_PIGLINS, pos) && {
            let difficulty_id = level.difficulty().get_id();
            level.random().next_int_bounded(2000) < difficulty_id
        } && level.chunk_source.chunk_map.any_player_close_enough_for_spawning(mc_core::pos::ChunkPos::containing(&pos))
        {
            let mut pos = pos;
            while level.get_block_state(pos).is_block(Block::NETHER_PORTAL) {
                pos = pos.below();
            }

            if level.get_block_state(pos).is_valid_spawn(&*level, pos, mc_data::EntityType::ZOMBIFIED_PIGLIN) {
                if let Some(id) = spawn_zombified_piglin(level, pos.above()) {
                    let vehicle = level.with_entity(id, |level, entity| {
                        entity.set_portal_cooldown(level);
                        entity.base().vehicle
                    });
                    if let Some(vehicle) = vehicle.flatten() {
                        level.with_entity(vehicle, |level, vehicle| vehicle.set_portal_cooldown(level));
                    }
                }
            }
        }
    }
    fn entity_inside(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &mut dyn Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        if entity.can_use_portal(false) {
            entity.set_as_inside_portal(level, &NETHER_PORTAL, pos);
        }
    }
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> crate::world::item::ItemStack {
        crate::world::item::ItemStack::empty()
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        rotate_pillar_horizontal(state, rotation)
    }
}

/// `EntityTypes.ZOMBIFIED_PIGLIN.spawn(level, pos, STRUCTURE)`.
fn spawn_zombified_piglin(level: &mut ServerLevel, pos: BlockPos) -> Option<crate::world::entity::EntityId> {
    use crate::world::entity::factory::{self, EntitySpawnRequest};
    use mc_core::random::RandomSource;
    let mut entity = factory::create(mc_data::EntityType::ZOMBIFIED_PIGLIN, level, EntitySpawnRequest { reason: mc_data::enums::EntitySpawnReason::Structure, ignore_checks: false })?;
    let y_rot = mc_core::math::mth::wrap_degrees_f32(level.random().next_float() * 360.0);
    entity.snap_to(f64::from(pos.x()) + 0.5, f64::from(pos.y()), f64::from(pos.z()) + 0.5, y_rot, 0.0);
    let id = entity.id();
    level.add_fresh_entity(entity).then_some(id)
}

fn nether_key() -> Identifier {
    Identifier::with_default_namespace("the_nether").expect("valid id")
}

impl Portal for NetherPortalBlock {
    /// `getPortalTransitionTime(level, entity)`.
    fn get_portal_transition_time(&self, level: &ServerLevel, entity: &dyn Entity) -> i32 {
        match entity.player() {
            Some(player) => {
                let delay = if player.abilities.invulnerable { level.players_nether_portal_creative_delay() } else { level.players_nether_portal_default_delay() };
                delay.max(0)
            }
            None => 0,
        }
    }

    /// `getPortalDestination(currentLevel, entity, portalEntryPos)`.
    fn get_portal_destination(&self, server: &mut MinecraftServer, current_level: &mut ServerLevel, entity: &mut dyn Entity, portal_entry_pos: BlockPos) -> Option<TeleportTransition> {
        let new_dimension = if current_level.dimension == nether_key() { crate::server::world_dimensions::overworld_key() } else { nether_key() };
        let new_level = server.level_mut(&new_dimension)?;
        let to_nether = new_level.dimension == nether_key();
        let teleportation_scale = current_level.teleportation_scale_to(new_level);
        let approximate_exit_pos = new_level.world_border().clamp_to_bounds(entity.base().x() * teleportation_scale, entity.base().y(), entity.base().z() * teleportation_scale);
        get_exit_portal(new_level, current_level, entity, portal_entry_pos, approximate_exit_pos, to_nether)
    }

    fn get_local_transition(&self) -> PortalTransition {
        PortalTransition::Confusion
    }
}

/// `NetherPortalBlock.getExitPortal`.
fn get_exit_portal(new_level: &mut ServerLevel, current_level: &ServerLevel, entity: &dyn Entity, portal_entry_pos: BlockPos, approximate_exit_pos: BlockPos, to_nether: bool) -> Option<TeleportTransition> {
    let exit_portal_pos = new_level.get_portal_forcer().find_closest_portal_position(approximate_exit_pos, to_nether);
    let exit_portal: FoundRectangle;
    let post: PostTeleportTransition;
    if let Some(pos) = exit_portal_pos {
        let mut forcer = new_level.get_portal_forcer();
        let portal_state = forcer.block_state(pos);
        exit_portal = block_util::get_largest_rectangle_around(pos, portal_state.get(props::HORIZONTAL_AXIS), 21, Axis::Y, 21, &mut |block_pos| forcer.block_state(block_pos) == portal_state);
        post = PostTeleportTransition::PLAY_PORTAL_SOUND.then(PostTeleportTransition::place_portal_ticket_at(pos));
    } else {
        if entity.is_spectator() {
            return None;
        }

        let source_portal_axis = current_level.get_block_state(portal_entry_pos).try_get(props::HORIZONTAL_AXIS).unwrap_or(Axis::X);
        let Some(created_exit) = new_level.get_portal_forcer().create_portal(approximate_exit_pos, source_portal_axis) else {
            log::error!("Unable to create a portal, likely target out of worldborder");
            return None;
        };

        exit_portal = created_exit;
        post = PostTeleportTransition::PLAY_PORTAL_SOUND.then(PostTeleportTransition::PLACE_PORTAL_TICKET);
    }

    Some(get_dimension_transition_from_exit(entity, current_level, portal_entry_pos, &exit_portal, new_level, post))
}

/// `NetherPortalBlock.getDimensionTransitionFromExit`.
fn get_dimension_transition_from_exit(
    entity: &dyn Entity,
    current_level: &ServerLevel,
    portal_entry_pos: BlockPos,
    exit_portal: &FoundRectangle,
    new_level: &mut ServerLevel,
    post_teleport_transition: PostTeleportTransition,
) -> TeleportTransition {
    let block_state = current_level.get_block_state(portal_entry_pos);
    let (axis, offset) = if block_state.has_property(props::HORIZONTAL_AXIS) {
        let axis = block_state.get(props::HORIZONTAL_AXIS);
        let portal_area = block_util::get_largest_rectangle_around(portal_entry_pos, axis, 21, Axis::Y, 21, &mut |pos| current_level.get_block_state(pos) == block_state);
        (axis, entity.get_relative_portal_position(axis, &portal_area))
    } else {
        (Axis::X, Vec3::new(0.5, 0.0, 0.0))
    };

    create_dimension_transition(new_level, exit_portal, axis, offset, entity, post_teleport_transition)
}

/// `NetherPortalBlock.createDimensionTransition`.
fn create_dimension_transition(
    new_level: &mut ServerLevel,
    found_rectangle: &FoundRectangle,
    portal_axis: Axis,
    offset: Vec3,
    entity: &dyn Entity,
    post_teleport_transition: PostTeleportTransition,
) -> TeleportTransition {
    let bottom_left = found_rectangle.min_corner;
    let block_state = new_level.get_block_state(bottom_left);
    let axis = block_state.try_get(props::HORIZONTAL_AXIS).unwrap_or(Axis::X);
    let width = f64::from(found_rectangle.axis1_size);
    let height = f64::from(found_rectangle.axis2_size);
    let dimensions = entity.get_dimensions(entity.base().get_pose());
    let output_rotation = if portal_axis == axis { 0 } else { 90 };
    let offset_right = f64::from(dimensions.width) / 2.0 + (width - f64::from(dimensions.width)) * offset.x;
    let offset_up = (height - f64::from(dimensions.height)) * offset.y;
    let offset_forward = 0.5 + offset.z;
    let x_aligned = axis == Axis::X;
    let target_pos = Vec3::new(
        f64::from(bottom_left.x()) + if x_aligned { offset_right } else { offset_forward },
        f64::from(bottom_left.y()) + offset_up,
        f64::from(bottom_left.z()) + if x_aligned { offset_forward } else { offset_right },
    );
    let collision_free_pos = PortalShape::find_collision_free_position(target_pos, new_level, entity, &dimensions);
    TeleportTransition::new(new_level.dimension.clone(), collision_free_pos, Vec3::ZERO, output_rotation as f32, 0.0, post_teleport_transition)
        .with_relatives(Relatives::DELTA.union(Relatives::ROTATION))
}

/// `NetherPortalBlock.rotate` (same axis swap as `RotatedPillarBlock.rotatePillar`, on the `axis` property of the portal).
fn rotate_pillar_horizontal(state: BlockState, rotation: Rotation) -> BlockState {
    match rotation {
        Rotation::Counterclockwise90 | Rotation::Clockwise90 => match state.get(props::HORIZONTAL_AXIS) {
            Axis::X => state.set(props::HORIZONTAL_AXIS, Axis::Z),
            Axis::Z => state.set(props::HORIZONTAL_AXIS, Axis::X),
            _ => state,
        },
        _ => state,
    }
}

register_block_class!("NetherPortalBlock", |_| Box::new(NetherPortalBlock(0)));
