//! `WaypointCommand` (+ `WaypointArgument`).
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, HexColorArgument, IdentifierArgument, TeamColorArgument};
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::world::scores::TeamColor;
use mc_game_core::world::waypoints::{Icon, default_style};
use mc_game_core::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, SimpleCommandExceptionType};
use mc_core::ident::Identifier;
use mc_text::events::{ClickEvent, HoverEvent};
use mc_text::Component;
use std::sync::LazyLock;

/// `WaypointArgument.ERROR_NOT_A_WAYPOINT`
static ERROR_NOT_A_WAYPOINT: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.waypoint.invalid"));

register_command!("WaypointCommand", |ctx| {
    let waypoint = || argument("waypoint", EntityArgument::entity());
    ctx.dispatcher.register(
        literal("waypoint")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(literal("list").executes(|c| list_waypoints(c.get_source())))
            .then(
                literal("modify").then(
                    waypoint()
                        .then(
                            literal("color")
                                .then(argument("color", TeamColorArgument::team_color()).executes(|c| {
                                    let waypoint = get_waypoint(c)?;
                                    set_team_color(c.get_source(), &waypoint, TeamColorArgument::get_team_color(c, "color"))
                                }))
                                .then(literal("hex").then(argument("color", HexColorArgument::hex_color()).executes(|c| {
                                    let waypoint = get_waypoint(c)?;
                                    set_hex_color(c.get_source(), &waypoint, HexColorArgument::get_hex_color(c, "color"))
                                })))
                                .then(literal("reset").executes(|c| {
                                    let waypoint = get_waypoint(c)?;
                                    reset_color(c.get_source(), &waypoint)
                                })),
                        )
                        .then(
                            literal("style")
                                .then(literal("reset").executes(|c| {
                                    let waypoint = get_waypoint(c)?;
                                    set_style(c.get_source(), &waypoint, default_style())
                                }))
                                .then(literal("set").then(argument("style", IdentifierArgument::id()).executes(|c| {
                                    let waypoint = get_waypoint(c)?;
                                    set_style(c.get_source(), &waypoint, IdentifierArgument::get_id(c, "style"))
                                }))),
                        ),
                ),
            ),
    );
});

/// `WaypointArgument.getWaypoint`: the single entity must be a `WaypointTransmitter` (a `LivingEntity`).
fn get_waypoint(c: &CommandContext<CommandSourceStack>) -> Result<EntityRef, CommandSyntaxException> {
    let entity = EntityArgument::get_entity(c, "waypoint")?;
    let is_transmitter = c.get_source().with_server(|server| host::get_entity(server, &entity).is_some_and(|e| e.living().is_some()));
    if is_transmitter { Ok(entity) } else { Err(ERROR_NOT_A_WAYPOINT.create()) }
}

/// `mutateIcon`: untrack, mutate the icon, track again (in the source's level, like vanilla).
fn mutate_icon(source: &CommandSourceStack, waypoint: &EntityRef, mutate: impl FnOnce(&mut Icon)) {
    source.with_server(|server| {
        let id = waypoint.id;
        if let Some(level) = server.level_mut(source.get_level()) {
            level.with_waypoint_manager(|m, ctx| m.untrack_waypoint(ctx, id));
        }
        if let Some(living) = server.level_mut(&waypoint.dimension).and_then(|l| l.get_entity_mut(id)).and_then(|e| e.living_mut()) {
            mutate(&mut living.locator_bar_icon);
        }
        if let Some(level) = server.level_mut(source.get_level()) {
            level.with_waypoint_manager(|m, ctx| m.track_waypoint(ctx, id));
        }
    });
}

fn set_style(source: &CommandSourceStack, waypoint: &EntityRef, style: Identifier) -> Result<i32, CommandSyntaxException> {
    mutate_icon(source, waypoint, |icon| icon.style = style);
    source.send_success(|| translatable!("commands.waypoint.modify.style"), false);
    Ok(0)
}

fn set_team_color(source: &CommandSourceStack, waypoint: &EntityRef, color: mc_game_core::commands::arguments::TeamColor) -> Result<i32, CommandSyntaxException> {
    let color = TeamColor::by_name(color.get_serialized_name()).expect("team colors have the same names");
    mutate_icon(source, waypoint, |icon| icon.color = Some(color.rgb()));
    source.send_success(
        || translatable!("commands.waypoint.modify.color", Component::literal(color.get_serialized_name()).with_style_fn(|s| s.with_color(Some(color.text_color())))),
        false,
    );
    Ok(0)
}

fn set_hex_color(source: &CommandSourceStack, waypoint: &EntityRef, color: i32) -> Result<i32, CommandSyntaxException> {
    mutate_icon(source, waypoint, |icon| icon.color = Some(color));
    source.send_success(
        || translatable!("commands.waypoint.modify.color", Component::literal(format!("{:06X}", color & 0xFF_FFFF)).with_color(color)),
        false,
    );
    Ok(0)
}

fn reset_color(source: &CommandSourceStack, waypoint: &EntityRef) -> Result<i32, CommandSyntaxException> {
    mutate_icon(source, waypoint, |icon| icon.color = None);
    source.send_success(|| translatable!("commands.waypoint.modify.color.reset"), false);
    Ok(0)
}

fn list_waypoints(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    let dimension = source.get_level().to_string();
    let entries: Vec<Component> = source.with_server(|server| {
        let Some(level) = server.level(source.get_level()) else { return Vec::new() };
        level
            .waypoint_manager
            .transmitters()
            .into_iter()
            .filter_map(|id| {
                let e = level.get_entity(id)?;
                let living = e.living()?;
                let pos = e.block_position();
                let tp = format!("/execute in {dimension} run tp @s {} {} {}", pos.x(), pos.y(), pos.z());
                let scoreboard_name = e.get_scoreboard_name();
                let color = living.locator_bar_icon.color.unwrap_or(-1);
                // `getFeedbackDisplayName()`: the display name hovering as the scoreboard name, then restyled
                Some(host::entity_display_name(server, e).with_style_fn(|s| {
                    s.with_hover_event(Some(HoverEvent::ShowText(Box::new(Component::literal(scoreboard_name)))))
                        .with_click_event(Some(ClickEvent::SuggestCommand(tp)))
                        .with_hover_event(Some(HoverEvent::ShowText(Box::new(translatable!("chat.coordinates.tooltip")))))
                        .with_color_rgb(color)
                }))
            })
            .collect()
    });
    let count = source.with_server(|server| server.level(source.get_level()).map_or(0, |l| l.waypoint_manager.transmitters().len())) as i32;
    if count == 0 {
        source.send_success(|| translatable!("commands.waypoint.list.empty", dimension.clone()), false);
        return Ok(0);
    }
    let names = mc_text::util::format_list(entries, &mc_text::util::default_separator(), |c| c);
    source.send_success(|| translatable!("commands.waypoint.list.success", count, dimension.clone(), names), false);
    Ok(count)
}
