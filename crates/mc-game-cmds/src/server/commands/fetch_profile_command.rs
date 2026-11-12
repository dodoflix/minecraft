//! `FetchProfileCommand`.
use mc_brigadier::arguments::StringArgumentType;
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, DynamicCommandExceptionType};
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, UuidArgument};
use mc_game_core::commands::profile_report::{
    avatar_profile, print_for_avatar, report_resolved_game_profile,
};
use mc_game_core::commands::server_access::enter_server;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::{register_command, translatable};
use mc_protocol::codec::GameProfile;
use mc_text::Component;
use std::sync::LazyLock;
use uuid::Uuid;

static NO_PROFILE: LazyLock<DynamicCommandExceptionType<Component>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|name: Component| {
        mc_game_core::commands::message::message(translatable!(
            "commands.fetchprofile.no_profile",
            name
        ))
    })
});

register_command!("FetchProfileCommand", |ctx| {
    ctx.dispatcher.register(
        literal("fetchprofile")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(literal("name").then(
                argument("name", StringArgumentType::greedy_string()).executes(|c| {
                    Ok(resolve_name(
                        c.get_source(),
                        StringArgumentType::get_string(c, "name").to_owned(),
                    ))
                }),
            ))
            .then(
                literal("id").then(
                    argument("id", UuidArgument::uuid()).executes(|c| {
                        Ok(resolve_id(c.get_source(), UuidArgument::get_uuid(c, "id")))
                    }),
                ),
            )
            .then(
                literal("entity").then(argument("entity", EntityArgument::entity()).executes(
                    |c| print_for_entity(c.get_source(), EntityArgument::get_entity(c, "entity")?),
                )),
            ),
    );
});

/// `Util.nonCriticalIoPool().execute(lookup)` then `server.execute(report)`.
fn run_lookup(
    source: &CommandSourceStack,
    lookup: impl FnOnce() -> Option<GameProfile> + Send + 'static,
    report: impl FnOnce(&CommandSourceStack, Option<GameProfile>) + Send + 'static,
) {
    let source = source.clone();
    let shared = source.with_server(|server| server.shared.clone());
    let spawned = std::thread::Builder::new()
        .name("Worker-Main-fetchprofile".into())
        .spawn(move || {
            let result = lookup();
            shared.execute(move |server| enter_server(server, || report(&source, result)));
        });
    if let Err(e) = spawned {
        log::error!("Failed to start the profile lookup thread: {e}");
    }
}

fn resolve_name(source: &CommandSourceStack, name: String) -> i32 {
    let resolver = source.with_server(|server| server.profile_resolver.clone());
    let lookup_name = name.clone();
    run_lookup(
        source,
        move || resolver.fetch_by_name(&lookup_name),
        move |source, result| {
            let name_component = Component::literal(name);
            match result {
                Some(profile) => report_resolved_game_profile(
                    source,
                    &profile,
                    "commands.fetchprofile.name.success",
                    name_component,
                ),
                None => source.send_failure(translatable!(
                    "commands.fetchprofile.name.failure",
                    name_component
                )),
            }
        },
    );
    1
}

fn resolve_id(source: &CommandSourceStack, id: Uuid) -> i32 {
    let resolver = source.with_server(|server| server.profile_resolver.clone());
    run_lookup(
        source,
        move || resolver.fetch_by_id(id),
        move |source, result| {
            let id_component = Component::literal(id.to_string());
            match result {
                Some(profile) => report_resolved_game_profile(
                    source,
                    &profile,
                    "commands.fetchprofile.id.success",
                    id_component,
                ),
                None => source.send_failure(translatable!(
                    "commands.fetchprofile.id.failure",
                    id_component
                )),
            }
        },
    );
    1
}

fn print_for_entity(
    source: &CommandSourceStack,
    entity: EntityRef,
) -> Result<i32, CommandSyntaxException> {
    let (profile, display_name) = source.with_server(|server| {
        let e = host::get_entity(server, &entity).expect("selected entity exists");
        (avatar_profile(e), host::entity_display_name(server, e))
    });
    match profile {
        Some(profile) => {
            print_for_avatar(source, display_name, &profile);
            Ok(1)
        }
        None => Err(NO_PROFILE.create(display_name)),
    }
}
