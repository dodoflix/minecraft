//! `AttributeCommand`.
use mc_brigadier::arguments::{DoubleArgumentType, double_arg};
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType,
    Dynamic3CommandExceptionType, DynamicCommandExceptionType,
};
use mc_core::ident::Identifier;
use mc_data::Attribute;
use mc_game_core::commands::arguments::resource_arguments::ResourceArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::{EntityArgument, IdentifierArgument};
use mc_game_core::commands::message::message;
use mc_game_core::commands::shared_suggestion_provider::suggest_resource;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use mc_game_core::world::entity::living::LivingEntityData;
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_NOT_LIVING_ENTITY: LazyLock<DynamicCommandExceptionType<Component>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|target: Component| {
            message(translatable!("commands.attribute.failed.entity", target))
        })
    });
static ERROR_NO_SUCH_ATTRIBUTE: LazyLock<Dynamic2CommandExceptionType<Component, Component>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|target: Component, attribute: Component| {
            message(translatable!(
                "commands.attribute.failed.no_attribute",
                target,
                attribute
            ))
        })
    });
static ERROR_NO_SUCH_MODIFIER: LazyLock<
    Dynamic3CommandExceptionType<Component, Component, String>,
> = LazyLock::new(|| {
    Dynamic3CommandExceptionType::new(
        |target: Component, attribute: Component, modifier: String| {
            message(translatable!(
                "commands.attribute.failed.no_modifier",
                attribute,
                target,
                modifier
            ))
        },
    )
});
static ERROR_MODIFIER_ALREADY_PRESENT: LazyLock<
    Dynamic3CommandExceptionType<Component, Component, String>,
> = LazyLock::new(|| {
    Dynamic3CommandExceptionType::new(
        |target: Component, attribute: Component, modifier: String| {
            message(translatable!(
                "commands.attribute.failed.modifier_already_present",
                modifier,
                attribute,
                target
            ))
        },
    )
});

register_command!("AttributeCommand", |ctx| {
    fn target(c: &CommandContext<CommandSourceStack>) -> Result<EntityRef, CommandSyntaxException> {
        EntityArgument::get_entity(c, "target")
    }
    fn attribute(
        c: &CommandContext<CommandSourceStack>,
    ) -> Result<Attribute, CommandSyntaxException> {
        ResourceArgument::get_attribute(c, "attribute")
    }
    fn id(c: &CommandContext<CommandSourceStack>) -> Identifier {
        IdentifierArgument::get_id(c, "id")
    }
    fn scale(c: &CommandContext<CommandSourceStack>) -> f64 {
        DoubleArgumentType::get_double(c, "scale")
    }
    fn value(c: &CommandContext<CommandSourceStack>) -> f64 {
        DoubleArgumentType::get_double(c, "value")
    }
    fn add(
        c: &CommandContext<CommandSourceStack>,
        operation: AttributeModifierOperation,
    ) -> Result<i32, CommandSyntaxException> {
        add_modifier(
            c.get_source(),
            &target(c)?,
            attribute(c)?,
            id(c),
            value(c),
            operation,
        )
    }
    fn modifier_ids(
        c: &CommandContext<CommandSourceStack>,
        builder: mc_brigadier::SuggestionsBuilder,
    ) -> Result<mc_brigadier::Suggestions, CommandSyntaxException> {
        Ok(suggest_resource(
            get_attribute_modifiers(c.get_source(), &target(c)?, attribute(c)?)?,
            builder,
        ))
    }
    ctx.dispatcher.register(
        literal("attribute")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                argument("target", EntityArgument::entity()).then(
                    argument(
                        "attribute",
                        ResourceArgument::resource(ctx.build_context, "attribute"),
                    )
                    .then(
                        literal("get")
                            .executes(|c| {
                                get_attribute_value(c.get_source(), &target(c)?, attribute(c)?, 1.0)
                            })
                            .then(argument("scale", double_arg()).executes(|c| {
                                get_attribute_value(
                                    c.get_source(),
                                    &target(c)?,
                                    attribute(c)?,
                                    scale(c),
                                )
                            })),
                    )
                    .then(
                        literal("base")
                            .then(
                                literal("set").then(argument("value", double_arg()).executes(
                                    |c| {
                                        set_attribute_base(
                                            c.get_source(),
                                            &target(c)?,
                                            attribute(c)?,
                                            value(c),
                                        )
                                    },
                                )),
                            )
                            .then(
                                literal("get")
                                    .executes(|c| {
                                        get_attribute_base(
                                            c.get_source(),
                                            &target(c)?,
                                            attribute(c)?,
                                            1.0,
                                        )
                                    })
                                    .then(argument("scale", double_arg()).executes(|c| {
                                        get_attribute_base(
                                            c.get_source(),
                                            &target(c)?,
                                            attribute(c)?,
                                            scale(c),
                                        )
                                    })),
                            )
                            .then(literal("reset").executes(|c| {
                                reset_attribute_base(c.get_source(), &target(c)?, attribute(c)?)
                            })),
                    )
                    .then(
                        literal("modifier")
                            .then(
                                literal("add").then(
                                    argument("id", IdentifierArgument::id()).then(
                                        argument("value", double_arg())
                                            .then(literal("add_value").executes(|c| {
                                                add(c, AttributeModifierOperation::AddValue)
                                            }))
                                            .then(literal("add_multiplied_base").executes(|c| {
                                                add(
                                                    c,
                                                    AttributeModifierOperation::AddMultipliedBase,
                                                )
                                            }))
                                            .then(literal("add_multiplied_total").executes(|c| {
                                                add(
                                                    c,
                                                    AttributeModifierOperation::AddMultipliedTotal,
                                                )
                                            })),
                                    ),
                                ),
                            )
                            .then(
                                literal("remove").then(
                                    argument("id", IdentifierArgument::id())
                                        .suggests(modifier_ids)
                                        .executes(|c| {
                                            remove_modifier(
                                                c.get_source(),
                                                &target(c)?,
                                                attribute(c)?,
                                                id(c),
                                            )
                                        }),
                                ),
                            )
                            .then(
                                literal("value").then(
                                    literal("get").then(
                                        argument("id", IdentifierArgument::id())
                                            .suggests(modifier_ids)
                                            .executes(|c| {
                                                get_attribute_modifier(
                                                    c.get_source(),
                                                    &target(c)?,
                                                    attribute(c)?,
                                                    id(c),
                                                    1.0,
                                                )
                                            })
                                            .then(argument("scale", double_arg()).executes(|c| {
                                                get_attribute_modifier(
                                                    c.get_source(),
                                                    &target(c)?,
                                                    attribute(c)?,
                                                    id(c),
                                                    scale(c),
                                                )
                                            })),
                                    ),
                                ),
                            ),
                    ),
                ),
            ),
    );
});

fn attribute_description(attribute: Attribute) -> Component {
    Component::translatable(attribute.data().description_id, Vec::new())
}

/// Runs `f` on the living data of `target`: `ERROR_NOT_LIVING_ENTITY` for other entities.
fn with_living<R>(
    source: &CommandSourceStack,
    target: &EntityRef,
    f: impl FnOnce(&mut LivingEntityData) -> R,
) -> Result<R, CommandSyntaxException> {
    let result = source.with_server(|server| {
        server
            .level_mut(&target.dimension)
            .and_then(|l| l.get_entity_mut(target.id))
            .and_then(|e| e.living_mut())
            .map(f)
    });
    result.ok_or_else(|| ERROR_NOT_LIVING_ENTITY.create(host::display_name_of(source, target)))
}

/// `getEntityWithAttribute(target, attribute)` then `f`.
fn with_attribute<R>(
    source: &CommandSourceStack,
    target: &EntityRef,
    attribute: Attribute,
    f: impl FnOnce(&mut LivingEntityData) -> R,
) -> Result<R, CommandSyntaxException> {
    let has_attribute = with_living(source, target, |living| {
        living.attributes.has_attribute(attribute)
    })?;
    if !has_attribute {
        return Err(ERROR_NO_SUCH_ATTRIBUTE.create(
            host::display_name_of(source, target),
            attribute_description(attribute),
        ));
    }
    with_living(source, target, f)
}

fn get_attribute_value(
    source: &CommandSourceStack,
    target: &EntityRef,
    attribute: Attribute,
    scale: f64,
) -> Result<i32, CommandSyntaxException> {
    let result = with_attribute(source, target, attribute, |living| {
        living.attributes.get_value(attribute)
    })?;
    source.send_success(
        || {
            translatable!(
                "commands.attribute.value.get.success",
                attribute_description(attribute),
                host::display_name_of(source, target),
                result
            )
        },
        false,
    );
    Ok((result * scale) as i32)
}

fn get_attribute_base(
    source: &CommandSourceStack,
    target: &EntityRef,
    attribute: Attribute,
    scale: f64,
) -> Result<i32, CommandSyntaxException> {
    let result = with_attribute(source, target, attribute, |living| {
        living.attributes.get_base_value(attribute)
    })?;
    source.send_success(
        || {
            translatable!(
                "commands.attribute.base_value.get.success",
                attribute_description(attribute),
                host::display_name_of(source, target),
                result
            )
        },
        false,
    );
    Ok((result * scale) as i32)
}

fn get_attribute_modifier(
    source: &CommandSourceStack,
    target: &EntityRef,
    attribute: Attribute,
    id: Identifier,
    scale: f64,
) -> Result<i32, CommandSyntaxException> {
    let result = with_attribute(source, target, attribute, |living| {
        living.attributes.has_modifier(attribute, &id).then(|| {
            living
                .attributes
                .get_instance(attribute)
                .and_then(|i| i.get_modifier(&id).map(|m| m.amount))
        })
    })?;
    let Some(Some(result)) = result else {
        return Err(ERROR_NO_SUCH_MODIFIER.create(
            host::display_name_of(source, target),
            attribute_description(attribute),
            id.to_string(),
        ));
    };
    source.send_success(
        || {
            translatable!(
                "commands.attribute.modifier.value.get.success",
                Component::literal(id.to_string()),
                attribute_description(attribute),
                host::display_name_of(source, target),
                result
            )
        },
        false,
    );
    Ok((result * scale) as i32)
}

/// `getAttributeModifiers(target, attribute)`: the ids of the modifiers (suggestions of the `id` argument).
fn get_attribute_modifiers(
    source: &CommandSourceStack,
    target: &EntityRef,
    attribute: Attribute,
) -> Result<Vec<Identifier>, CommandSyntaxException> {
    let instance = with_living(source, target, |living| {
        living
            .attributes
            .get_instance(attribute)
            .map(|i| i.get_modifiers().map(|m| m.id.clone()).collect::<Vec<_>>())
    })?;
    instance.ok_or_else(|| {
        ERROR_NO_SUCH_ATTRIBUTE.create(
            host::display_name_of(source, target),
            attribute_description(attribute),
        )
    })
}

/// `getAttributeInstance(target, attribute)` then `f`.
fn with_instance<R>(
    source: &CommandSourceStack,
    target: &EntityRef,
    attribute: Attribute,
    f: impl FnOnce(&mut mc_game_core::world::entity::attributes::AttributeInstance) -> R,
) -> Result<R, CommandSyntaxException> {
    let result = with_living(source, target, |living| {
        living.attributes.get_instance(attribute).map(f)
    })?;
    result.ok_or_else(|| {
        ERROR_NO_SUCH_ATTRIBUTE.create(
            host::display_name_of(source, target),
            attribute_description(attribute),
        )
    })
}

fn set_attribute_base(
    source: &CommandSourceStack,
    target: &EntityRef,
    attribute: Attribute,
    value: f64,
) -> Result<i32, CommandSyntaxException> {
    with_instance(source, target, attribute, |instance| {
        instance.set_base_value(value)
    })?;
    source.send_success(
        || {
            translatable!(
                "commands.attribute.base_value.set.success",
                attribute_description(attribute),
                host::display_name_of(source, target),
                value
            )
        },
        false,
    );
    Ok(1)
}

fn reset_attribute_base(
    source: &CommandSourceStack,
    target: &EntityRef,
    attribute: Attribute,
) -> Result<i32, CommandSyntaxException> {
    let reset = with_living(source, target, |living| {
        living
            .attributes
            .reset_base_value(attribute)
            .then(|| living.attributes.get_base_value(attribute))
    })?;
    let Some(value) = reset else {
        return Err(ERROR_NO_SUCH_ATTRIBUTE.create(
            host::display_name_of(source, target),
            attribute_description(attribute),
        ));
    };
    source.send_success(
        || {
            translatable!(
                "commands.attribute.base_value.reset.success",
                attribute_description(attribute),
                host::display_name_of(source, target),
                value
            )
        },
        false,
    );
    Ok(1)
}

fn add_modifier(
    source: &CommandSourceStack,
    target: &EntityRef,
    attribute: Attribute,
    id: Identifier,
    value: f64,
    operation: AttributeModifierOperation,
) -> Result<i32, CommandSyntaxException> {
    let modifier = AttributeModifier {
        id: id.clone(),
        amount: value,
        operation,
    };
    let added = with_instance(source, target, attribute, |instance| {
        if instance.has_modifier(&id) {
            false
        } else {
            instance.add_permanent_modifier(modifier);
            true
        }
    })?;
    if !added {
        return Err(ERROR_MODIFIER_ALREADY_PRESENT.create(
            host::display_name_of(source, target),
            attribute_description(attribute),
            id.to_string(),
        ));
    }
    source.send_success(
        || {
            translatable!(
                "commands.attribute.modifier.add.success",
                Component::literal(id.to_string()),
                attribute_description(attribute),
                host::display_name_of(source, target)
            )
        },
        false,
    );
    Ok(1)
}

fn remove_modifier(
    source: &CommandSourceStack,
    target: &EntityRef,
    attribute: Attribute,
    id: Identifier,
) -> Result<i32, CommandSyntaxException> {
    let removed = with_instance(source, target, attribute, |instance| {
        instance.remove_modifier(&id)
    })?;
    if !removed {
        return Err(ERROR_NO_SUCH_MODIFIER.create(
            host::display_name_of(source, target),
            attribute_description(attribute),
            id.to_string(),
        ));
    }
    source.send_success(
        || {
            translatable!(
                "commands.attribute.modifier.remove.success",
                Component::literal(id.to_string()),
                attribute_description(attribute),
                host::display_name_of(source, target)
            )
        },
        false,
    );
    Ok(1)
}
