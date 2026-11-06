//! `SummonCommand` (also used by `/execute summon`).
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, SimpleCommandExceptionType,
};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;
use mc_game_core::commands::arguments::CompoundTagArgument;
use mc_game_core::commands::arguments::coordinates::Vec3Argument;
use mc_game_core::commands::arguments::coordinates::block_pos_argument::is_in_spawnable_bounds;
use mc_game_core::commands::arguments::resource_arguments::ResourceArgument;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::message::simple_translatable;
use mc_game_core::commands::synchronization::suggestion_providers;
use mc_game_core::commands::{
    CommandEntity, CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::world::entity::factory::{self, EntitySpawnRequest};
use mc_game_core::{register_command, translatable};
use mc_nbt::CompoundTag;
use mc_world::level_storage::Difficulty;
use std::sync::LazyLock;

static ERROR_FAILED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.summon.failed"));
static ERROR_FAILED_PEACEFUL: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.summon.failed.peaceful"));
static ERROR_DUPLICATE_UUID: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.summon.failed.uuid"));
static INVALID_POSITION: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.summon.invalidPosition"));

register_command!("SummonCommand", |ctx| {
    fn entity(
        c: &CommandContext<CommandSourceStack>,
    ) -> Result<EntityType, CommandSyntaxException> {
        ResourceArgument::get_summonable_entity_type(c, "entity")
    }
    ctx.dispatcher.register(
        literal("summon")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                argument(
                    "entity",
                    ResourceArgument::resource(ctx.build_context, "entity_type"),
                )
                .suggests_provider(Some(suggestion_providers::summonable_entities()))
                .executes(|c| {
                    spawn_entity(
                        c.get_source(),
                        entity(c)?,
                        c.get_source().get_position(),
                        &CompoundTag::new(),
                        true,
                    )
                })
                .then(
                    argument("pos", Vec3Argument::vec3())
                        .executes(|c| {
                            spawn_entity(
                                c.get_source(),
                                entity(c)?,
                                Vec3Argument::get_vec3(c, "pos"),
                                &CompoundTag::new(),
                                true,
                            )
                        })
                        .then(
                            argument("nbt", CompoundTagArgument::compound_tag()).executes(|c| {
                                spawn_entity(
                                    c.get_source(),
                                    entity(c)?,
                                    Vec3Argument::get_vec3(c, "pos"),
                                    &CompoundTagArgument::get_compound_tag(c, "nbt"),
                                    false,
                                )
                            }),
                        ),
                ),
            ),
    );
});

/// `createEntity(source, type, pos, nbt, finalize)`: loads the entity (and its passengers) from `nbt`, places it at `pos`,
/// finalizes mobs and adds the tree to the source's level. The [`CommandEntity`] describes the entity as it was when added.
pub fn create_entity(
    source: &CommandSourceStack,
    entity_type: EntityType,
    pos: Vec3,
    nbt: &CompoundTag,
    finalize: bool,
) -> Result<(EntityRef, CommandEntity), CommandSyntaxException> {
    if !is_in_spawnable_bounds(BlockPos::containing(pos.x, pos.y, pos.z)) {
        return Err(INVALID_POSITION.create());
    }
    let dimension = source.get_level().clone();
    source.with_server(|server| {
        let Some(level) = server.level_mut(&dimension) else {
            return Err(ERROR_FAILED.create());
        };
        if level.difficulty() == Difficulty::Peaceful && !entity_type.data().allowed_in_peaceful {
            return Err(ERROR_FAILED_PEACEFUL.create());
        }
        let mut entity_tag = nbt.clone();
        entity_tag.put_string("id", entity_type.name());
        let mut loaded = Vec::new();
        factory::load_entity_recursive(
            &entity_tag,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Command,
                ignore_checks: false,
            },
            &mut loaded,
        );
        if loaded.is_empty() {
            return Err(ERROR_FAILED.create());
        }
        // the `postLoad` of the command: every entity of the tree is moved to `pos`, keeping its own rotation
        for entity in &mut loaded {
            let (y_rot, x_rot) = (entity.base().y_rot(), entity.base().x_rot());
            entity.snap_to(pos.x, pos.y, pos.z, y_rot, x_rot);
        }
        let id = loaded[0].id();
        if finalize {
            host::finalize_mob_spawn(level, loaded[0].as_mut());
        }
        // vanilla keeps the entity object, so the name is known even when its chunk is not loaded and the entity cannot
        // be looked up in the level
        let entity = EntityRef { dimension, id };
        let command_entity = host::command_entity_of(server, &entity, loaded[0].as_ref());
        let Some(level) = server.level_mut(&entity.dimension) else {
            return Err(ERROR_FAILED.create());
        };
        if !level.try_add_fresh_entity_with_passengers(loaded) {
            return Err(ERROR_DUPLICATE_UUID.create());
        }
        Ok((entity, command_entity))
    })
}

fn spawn_entity(
    source: &CommandSourceStack,
    entity_type: EntityType,
    pos: Vec3,
    nbt: &CompoundTag,
    finalize: bool,
) -> Result<i32, CommandSyntaxException> {
    let (_, entity) = create_entity(source, entity_type, pos, nbt, finalize)?;
    source.send_success(
        || translatable!("commands.summon.success", entity.display_name.clone()),
        true,
    );
    Ok(1)
}
