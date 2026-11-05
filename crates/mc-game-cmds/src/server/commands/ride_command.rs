//! `RideCommand`.
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::message::{message, simple_translatable};
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, Dynamic2CommandExceptionType, DynamicCommandExceptionType, SimpleCommandExceptionType};
use mc_data::EntityType;
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_NOT_RIDING: LazyLock<DynamicCommandExceptionType<Component>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|entity: Component| message(translatable!("commands.ride.not_riding", entity))));
static ERROR_ALREADY_RIDING: LazyLock<Dynamic2CommandExceptionType<Component, Component>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|entity: Component, vehicle: Component| message(translatable!("commands.ride.already_riding", entity, vehicle)))
});
static ERROR_MOUNT_FAILED: LazyLock<Dynamic2CommandExceptionType<Component, Component>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|entity: Component, vehicle: Component| message(translatable!("commands.ride.mount.failure.generic", entity, vehicle)))
});
static ERROR_MOUNTING_PLAYER: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.ride.mount.failure.cant_ride_players"));
static ERROR_MOUNTING_LOOP: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.ride.mount.failure.loop"));
static ERROR_WRONG_DIMENSION: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.ride.mount.failure.wrong_dimension"));

register_command!("RideCommand", |ctx| {
    ctx.dispatcher.register(
        literal("ride").requires(has_permission(LEVEL_GAMEMASTERS)).then(
            argument("target", EntityArgument::entity())
                .then(literal("mount").then(
                    argument("vehicle", EntityArgument::entity()).executes(|c| mount(c.get_source(), EntityArgument::get_entity(c, "target")?, EntityArgument::get_entity(c, "vehicle")?)),
                ))
                .then(literal("dismount").executes(|c| dismount(c.get_source(), EntityArgument::get_entity(c, "target")?))),
        ),
    );
});

/// `target.getVehicle()` (a vehicle is always in the passenger's level).
fn vehicle_of(source: &CommandSourceStack, target: &EntityRef) -> Option<EntityRef> {
    source.with_server(|server| {
        let vehicle = host::get_entity(server, target)?.base().vehicle?;
        Some(EntityRef { dimension: target.dimension.clone(), id: vehicle })
    })
}

fn mount(source: &CommandSourceStack, target: EntityRef, vehicle: EntityRef) -> Result<i32, CommandSyntaxException> {
    if let Some(current_vehicle) = vehicle_of(source, &target) {
        return Err(ERROR_ALREADY_RIDING.create(host::display_name_of(source, &target), host::display_name_of(source, &current_vehicle)));
    }
    if source.with_server(|server| host::get_entity(server, &vehicle).is_some_and(|v| v.get_type() == EntityType::PLAYER)) {
        return Err(ERROR_MOUNTING_PLAYER.create());
    }
    let loops = source.with_server(|server| {
        let level = server.level(&target.dimension)?;
        Some(host::get_entity(server, &target)?.get_self_and_passengers(level).contains(&vehicle.id))
    });
    if loops == Some(true) {
        return Err(ERROR_MOUNTING_LOOP.create());
    }
    if target.dimension != vehicle.dimension {
        return Err(ERROR_WRONG_DIMENSION.create());
    }
    let mounted = source.with_server(|server| {
        server.level_mut(&target.dimension)?.with_entity(target.id, |level, entity| level.start_riding(entity, vehicle.id, true, true))
    });
    if mounted != Some(true) {
        return Err(ERROR_MOUNT_FAILED.create(host::display_name_of(source, &target), host::display_name_of(source, &vehicle)));
    }
    source.send_success(|| translatable!("commands.ride.mount.success", host::display_name_of(source, &target), host::display_name_of(source, &vehicle)), true);
    Ok(1)
}

fn dismount(source: &CommandSourceStack, target: EntityRef) -> Result<i32, CommandSyntaxException> {
    let Some(vehicle) = vehicle_of(source, &target) else {
        return Err(ERROR_NOT_RIDING.create(host::display_name_of(source, &target)));
    };
    source.with_server(|server| {
        if let Some(level) = server.level_mut(&target.dimension) {
            level.with_entity(target.id, |level, entity| level.stop_riding(entity));
        }
    });
    source.send_success(|| translatable!("commands.ride.dismount.success", host::display_name_of(source, &target), host::display_name_of(source, &vehicle)), true);
    Ok(1)
}
