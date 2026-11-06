//! `SetSpawnCommand` (`/spawnpoint`).
use mc_brigadier::ArgumentBuilder;
use mc_brigadier::CommandSyntaxException;
use mc_core::pos::BlockPos;
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::coordinates::{
    BlockPosArgument, Coordinates, RotationArgument, WorldCoordinates,
};
use mc_game_core::commands::arguments::selector::entity_selector::EntityRef;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::world::entity::respawn::{RespawnConfig, respawn_data_of};
use mc_game_core::{register_command, translatable};
use mc_text::Component;

register_command!("SetSpawnCommand", |ctx| {
    ctx.dispatcher.register(
        literal("spawnpoint")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .executes(|c| {
                let source = c.get_source();
                let player = source.get_player_or_exception()?;
                let target = source.with_server(|server| {
                    host::server_players(server)
                        .into_iter()
                        .find(|p| p.id == player.id)
                });
                set_spawn(
                    source,
                    target.into_iter().collect(),
                    BlockPos::containing_position(&source.get_position()),
                    &zero_rotation(),
                )
            })
            .then(
                argument("targets", EntityArgument::players())
                    .executes(|c| {
                        let source = c.get_source();
                        set_spawn(
                            source,
                            EntityArgument::get_players(c, "targets")?,
                            BlockPos::containing_position(&source.get_position()),
                            &zero_rotation(),
                        )
                    })
                    .then(
                        argument("pos", BlockPosArgument::block_pos())
                            .executes(|c| {
                                set_spawn(
                                    c.get_source(),
                                    EntityArgument::get_players(c, "targets")?,
                                    BlockPosArgument::get_spawnable_pos(c, "pos")?,
                                    &zero_rotation(),
                                )
                            })
                            .then(argument("rotation", RotationArgument::rotation()).executes(
                                |c| {
                                    set_spawn(
                                        c.get_source(),
                                        EntityArgument::get_players(c, "targets")?,
                                        BlockPosArgument::get_spawnable_pos(c, "pos")?,
                                        &RotationArgument::get_rotation(c, "rotation"),
                                    )
                                },
                            )),
                    ),
            ),
    );
});

fn zero_rotation() -> Coordinates {
    Coordinates::World(WorldCoordinates::zero_rotation())
}

fn set_spawn(
    source: &CommandSourceStack,
    targets: Vec<EntityRef>,
    pos: BlockPos,
    rotation: &Coordinates,
) -> Result<i32, CommandSyntaxException> {
    let dimension = source.get_level().clone();
    let rotation_vector = rotation.get_rotation(source);
    let yaw = mc_core::math::mth::wrap_degrees_f32(rotation_vector.y);
    let pitch = mc_core::math::mth::clamp_f32(rotation_vector.x, -90.0, 90.0);
    let respawn_data = respawn_data_of(dimension.clone(), pos, yaw, pitch);
    let config = RespawnConfig {
        respawn_data: respawn_data.clone(),
        forced: true,
    };
    for target in &targets {
        source.with_server(|server| {
            if let Some(level) = server.level_mut(&target.dimension) {
                level.with_player(target.id, |_, player| {
                    player.set_respawn_position(Some(config.clone()), false)
                });
            }
        });
    }
    // `CommandResponseTracker`: every target is tracked with value 1 (`getPlayers` guarantees at least one).
    let total = targets.len() as i32;
    source.send_success(
        || {
            let dimension_arg = Component::literal(dimension.to_string());
            match targets.as_slice() {
                [only] => {
                    let name = source.with_server(|server| {
                        host::get_entity(server, only)
                            .map_or_else(Component::empty, |e| host::entity_display_name(server, e))
                    });
                    translatable!(
                        "commands.spawnpoint.success.single",
                        pos.x(),
                        pos.y(),
                        pos.z(),
                        respawn_data.yaw,
                        respawn_data.pitch,
                        dimension_arg,
                        name
                    )
                }
                _ => translatable!(
                    "commands.spawnpoint.success.multiple",
                    pos.x(),
                    pos.y(),
                    pos.z(),
                    respawn_data.yaw,
                    respawn_data.pitch,
                    dimension_arg,
                    total
                ),
            }
        },
        true,
    );
    Ok(total)
}
