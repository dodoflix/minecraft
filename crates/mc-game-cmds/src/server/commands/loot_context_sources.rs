//! `LootContextSources`: the `default | block <pos> | entity <target>` branch shared by the `compute` commands.
//!
//! `ContextDecorator.createContext(context)` returns a `LootContext` that borrows the level; here the decorator builds it
//! inside the server borrow and hands it to a callback instead.
use super::item::item_accessor::resolve_resource_or_id;
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, NodeBuilder};
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::coordinates::block_pos_argument::BlockPosArgument;
use mc_game_core::commands::arguments::resource_arguments::ResourceOrId;
use mc_game_core::commands::host;
use mc_game_core::commands::{CommandSourceStack, argument, literal};
use mc_game_core::registry::keys;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::storage::loot::LootContextParamSets;
use mc_game_core::world::level::storage::loot::context::{
    ContextKeySet, LootContext, LootParams, LootParamsBuilder,
};
use mc_game_core::world::level::storage::loot::providers::number::{
    ArithmeticException, float_provider_codec, int_provider_codec,
};
use std::sync::Arc;

/// `LootContextSources.ContextDecorator.createContext(context)`: calls `callback` with the context.
pub type ContextDecorator = Arc<
    dyn Fn(
            &CommandContext<CommandSourceStack>,
            &mut dyn FnMut(&mut LootContext<'_>),
        ) -> Result<(), CommandSyntaxException>
        + Send
        + Sync,
>;

/// `ContextDecorator.createParams` + `customize` + `createContext`: the `THIS_ENTITY` of the source and its position are the
/// common parameters; `customize` adds the branch's own.
fn create_context(
    c: &CommandContext<CommandSourceStack>,
    callback: &mut dyn FnMut(&mut LootContext<'_>),
    set: &ContextKeySet,
    customize: impl for<'a> FnOnce(
        &'a mc_game_core::server::level::ServerLevel,
        &'a mc_game_core::server::MinecraftServer,
        LootParamsBuilder<'a>,
    ) -> LootParamsBuilder<'a>,
) {
    let source = c.get_source();
    source.with_server(|server| {
        let level = server.level(source.get_level()).expect("source level");
        let this_entity = source
            .get_entity()
            .and_then(|e| host::entity_ref_of(server, e, source.get_level()))
            .and_then(|e| host::get_entity(server, &e));
        let builder = LootParams::builder(level)
            .optional_this_entity(this_entity)
            .origin(source.get_position());
        let params = customize(level, server, builder).create(set);
        let mut context = LootContext::builder(&params).create(None);
        callback(&mut context);
    });
}

/// `provider.value().getIntUnsafe(decorator.createContext(context))`; the outer `Err` is the context creation failure, the
/// inner one the `ArithmeticException`.
pub fn evaluate_int(
    context: &CommandContext<CommandSourceStack>,
    decorator: &ContextDecorator,
    provider: &ResourceOrId,
) -> Result<Result<i32, ArithmeticException>, CommandSyntaxException> {
    let value = resolve_resource_or_id(
        context.get_source(),
        keys::CONTEXT_INT_PROVIDER,
        provider,
        int_provider_codec(),
    )?;
    let mut result = None;
    decorator(context, &mut |loot_context| {
        result = Some(value.get_int_unsafe(loot_context));
    })?;
    Ok(result.expect("the decorator calls back"))
}

/// `provider.value().getFloatUnsafe(decorator.createContext(context))`.
pub fn evaluate_float(
    context: &CommandContext<CommandSourceStack>,
    decorator: &ContextDecorator,
    provider: &ResourceOrId,
) -> Result<Result<f32, ArithmeticException>, CommandSyntaxException> {
    let value = resolve_resource_or_id(
        context.get_source(),
        keys::CONTEXT_FLOAT_PROVIDER,
        provider,
        float_provider_codec(),
    )?;
    let mut result = None;
    decorator(context, &mut |loot_context| {
        result = Some(value.get_float_unsafe(loot_context));
    })?;
    Ok(result.expect("the decorator calls back"))
}

/// `LootContextSources.NodeVisitor`: adds the nodes that follow a context source to `output`.
pub type NodeVisitor<'a> =
    &'a dyn Fn(ContextDecorator, &mut dyn FnMut(NodeBuilder<CommandSourceStack>));

fn decorate<B: ArgumentBuilder<CommandSourceStack>>(
    mut node: B,
    visitor: NodeVisitor<'_>,
    decorator: ContextDecorator,
) -> B {
    let mut children = Vec::new();
    visitor(decorator, &mut |child| children.push(child));
    for child in children {
        node = node.then(child);
    }
    node
}

/// `LootContextSources.addContextSources(node, factory)`.
pub fn add_context_sources<B: ArgumentBuilder<CommandSourceStack>>(
    node: B,
    visitor: NodeVisitor<'_>,
) -> B {
    node.then(decorate(
        literal("default"),
        visitor,
        Arc::new(|c, callback| {
            create_context(
                c,
                callback,
                &LootContextParamSets::COMMAND_COMPUTE_DEFAULT,
                |_, _, builder| builder,
            );
            Ok(())
        }),
    ))
    .then(literal("block").then(decorate(
        argument("computePos", BlockPosArgument::block_pos()),
        visitor,
        Arc::new(|c, callback| {
            let pos = BlockPosArgument::get_loaded_block_pos(c, "computePos")?;
            create_context(
                c,
                callback,
                &LootContextParamSets::COMMAND_COMPUTE_POSITION,
                |level, _, builder| {
                    builder
                        .block_state(level.get_block_state(pos))
                        .block_entity(level.get_block_entity(pos))
                },
            );
            Ok(())
        }),
    )))
    .then(literal("entity").then(decorate(
        argument("computeTarget", EntityArgument::entity()),
        visitor,
        Arc::new(|c, callback| {
            let target = EntityArgument::get_entity(c, "computeTarget")?;
            create_context(
                c,
                callback,
                &LootContextParamSets::COMMAND_COMPUTE_ENTITY,
                |_, server, builder| {
                    let entity =
                        host::get_entity(server, &target).expect("a selected entity exists");
                    builder.target_entity(entity)
                },
            );
            Ok(())
        }),
    )))
}
