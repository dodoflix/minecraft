//! `EnchantCommand`.
use super::command_response_tracker::CommandResponseTracker;
use mc_brigadier::arguments::{IntegerArgumentType, integer_min};
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType,
    DynamicCommandExceptionType, SimpleCommandExceptionType,
};
use mc_core::ident::Identifier;
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::resource_arguments::ResourceArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::message::{dynamic_translatable, message, simple_translatable};
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::item::ItemStackExt;
use mc_game_core::world::item::enchantment::item_enchantments;
use mc_game_core::world::item::enchantment_helper::{
    definition, get_enchantments_for_crafting, is_enchantment_compatible,
};
use mc_game_core::{register_command, translatable};
use std::sync::LazyLock;

static ERROR_NOT_LIVING_ENTITY: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.enchant.failed.entity"));
static ERROR_NO_ITEM: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.enchant.failed.itemless"));
static ERROR_INCOMPATIBLE: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.enchant.failed.incompatible"));
static ERROR_LEVEL_TOO_HIGH: LazyLock<Dynamic2CommandExceptionType<i32, i32>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|level: i32, max: i32| {
            message(translatable!("commands.enchant.failed.level", level, max))
        })
    });
static ERROR_NOTHING_HAPPENED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.enchant.failed"));

register_command!("EnchantCommand", |ctx| {
    fn enchantment(
        c: &CommandContext<CommandSourceStack>,
    ) -> Result<Identifier, CommandSyntaxException> {
        Ok(ResourceArgument::get_enchantment(c, "enchantment")?.id)
    }
    ctx.dispatcher.register(
        literal("enchant")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                argument("targets", EntityArgument::entities()).then(
                    argument(
                        "enchantment",
                        ResourceArgument::resource(ctx.build_context, "enchantment"),
                    )
                    .executes(|c| {
                        enchant(
                            c.get_source(),
                            EntityArgument::get_entities(c, "targets")?,
                            enchantment(c)?,
                            1,
                        )
                    })
                    .then(argument("level", integer_min(0)).executes(|c| {
                        enchant(
                            c.get_source(),
                            EntityArgument::get_entities(c, "targets")?,
                            enchantment(c)?,
                            IntegerArgumentType::get_integer(c, "level"),
                        )
                    })),
                ),
            ),
    );
});

/// What happened to one target.
enum Outcome {
    Enchanted,
    NotLiving,
    NoItem,
    Incompatible(String),
}

fn enchant(
    source: &CommandSourceStack,
    targets: Vec<EntityRef>,
    enchantment: Identifier,
    level: i32,
) -> Result<i32, CommandSyntaxException> {
    let max_level = source.with_server(|server| {
        let level = server.level(&source.get_level().clone())?;
        definition(&level.registries, &enchantment).map(|e| e.get_max_level())
    });
    let Some(max_level) = max_level else {
        return Err(ERROR_NOTHING_HAPPENED.create());
    };
    if level > max_level {
        return Err(ERROR_LEVEL_TOO_HIGH.create(level, max_level));
    }
    let mut tracker = CommandResponseTracker::<EntityRef>::create();
    for entity in &targets {
        let outcome = source.with_server(|server| {
            let Some(world) = server.level_mut(&entity.dimension) else {
                return Outcome::NotLiving;
            };
            let registries = world.registries.clone();
            let Some(target) = world.get_entity_mut(entity.id) else {
                return Outcome::NotLiving;
            };
            if target.living().is_none() {
                return Outcome::NotLiving;
            }
            let Some(item) = target.item_by_slot_mut(EquipmentSlot::MainHand) else {
                return Outcome::NotLiving;
            };
            if item.is_empty() {
                return Outcome::NoItem;
            }
            let keys: Vec<Identifier> = get_enchantments_for_crafting(item)
                .entries()
                .iter()
                .filter_map(|(holder, _)| holder.name().cloned())
                .collect();
            let can_enchant = definition(&registries, &enchantment)
                .is_some_and(|e| e.can_enchant(&registries, item));
            if can_enchant && is_enchantment_compatible(&registries, &keys, &enchantment) {
                item_enchantments::enchant(item, &enchantment, level);
                Outcome::Enchanted
            } else {
                Outcome::Incompatible(item.get_hover_name().get_string())
            }
        });
        match outcome {
            Outcome::Enchanted => tracker.track_one(entity),
            Outcome::NotLiving if targets.len() == 1 => {
                return Err(ERROR_NOT_LIVING_ENTITY
                    .create(host::display_name_of(source, entity).get_string()));
            }
            Outcome::NoItem if targets.len() == 1 => {
                return Err(
                    ERROR_NO_ITEM.create(host::display_name_of(source, entity).get_string())
                );
            }
            Outcome::Incompatible(item_name) if targets.len() == 1 => {
                return Err(ERROR_INCOMPATIBLE.create(item_name));
            }
            _ => {}
        }
    }
    let full_name = || {
        source.with_server(|server| {
            server
                .level(source.get_level())
                .map(|l| host::enchantment_full_name(l, &enchantment, level))
                .unwrap_or_else(mc_text::Component::empty)
        })
    };
    tracker.send_feedback_default(
        source,
        true,
        Some(&|| ERROR_NOTHING_HAPPENED.create()),
        |entity, _| {
            translatable!(
                "commands.enchant.success.single",
                full_name(),
                host::display_name_of(source, entity)
            )
        },
        |entity_count, _| {
            translatable!(
                "commands.enchant.success.multiple",
                full_name(),
                entity_count
            )
        },
    )
}
