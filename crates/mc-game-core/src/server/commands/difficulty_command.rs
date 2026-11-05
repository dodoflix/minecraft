//! `DifficultyCommand`.
use crate::commands::message::message;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, has_permission, literal};
use crate::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, DynamicCommandExceptionType};
use mc_text::Component;
use mc_world::level_storage::Difficulty;
use std::sync::LazyLock;

static ERROR_ALREADY_SAME_DIFFICULTY: LazyLock<DynamicCommandExceptionType<Component>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|difficulty: Component| message(translatable!("commands.difficulty.failure", difficulty))));

const DIFFICULTIES: [Difficulty; 4] = [Difficulty::Peaceful, Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];

/// `Difficulty.getDisplayName()`.
fn display_name(difficulty: Difficulty) -> Component {
    Component::translatable(format!("options.difficulty.{}", difficulty.get_serialized_name()), Vec::new())
}

register_command!("DifficultyCommand", |ctx| {
    let mut command = literal("difficulty");
    for difficulty in DIFFICULTIES {
        command = command.then(literal(difficulty.get_serialized_name()).executes(move |c| set_difficulty(c.get_source(), difficulty)));
    }
    ctx.dispatcher.register(command.requires(has_permission(LEVEL_GAMEMASTERS)).executes(|c| {
        let difficulty = c.get_source().with_server(|server| server.world_data.lock().settings.difficulty_settings.difficulty);
        c.get_source().send_success(|| translatable!("commands.difficulty.query", display_name(difficulty)), false);
        Ok(difficulty.get_id())
    }));
});

pub fn set_difficulty(source: &CommandSourceStack, difficulty: Difficulty) -> Result<i32, CommandSyntaxException> {
    let current = source.with_server(|server| server.world_data.lock().settings.difficulty_settings.difficulty);
    if current == difficulty {
        return Err(ERROR_ALREADY_SAME_DIFFICULTY.create(display_name(difficulty)));
    }
    source.with_server(|server| server.set_difficulty(difficulty, true));
    source.send_success(|| translatable!("commands.difficulty.success", display_name(difficulty)), true);
    Ok(0)
}
