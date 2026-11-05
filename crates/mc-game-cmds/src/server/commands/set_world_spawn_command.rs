//! `SetWorldSpawnCommand`.
use mc_brigadier::ArgumentBuilder;
use mc_game_core::commands::arguments::coordinates::{BlockPosArgument, Coordinates, RotationArgument, WorldCoordinates};
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use mc_game_core::world::entity::respawn::respawn_data_of;
use mc_game_core::{register_command, translatable};
use mc_core::pos::BlockPos;

register_command!("SetWorldSpawnCommand", |ctx| {
    ctx.dispatcher.register(
        literal("setworldspawn")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .executes(|c| Ok(set_spawn(c.get_source(), BlockPos::containing_position(&c.get_source().get_position()), &Coordinates::World(WorldCoordinates::zero_rotation()))))
            .then(
                argument("pos", BlockPosArgument::block_pos())
                    .executes(|c| Ok(set_spawn(c.get_source(), BlockPosArgument::get_spawnable_pos(c, "pos")?, &Coordinates::World(WorldCoordinates::zero_rotation()))))
                    .then(argument("rotation", RotationArgument::rotation()).executes(|c| {
                        Ok(set_spawn(c.get_source(), BlockPosArgument::get_spawnable_pos(c, "pos")?, &RotationArgument::get_rotation(c, "rotation")))
                    })),
            ),
    );
});

fn set_spawn(source: &CommandSourceStack, pos: BlockPos, rotation: &Coordinates) -> i32 {
    let dimension = source.get_level().clone();
    let rotation_vector = rotation.get_rotation(source);
    let respawn_data = respawn_data_of(dimension.clone(), pos, rotation_vector.y, rotation_vector.x);
    source.with_server(|server| server.set_respawn_data(respawn_data.clone()));
    source.send_success(
        || translatable!("commands.setworldspawn.success", pos.x(), pos.y(), pos.z(), respawn_data.yaw, respawn_data.pitch, dimension.to_string()),
        true,
    );
    1
}
