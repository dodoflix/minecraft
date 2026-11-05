//! `SetBlockCommand`.
use mc_game_core::commands::arguments::blocks::BlockStateArgument;
use mc_game_core::commands::arguments::coordinates::BlockPosArgument;
use mc_game_core::commands::arguments::blocks::BlockInput;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use mc_game_core::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_brigadier::ArgumentBuilder;
use mc_game_core::{register_command, translatable};
use mc_brigadier::{CommandContext, CommandSyntaxException, SimpleCommandExceptionType};
use mc_core::pos::BlockPos;
use std::sync::LazyLock;

static ERROR_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.setblock.failed"));

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Replace,
    Destroy,
}

/// `keep` filter: `b.getLevel().isEmptyBlock(b.getPos())`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    None,
    EmptyOnly,
}

register_command!("SetBlockCommand", |ctx| {
    fn run(c: &CommandContext<CommandSourceStack>, mode: Mode, filter: Filter, strict: bool) -> Result<i32, CommandSyntaxException> {
        set_block(c.get_source(), BlockPosArgument::get_loaded_block_pos(c, "pos")?, BlockStateArgument::get_block(c, "block"), mode, filter, strict)
    }
    ctx.dispatcher.register(
        literal("setblock").requires(has_permission(LEVEL_GAMEMASTERS)).then(
            argument("pos", BlockPosArgument::block_pos()).then(
                argument("block", BlockStateArgument::block(ctx.build_context))
                    .executes(|c| run(c, Mode::Replace, Filter::None, false))
                    .then(literal("destroy").executes(|c| run(c, Mode::Destroy, Filter::None, false)))
                    .then(literal("keep").executes(|c| run(c, Mode::Replace, Filter::EmptyOnly, false)))
                    .then(literal("replace").executes(|c| run(c, Mode::Replace, Filter::None, false)))
                    .then(literal("strict").executes(|c| run(c, Mode::Replace, Filter::None, true))),
            ),
        ),
    );
});

fn set_block(source: &CommandSourceStack, pos: BlockPos, block: BlockInput, mode: Mode, filter: Filter, strict: bool) -> Result<i32, CommandSyntaxException> {
    let outcome = source.with_server(|server| {
        let level = server.level_mut(source.get_level())?;
        if level.is_debug {
            return Some(false);
        }
        if filter == Filter::EmptyOnly && !level.is_empty_block(pos) {
            return Some(false);
        }
        let place_needed = if mode == Mode::Destroy {
            level.destroy_block(pos, true, None, 512);
            !block.get_state().is_air() || !level.get_block_state(pos).is_air()
        } else {
            true
        };
        let old_state = level.get_block_state(pos);
        if place_needed && !block.place(level, pos, 2 | if strict { 816 } else { 256 }) {
            return Some(false);
        }
        if !strict {
            level.update_neighbours_on_block_set(pos, old_state);
        }
        Some(true)
    });
    if outcome != Some(true) {
        return Err(ERROR_FAILED.create());
    }
    source.send_success(|| translatable!("commands.setblock.success", pos.x(), pos.y(), pos.z()), true);
    Ok(1)
}
