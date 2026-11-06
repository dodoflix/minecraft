//! `FetchProfileCommand`.
use mc_brigadier::arguments::StringArgumentType;
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, DynamicCommandExceptionType};
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, UuidArgument};
use mc_game_core::commands::server_access::enter_server;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::{register_command, translatable};
use mc_protocol::codec::{Either, GameProfile};
use mc_protocol::components::{PlayerSkinPatch, ResolvableProfile};
use mc_text::component::ObjectInfo;
use mc_text::profile::{
    ProfileContents, ProfileProperty, ResolvableProfile as TextProfile, SkinPatch,
};
use mc_text::util::{format_list, space, wrap_in_square_brackets};
use mc_text::{ChatFormatting, ClickEvent, Component};
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

/// `ResolvableProfile.createResolved(gameProfile)` as the chat component's profile type.
fn text_profile(profile: &GameProfile) -> TextProfile {
    TextProfile {
        contents: ProfileContents::Full {
            id: profile.id.as_u128(),
            name: profile.name.clone(),
            properties: profile
                .properties
                .iter()
                .map(|p| ProfileProperty {
                    name: p.name.clone(),
                    value: p.value.clone(),
                    signature: p.signature.clone(),
                })
                .collect(),
        },
        skin_patch: SkinPatch::default(),
    }
}

fn report_resolved_profile(
    source: &CommandSourceStack,
    profile: &GameProfile,
    message_id: &'static str,
    argument: Component,
) {
    // `ResolvableProfile.CODEC.encodeStart(NbtOps.INSTANCE, profile)`
    let resolved = ResolvableProfile {
        contents: Either::Left(profile.clone()),
        skin_patch: PlayerSkinPatch::default(),
    };
    let encoded_profile = match ResolvableProfile::codec()
        .encode_start(&mc_dfu::NBT, &resolved)
        .into_result()
        .and_then(|d| d.to_tag())
    {
        Ok(tag) => tag.to_string(),
        Err(error) => {
            return source.send_failure(translatable!(
                "commands.fetchprofile.failed_to_serialize",
                error
            ));
        }
    };
    let head = Component::object(
        ObjectInfo::PlayerSprite {
            player: Box::new(text_profile(profile)),
            hat: true,
        },
        None,
    );
    // `ComponentSerialization.CODEC.encodeStart(NbtOps.INSTANCE, headComponent)` (before the white style is applied)
    let encoded_component = match mc_text::codec::encode_component::<mc_nbt::Tag>(&head) {
        Ok(tag) => tag.to_string(),
        Err(error) => {
            return source.send_failure(translatable!(
                "commands.fetchprofile.failed_to_serialize",
                error.0
            ));
        }
    };
    source.send_success(
        || {
            let click = |key: &'static str, event: ClickEvent| {
                translatable!(key).with_style_fn(|s| s.with_click_event(Some(event)))
            };
            let entries = vec![
                click(
                    "commands.fetchprofile.copy_component",
                    ClickEvent::CopyToClipboard(encoded_profile.clone()),
                ),
                click(
                    "commands.fetchprofile.give_item",
                    ClickEvent::RunCommand(format!(
                        "give @s minecraft:player_head[profile={encoded_profile}]"
                    )),
                ),
                click(
                    "commands.fetchprofile.summon_mannequin",
                    ClickEvent::RunCommand(format!(
                        "summon minecraft:mannequin ~ ~ ~ {{profile:{encoded_profile}}}"
                    )),
                ),
                translatable!(
                    "commands.fetchprofile.copy_text",
                    head.clone().with_format(ChatFormatting::White)
                )
                .with_style_fn(|s| {
                    s.with_click_event(Some(ClickEvent::CopyToClipboard(encoded_component.clone())))
                }),
            ];
            let clickable = format_list(entries, &space(), |c| {
                wrap_in_square_brackets(c.with_format(ChatFormatting::Green))
            });
            translatable!(message_id, argument.clone(), clickable)
        },
        false,
    );
}

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
                Some(profile) => report_resolved_profile(
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
                Some(profile) => report_resolved_profile(
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
    // `entity instanceof Avatar` (players; the mannequin entity does not exist in this server yet)
    let (profile, display_name) = source.with_server(|server| {
        let e = host::get_entity(server, &entity).expect("selected entity exists");
        let profile = e
            .as_any()
            .downcast_ref::<ServerPlayer>()
            .map(|p| p.player.game_profile.clone());
        (profile, host::entity_display_name(server, e))
    });
    match profile {
        Some(profile) => {
            report_resolved_profile(
                source,
                &profile,
                "commands.fetchprofile.entity.success",
                display_name,
            );
            Ok(1)
        }
        None => Err(NO_PROFILE.create(display_name)),
    }
}
