//! `LootContextSources`: the `default | block <pos> | entity <target>` branch shared by the `compute` commands.
//!
//! SEAM(loot): there is no loot system (`LootContext`/`LootParams`/loot number providers) yet, so a decorator resolves its
//! command arguments exactly like vanilla (loaded position, entity selector) and then fails: [`LootContext`] cannot be
//! constructed. Replace the enum with the real `LootContext` and fill [`ContextDecorator`] once loot contexts exist.
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::coordinates::block_pos_argument::BlockPosArgument;
use mc_game_core::commands::message::literal_message;
use mc_game_core::commands::{CommandSourceStack, argument, literal};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, NodeBuilder, SimpleCommandExceptionType};
use std::sync::{Arc, LazyLock};

/// `LootContext` (not implemented: uninhabited).
pub enum LootContext {}

/// `LootContextSources.ContextDecorator.createContext(context)`.
pub type ContextDecorator = Arc<dyn Fn(&CommandContext<CommandSourceStack>) -> Result<LootContext, CommandSyntaxException> + Send + Sync>;

fn unsupported() -> CommandSyntaxException {
    static ERROR: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| {
        SimpleCommandExceptionType::new(literal_message("Loot contexts are not implemented: cannot evaluate context number providers"))
    });
    ERROR.create()
}

/// `LootContextSources.NodeVisitor`: adds the nodes that follow a context source to `output`.
pub type NodeVisitor<'a> = &'a dyn Fn(ContextDecorator, &mut dyn FnMut(NodeBuilder<CommandSourceStack>));

fn decorate<B: ArgumentBuilder<CommandSourceStack>>(mut node: B, visitor: NodeVisitor<'_>, decorator: ContextDecorator) -> B {
    let mut children = Vec::new();
    visitor(decorator, &mut |child| children.push(child));
    for child in children {
        node = node.then(child);
    }
    node
}

/// `LootContextSources.addContextSources(node, factory)`.
pub fn add_context_sources<B: ArgumentBuilder<CommandSourceStack>>(node: B, visitor: NodeVisitor<'_>) -> B {
    node.then(decorate(literal("default"), visitor, Arc::new(|_| Err(unsupported()))))
        .then(literal("block").then(decorate(
            argument("computePos", BlockPosArgument::block_pos()),
            visitor,
            Arc::new(|c| {
                BlockPosArgument::get_loaded_block_pos(c, "computePos")?;
                Err(unsupported())
            }),
        )))
        .then(literal("entity").then(decorate(
            argument("computeTarget", EntityArgument::entity()),
            visitor,
            Arc::new(|c| {
                EntityArgument::get_entity(c, "computeTarget")?;
                Err(unsupported())
            }),
        )))
}
