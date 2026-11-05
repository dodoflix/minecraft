//! `ComputeCommand`.
//!
//! SEAM(loot): evaluating a `ContextIntProvider`/`ContextFloatProvider` needs a `LootContext` and the loot number
//! providers, which the entity/loot code owns ([`LootContext`] cannot be constructed yet, see `loot_context_sources`).
//! [`evaluate_int`]/[`evaluate_float`] are the two calls (`provider.value().getIntUnsafe(lootContext)` /
//! `getFloatUnsafe(lootContext)`, `Err` = the `ArithmeticException` message) to fill once it exists; everything else
//! (tree, `scale`, rounding, result messages and errors) is vanilla's.
use super::loot_context_sources::{ContextDecorator, LootContext, add_context_sources};
use crate::commands::arguments::resource_arguments::{ContextFloatProviderArgument, ContextIntProviderArgument, ResourceOrId};
use crate::commands::message::message;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::{register_command, translatable};
use mc_brigadier::arguments::{FloatArgumentType, float_arg};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType, DynamicCommandExceptionType, NodeBuilder};
use mc_core::math::java::java_float_to_string;
use mc_core::math::mth;
use mc_text::Component;
use std::sync::LazyLock;

static INVALID_NAMED_VALUE: LazyLock<Dynamic2CommandExceptionType<Component, String>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|provider: Component, value: String| message(translatable!("command.compute.result.named.invalid", provider, value)))
});
static INVALID_UNNAMED_VALUE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|value: String| message(translatable!("command.compute.result.unnamed.invalid", value))));

register_command!("ComputeCommand", |ctx| {
    let build_context = ctx.build_context.clone();
    ctx.dispatcher.register(add_context_sources(
        literal("compute").requires(has_permission(LEVEL_GAMEMASTERS)),
        &move |decorator: ContextDecorator, output: &mut dyn FnMut(NodeBuilder<CommandSourceStack>)| {
            let (float_decorator, scaled_decorator, int_decorator) = (decorator.clone(), decorator.clone(), decorator);
            output(NodeBuilder::Literal(literal("float").then(
                argument("provider", ContextFloatProviderArgument::float_provider(&build_context))
                    .executes(move |c| compute_as_float(c, &float_decorator, &ContextFloatProviderArgument::get_float_provider(c, "provider"), 1.0))
                    .then(argument("scale", float_arg()).executes(move |c| {
                        compute_as_float(
                            c,
                            &scaled_decorator,
                            &ContextFloatProviderArgument::get_float_provider(c, "provider"),
                            FloatArgumentType::get_float(c, "scale"),
                        )
                    })),
            )));
            output(NodeBuilder::Literal(literal("integer").then(
                argument("provider", ContextIntProviderArgument::int_provider(&build_context))
                    .executes(move |c| compute_as_int(c, &int_decorator, &ContextIntProviderArgument::get_int_provider(c, "provider"))),
            )));
        },
    ));
});

/// `provider.value().getIntUnsafe(context)`.
fn evaluate_int(_provider: &ResourceOrId, context: &LootContext) -> Result<i32, String> {
    match *context {}
}

/// `provider.value().getFloatUnsafe(context)`.
fn evaluate_float(_provider: &ResourceOrId, context: &LootContext) -> Result<f32, String> {
    match *context {}
}

/// `Component.translationArg(key.identifier())` of the holder's key, when it has one (`Holder.unwrapKey()`).
fn provider_name(provider: &ResourceOrId) -> Option<Component> {
    match provider {
        ResourceOrId::Reference(key) => Some(Component::literal(key.id.to_string())),
        ResourceOrId::Inline(_) => None,
    }
}

fn print_exact_output(source: &CommandSourceStack, provider: &ResourceOrId, result: i32) {
    source.send_success(
        || match provider_name(provider) {
            Some(name) => translatable!("command.compute.result.named.exact", name, result),
            None => translatable!("command.compute.result.unnamed.exact", result),
        },
        false,
    );
}

fn print_rounded_output(source: &CommandSourceStack, provider: &ResourceOrId, result: i32, original: f32) {
    source.send_success(
        || match provider_name(provider) {
            Some(name) => translatable!("command.compute.result.named.rounded", name, original, result),
            None => translatable!("command.compute.result.unnamed.rounded", original, result),
        },
        false,
    );
}

fn compute_as_int(context: &CommandContext<CommandSourceStack>, decorator: &ContextDecorator, provider: &ResourceOrId) -> Result<i32, CommandSyntaxException> {
    let loot_context = decorator(context)?;
    match evaluate_int(provider, &loot_context) {
        Ok(result) => {
            print_exact_output(context.get_source(), provider, result);
            Ok(result)
        }
        Err(message) => Err(throw_invalid_value(provider, message)),
    }
}

fn compute_as_float(
    context: &CommandContext<CommandSourceStack>,
    decorator: &ContextDecorator,
    provider: &ResourceOrId,
    scale: f32,
) -> Result<i32, CommandSyntaxException> {
    let loot_context = decorator(context)?;
    let original = evaluate_float(provider, &loot_context).map_err(|message| throw_invalid_value(provider, message))?;
    if !original.is_finite() {
        return Err(throw_invalid_value(provider, java_float_to_string(original)));
    }
    let result = mth::floor_f32(original * scale);
    let source = context.get_source();
    if result as f32 == original {
        print_exact_output(source, provider, result);
    } else {
        print_rounded_output(source, provider, result, original);
    }
    Ok(result)
}

fn throw_invalid_value(provider: &ResourceOrId, invalid_value: String) -> CommandSyntaxException {
    match provider_name(provider) {
        Some(name) => INVALID_NAMED_VALUE.create(name, invalid_value),
        None => INVALID_UNNAMED_VALUE.create(invalid_value),
    }
}
