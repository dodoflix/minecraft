//! `RotateCommand`.
use super::look_at::LookAt;
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::coordinates::{Coordinates, RotationArgument, Vec3Argument};
use mc_game_core::commands::arguments::entity_anchor_argument::{Anchor, EntityAnchorArgument};
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException};

register_command!("RotateCommand", |ctx| {
    ctx.dispatcher.register(
        literal("rotate").requires(has_permission(LEVEL_GAMEMASTERS)).then(
            argument("target", EntityArgument::entity())
                .then(
                    argument("rotation", RotationArgument::rotation())
                        .executes(|c| rotate(c.get_source(), EntityArgument::get_entity(c, "target")?, RotationArgument::get_rotation(c, "rotation"))),
                )
                .then(
                    literal("facing")
                        .then(
                            literal("entity").then(
                                argument("facingEntity", EntityArgument::entity())
                                    .executes(|c| {
                                        facing(c.get_source(), EntityArgument::get_entity(c, "target")?, LookAt::Entity(EntityArgument::get_entity(c, "facingEntity")?, Anchor::Feet))
                                    })
                                    .then(argument("facingAnchor", EntityAnchorArgument::anchor()).executes(|c| {
                                        facing(
                                            c.get_source(),
                                            EntityArgument::get_entity(c, "target")?,
                                            LookAt::Entity(EntityArgument::get_entity(c, "facingEntity")?, EntityAnchorArgument::get_anchor(c, "facingAnchor")),
                                        )
                                    })),
                            ),
                        )
                        .then(argument("facingLocation", Vec3Argument::vec3()).executes(|c| {
                            facing(c.get_source(), EntityArgument::get_entity(c, "target")?, LookAt::Position(Vec3Argument::get_vec3(c, "facingLocation")))
                        })),
                ),
        ),
    );
});

fn rotate(source: &CommandSourceStack, entity: EntityRef, rotation: Coordinates) -> Result<i32, CommandSyntaxException> {
    let rot = rotation.get_rotation(source);
    let (current_y_rot, current_x_rot) =
        source.with_server(|server| host::get_entity(server, &entity).map_or((0.0, 0.0), |e| (e.base().y_rot(), e.base().x_rot())));
    let relative_or_absolute_y_rot = if rotation.is_y_relative() { rot.y - current_y_rot } else { rot.y };
    let relative_or_absolute_x_rot = if rotation.is_x_relative() { rot.x - current_x_rot } else { rot.x };
    source.with_server(|server| {
        host::force_set_rotation(server, &entity, relative_or_absolute_y_rot, rotation.is_y_relative(), relative_or_absolute_x_rot, rotation.is_x_relative())
    });
    source.send_success(|| translatable!("commands.rotate.success", host::display_name_of(source, &entity)), true);
    Ok(1)
}

fn facing(source: &CommandSourceStack, entity: EntityRef, facing: LookAt) -> Result<i32, CommandSyntaxException> {
    facing.perform(source, &entity);
    source.send_success(|| translatable!("commands.rotate.success", host::display_name_of(source, &entity)), true);
    Ok(1)
}
