//! `RecipeCommand`.
use super::command_response_tracker::{CommandResponseTracker, ElementType, display_name};
use crate::commands::arguments::selector::EntityRef;
use crate::commands::arguments::EntityArgument;
use crate::commands::arguments::resource_arguments::ResourceKeyArgument;
use crate::commands::host;
use crate::commands::message::simple_translatable;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, SimpleCommandExceptionType};
use mc_core::ident::Identifier;
use std::sync::LazyLock;

static ERROR_GIVE_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.recipe.give.failed"));
static ERROR_TAKE_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.recipe.take.failed"));

#[derive(Clone, Copy)]
enum Action {
    Give,
    Take,
}

register_command!("RecipeCommand", |ctx| {
    fn branch(action: Action, name: &'static str) -> mc_brigadier::LiteralArgumentBuilder<CommandSourceStack> {
        literal(name).then(
            argument("targets", EntityArgument::players())
                .then(argument("recipe", ResourceKeyArgument::key_with_filter("recipe", |id| host_learnable_filter(id))).executes(
                    move |c| {
                        let players = EntityArgument::get_players(c, "targets")?;
                        perform(c.get_source(), &players, action, &[ResourceKeyArgument::get_recipe(c, "recipe")?])
                    },
                ))
                .then(literal("*").executes(move |c| {
                    let players = EntityArgument::get_players(c, "targets")?;
                    let recipes = c.get_source().with_server(|server| host::learnable_recipes(server));
                    perform(c.get_source(), &players, action, &recipes)
                })),
        )
    }
    ctx.dispatcher.register(
        literal("recipe").requires(has_permission(LEVEL_GAMEMASTERS)).then(branch(Action::Give, "give")).then(branch(Action::Take, "take")),
    );
});

/// `LEARNABLE_RECIPE` (`!recipe.isSpecial()`) as the suggestion filter.
fn host_learnable_filter(id: &Identifier) -> bool {
    crate::commands::server_access::with_server(|server| host::learnable_recipe_exists(server, id))
}

/// `giveRecipes`/`takeRecipes`.
fn perform(source: &CommandSourceStack, players: &[EntityRef], action: Action, recipes: &[Identifier]) -> Result<i32, CommandSyntaxException> {
    let mut response = CommandResponseTracker::<EntityRef>::create();
    for player in players {
        let count = source.with_server(|server| match action {
            Action::Give => host::player_award_recipes(server, player, recipes),
            Action::Take => host::player_reset_recipes(server, player, recipes),
        });
        response.track(player, count);
    }
    let (key, failed) = match action {
        Action::Give => ("give", &*ERROR_GIVE_FAILED),
        Action::Take => ("take", &*ERROR_TAKE_FAILED),
    };
    response.send_feedback(
        source,
        true,
        ElementType::NonZero,
        Some(|| failed.create()),
        |player, total| translatable!(format!("commands.recipe.{key}.success.single"), total, display_name(source, player)),
        |player_count, total| translatable!(format!("commands.recipe.{key}.success.multiple"), total, player_count),
    )
}
