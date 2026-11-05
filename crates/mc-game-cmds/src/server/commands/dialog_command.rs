//! `DialogCommand`.
use super::command_response_tracker::{CommandResponseTracker, ElementType};
use crate::commands::arguments::resource_arguments::{DialogArgument, ResourceOrId};
use crate::commands::arguments::selector::EntityRef;
use crate::commands::arguments::EntityArgument;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use crate::world::entity::ServerPlayer;
use crate::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException};
use mc_protocol::components::Holder;
use mc_protocol::dialog::Dialog;
use mc_protocol::packets::play::ClientboundClearDialogPacket;
use mc_text::Component;

register_command!("DialogCommand", |ctx| {
    ctx.dispatcher.register(
        literal("dialog")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(literal("show").then(argument("targets", EntityArgument::players()).then(
                argument("dialog", DialogArgument::dialog(ctx.build_context)).executes(|c| {
                    show_dialog(c.get_source(), &EntityArgument::get_players(c, "targets")?, DialogArgument::get_dialog(c, "dialog"))
                }),
            )))
            .then(literal("clear").then(
                argument("targets", EntityArgument::players()).executes(|c| clear_dialog(c.get_source(), &EntityArgument::get_players(c, "targets")?)),
            )),
    );
});

/// `player.getDisplayName()`
fn display_name(source: &CommandSourceStack, player: &EntityRef) -> Component {
    source.with_server(|server| host::get_entity(server, player).map_or_else(Component::empty, |e| host::entity_display_name(server, e)))
}

/// Runs `f` on the player entity `player`.
fn with_player(source: &CommandSourceStack, player: &EntityRef, f: impl FnOnce(&ServerPlayer)) {
    source.with_server(|server| {
        if let Some(target) = host::get_entity(server, player).and_then(|e| e.as_any().downcast_ref::<ServerPlayer>()) {
            f(target);
        }
    });
}

fn show_dialog(source: &CommandSourceStack, targets: &[EntityRef], dialog: ResourceOrId) -> Result<i32, CommandSyntaxException> {
    // the argument validated an inline dialog against `Dialog.DIRECT_CODEC` already
    let holder: Option<Holder<Dialog>> = match dialog {
        ResourceOrId::Reference(key) => Some(Holder::Reference(key.id)),
        ResourceOrId::Inline(tag) => Dialog::from_inline_tag(&tag).ok().map(Holder::Direct),
    };
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for target in targets {
        if let Some(holder) = &holder {
            with_player(source, target, |p| p.open_dialog(holder));
        }
        tracker.track(target, 1);
    }
    tracker.send_feedback(
        source,
        true,
        ElementType::NonZero,
        None::<fn() -> CommandSyntaxException>,
        |player, _| translatable!("commands.dialog.show.single", display_name(source, player)),
        |count, _| translatable!("commands.dialog.show.multiple", count),
    )
}

fn clear_dialog(source: &CommandSourceStack, targets: &[EntityRef]) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for target in targets {
        with_player(source, target, |p| p.send(&ClientboundClearDialogPacket));
        tracker.track(target, 1);
    }
    tracker.send_feedback(
        source,
        true,
        ElementType::NonZero,
        None::<fn() -> CommandSyntaxException>,
        |player, _| translatable!("commands.dialog.clear.single", display_name(source, player)),
        |count, _| translatable!("commands.dialog.clear.multiple", count),
    )
}
