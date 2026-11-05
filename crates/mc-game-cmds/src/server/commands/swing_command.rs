//! `SwingCommand`.
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, SwingAnimationArgument, TimeArgument};
use mc_game_core::commands::arguments::SwingAnimationType as ArgumentType;
use super::command_response_tracker::CommandResponseTracker;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::world::entity::player::item_use::SWING_ANIMATION_DEFAULT;
use mc_game_core::world::item::InteractionHand;
use mc_game_core::{register_command, translatable};
use mc_brigadier::arguments::IntegerArgumentType;
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, LiteralArgumentBuilder, SimpleCommandExceptionType};
use mc_protocol::packets::play::{SwingAnimation, SwingAnimationType};
use std::sync::LazyLock;

static ERROR_NO_LIVING_ENTITY: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.swing.failed.notliving"));

register_command!("SwingCommand", |ctx| {
    ctx.dispatcher.register(
        literal("swing")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .executes(|c| swing(c.get_source(), vec![host::source_entity_ref(c.get_source())?], InteractionHand::MainHand, SWING_ANIMATION_DEFAULT))
            .then(
                argument("targets", EntityArgument::entities())
                    .executes(|c| swing(c.get_source(), EntityArgument::get_entities(c, "targets")?, InteractionHand::MainHand, SWING_ANIMATION_DEFAULT))
                    .then(hand_swing("mainhand", InteractionHand::MainHand))
                    .then(hand_swing("offhand", InteractionHand::OffHand)),
            ),
    );
});

fn hand_swing(name: &'static str, hand: InteractionHand) -> LiteralArgumentBuilder<CommandSourceStack> {
    fn animation(c: &mc_brigadier::CommandContext<CommandSourceStack>, duration: i32) -> SwingAnimation {
        let kind = match SwingAnimationArgument::get_swing_animation_type(c, "animation") {
            ArgumentType::None => SwingAnimationType::None,
            ArgumentType::Whack => SwingAnimationType::Whack,
            ArgumentType::Stab => SwingAnimationType::Stab,
        };
        SwingAnimation { kind, duration }
    }
    literal(name)
        .executes(move |c| swing(c.get_source(), EntityArgument::get_entities(c, "targets")?, hand, SWING_ANIMATION_DEFAULT))
        .then(
            argument("animation", SwingAnimationArgument::swing_animation_type())
                .executes(move |c| swing(c.get_source(), EntityArgument::get_entities(c, "targets")?, hand, animation(c, SWING_ANIMATION_DEFAULT.duration)))
                .then(argument("duration", TimeArgument::time_min(1)).executes(move |c| {
                    swing(c.get_source(), EntityArgument::get_entities(c, "targets")?, hand, animation(c, IntegerArgumentType::get_integer(c, "duration")))
                })),
        )
}

fn swing(source: &CommandSourceStack, targets: Vec<EntityRef>, hand: InteractionHand, animation: SwingAnimation) -> Result<i32, CommandSyntaxException> {
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for entity in &targets {
        let swung = source.with_server(|server| {
            let level = server.level_mut(&entity.dimension)?;
            level.with_entity(entity.id, |level, e| {
                e.living()?;
                mc_game_core::world::entity::living_entity::swing::swing(e, level, hand, animation, true);
                Some(())
            })?
        });
        if swung.is_some() {
            tracker.track_one(entity);
        }
    }
    tracker.send_feedback_default(
        source,
        true,
        Some(&|| ERROR_NO_LIVING_ENTITY.create()),
        |entity, _| translatable!("commands.swing.success.single", host::display_name_of(source, entity)),
        |entity_count, _| translatable!("commands.swing.success.multiple", entity_count),
    )
}
