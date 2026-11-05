//! `DamageCommand`.
use mc_game_core::commands::arguments::coordinates::Vec3Argument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::resource_arguments::ResourceArgument;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::registry::typed::damage_type::DamageType;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::{register_command, translatable};
use mc_brigadier::arguments::{FloatArgumentType, float_arg_min};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, SimpleCommandExceptionType};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use std::sync::{Arc, LazyLock};

static ERROR_INVULNERABLE: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.damage.invulnerable"));

/// The `DamageSource` constructor the command picked: `generic()` or `new DamageSource(type[, position | direct[, causing]])`.
enum Spec {
    Generic,
    Type(Identifier),
    At(Identifier, Vec3),
    By(Identifier, EntityRef),
    From(Identifier, EntityRef, EntityRef),
}

register_command!("DamageCommand", |ctx| {
    fn run(c: &CommandContext<CommandSourceStack>, spec: impl FnOnce(&CommandContext<CommandSourceStack>) -> Result<Spec, CommandSyntaxException>) -> Result<i32, CommandSyntaxException> {
        let target = EntityArgument::get_entity(c, "target")?;
        let amount = FloatArgumentType::get_float(c, "amount");
        damage(c.get_source(), target, amount, spec(c)?)
    }
    fn damage_type(c: &CommandContext<CommandSourceStack>) -> Result<Identifier, CommandSyntaxException> {
        Ok(ResourceArgument::get_resource(c, "damageType", "damage_type")?.id)
    }
    ctx.dispatcher.register(
        literal("damage").requires(has_permission(LEVEL_GAMEMASTERS)).then(
            argument("target", EntityArgument::entity()).then(
                argument("amount", float_arg_min(0.0))
                    .executes(|c| run(c, |_| Ok(Spec::Generic)))
                    .then(
                        argument("damageType", ResourceArgument::resource(ctx.build_context, "damage_type"))
                            .executes(|c| run(c, |c| Ok(Spec::Type(damage_type(c)?))))
                            .then(literal("at").then(
                                argument("location", Vec3Argument::vec3()).executes(|c| run(c, |c| Ok(Spec::At(damage_type(c)?, Vec3Argument::get_vec3(c, "location"))))),
                            ))
                            .then(
                                literal("by").then(
                                    argument("entity", EntityArgument::entity())
                                        .executes(|c| run(c, |c| Ok(Spec::By(damage_type(c)?, EntityArgument::get_entity(c, "entity")?))))
                                        .then(literal("from").then(argument("cause", EntityArgument::entity()).executes(|c| {
                                            run(c, |c| Ok(Spec::From(damage_type(c)?, EntityArgument::get_entity(c, "entity")?, EntityArgument::get_entity(c, "cause")?)))
                                        }))),
                                ),
                            ),
                    ),
            ),
        ),
    );
});

/// Builds the source against the target's level (the registries are the same in every level).
fn build_source(server: &mc_game_core::server::MinecraftServer, target: &EntityRef, spec: &Spec) -> Option<DamageSource> {
    let level = server.level(&target.dimension)?;
    let entity = |reference: &EntityRef| host::get_entity(server, reference);
    let (id, direct, causing, position) = match spec {
        Spec::Generic => return Some(level.damage_sources().generic()),
        Spec::Type(id) => (id, None, None, None),
        Spec::At(id, position) => (id, None, None, Some(*position)),
        Spec::By(id, by) => (id, entity(by), entity(by), None),
        Spec::From(id, by, cause) => (id, entity(by), entity(cause), None),
    };
    let data = level.registries.get::<DamageType>(mc_game_core::registry::keys::DAMAGE_TYPE, id)?.clone();
    Some(DamageSource::new(id.clone(), Arc::new(data), direct, causing, position))
}

fn damage(source: &CommandSourceStack, target: EntityRef, amount: f32, spec: Spec) -> Result<i32, CommandSyntaxException> {
    let hurt = source.with_server(|server| {
        let damage_source = build_source(server, &target, &spec)?;
        server.level_mut(&target.dimension)?.with_entity(target.id, |level, entity| entity.hurt_server(level, &damage_source, amount))
    });
    if hurt == Some(true) {
        source.send_success(|| translatable!("commands.damage.success", amount, host::display_name_of(source, &target)), true);
        Ok(1)
    } else {
        Err(ERROR_INVULNERABLE.create())
    }
}
