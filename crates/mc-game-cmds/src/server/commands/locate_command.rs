//! `LocateCommand`.
use mc_game_core::commands::arguments::resource_arguments::{ResourceOrTag, ResourceOrTagArgument, ResourceOrTagKey, ResourceOrTagKeyArgument};
use mc_game_core::commands::message::dynamic_translatable;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal};
use mc_game_core::world::entity::ai::village::poi::{Occupancy, PoiTypeHolder};
use mc_game_core::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, DynamicCommandExceptionType};
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_text::events::{ClickEvent, HoverEvent};
use mc_text::ChatFormatting;
use std::sync::LazyLock;
use std::time::Instant;

static ERROR_STRUCTURE_NOT_FOUND: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("commands.locate.structure.not_found"));
static ERROR_STRUCTURE_INVALID: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("commands.locate.structure.invalid"));
static ERROR_BIOME_NOT_FOUND: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("commands.locate.biome.not_found"));
static ERROR_POI_NOT_FOUND: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| dynamic_translatable("commands.locate.poi.not_found"));

const MAX_STRUCTURE_SEARCH_RADIUS: i32 = 100;
const MAX_BIOME_SEARCH_RADIUS: i32 = 6400;
const BIOME_SAMPLE_RESOLUTION_HORIZONTAL: i32 = 32;
const BIOME_SAMPLE_RESOLUTION_VERTICAL: i32 = 64;
const POI_SEARCH_RADIUS: i32 = 256;

const STRUCTURE_REGISTRY: &str = "worldgen/structure";
const BIOME_REGISTRY: &str = "worldgen/biome";
const POI_REGISTRY: &str = "point_of_interest_type";

register_command!("LocateCommand", |ctx| {
    ctx.dispatcher.register(
        literal("locate")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(literal("structure").then(
                argument("structure", ResourceOrTagKeyArgument::resource_or_tag_key(STRUCTURE_REGISTRY)).executes(|c| {
                    locate_structure(
                        c.get_source(),
                        ResourceOrTagKeyArgument::get_resource_or_tag_key(c, "structure", STRUCTURE_REGISTRY, &ERROR_STRUCTURE_INVALID)?,
                    )
                }),
            ))
            .then(literal("biome").then(
                argument("biome", ResourceOrTagArgument::resource_or_tag(ctx.build_context, BIOME_REGISTRY))
                    .executes(|c| locate_biome(c.get_source(), ResourceOrTagArgument::get_resource_or_tag(c, "biome", BIOME_REGISTRY)?)),
            ))
            .then(literal("poi").then(
                argument("poi", ResourceOrTagArgument::resource_or_tag(ctx.build_context, POI_REGISTRY))
                    .executes(|c| locate_poi(c.get_source(), ResourceOrTagArgument::get_resource_or_tag(c, "poi", POI_REGISTRY)?)),
            )),
    );
});

fn source_block_pos(source: &CommandSourceStack) -> BlockPos {
    BlockPos::containing_position(&source.get_position())
}

fn locate_structure(source: &CommandSourceStack, resource_or_tag: ResourceOrTagKey) -> Result<i32, CommandSyntaxException> {
    let source_pos = source_block_pos(source);
    let started = Instant::now();
    // `getHolders(resourceOrTag, registry)`: `None` = unknown element / tag; the search is skipped without a generator.
    let found = source.with_server(|server| {
        let registry = host::registry_view(server, &format!("minecraft:{STRUCTURE_REGISTRY}"));
        let level = server.level_mut(source.get_level())?;
        let env = level.gen_env.clone();
        let wanted: Option<Vec<Identifier>> = match &resource_or_tag {
            ResourceOrTagKey::Resource(key) => env.as_ref().map_or(Some(Vec::new()), |env| env.structures.get(&key.id).is_some().then(|| vec![key.id.clone()])),
            ResourceOrTagKey::Tag(tag) => registry
                .as_ref()
                .and_then(|r| r.tag_elements(&tag.location))
                .map(|ids| ids.into_iter().filter(|id| env.as_ref().is_none_or(|env| env.structures.get(id).is_some())).collect()),
        };
        Some(wanted.map(|wanted| level.find_nearest_map_structure(&wanted, source_pos, MAX_STRUCTURE_SEARCH_RADIUS, false)))
    });
    let Some(Some(nearest)) = found else { return Err(ERROR_STRUCTURE_INVALID.create(resource_or_tag.as_printable())) };
    let Some((found_pos, found_id)) = nearest else { return Err(ERROR_STRUCTURE_NOT_FOUND.create(resource_or_tag.as_printable())) };
    let name = match &resource_or_tag {
        ResourceOrTagKey::Resource(key) => key.id.to_string(),
        ResourceOrTagKey::Tag(tag) => format!("#{} ({})", tag.location, found_id),
    };
    Ok(show_locate_result(source, source_pos, found_pos, "commands.locate.structure.success", false, &name, started))
}

fn locate_biome(source: &CommandSourceStack, element_or_tag: ResourceOrTag) -> Result<i32, CommandSyntaxException> {
    let source_pos = source_block_pos(source);
    let started = Instant::now();
    let nearest = source.with_server(|server| {
        let level = server.level(source.get_level())?;
        let registry = level.tick_state.biomes.clone()?;
        let (pos, biome) = level.find_closest_biome_3d(
            &|id| element_or_tag.test(registry.key(id)),
            source_pos,
            MAX_BIOME_SEARCH_RADIUS,
            BIOME_SAMPLE_RESOLUTION_HORIZONTAL,
            BIOME_SAMPLE_RESOLUTION_VERTICAL,
        )?;
        Some((pos, registry.key(biome).clone()))
    });
    let Some((found_pos, found_id)) = nearest else { return Err(ERROR_BIOME_NOT_FOUND.create(element_or_tag.as_printable())) };
    let name = found_name(&element_or_tag, &found_id);
    Ok(show_locate_result(source, source_pos, found_pos, "commands.locate.biome.success", true, &name, started))
}

fn locate_poi(source: &CommandSourceStack, resource_or_tag: ResourceOrTag) -> Result<i32, CommandSyntaxException> {
    let source_pos = source_block_pos(source);
    let started = Instant::now();
    let closest = source.with_server(|server| {
        let level = server.level(source.get_level())?;
        let matches = |holder: PoiTypeHolder| Identifier::try_parse(holder.registered_name()).is_some_and(|id| resource_or_tag.test(&id));
        level.poi_manager().find_closest_with_type(&matches, source_pos, POI_SEARCH_RADIUS, Occupancy::Any)
    });
    let Some((holder, found_pos)) = closest else { return Err(ERROR_POI_NOT_FOUND.create(resource_or_tag.as_printable())) };
    let name = found_name(&resource_or_tag, &Identifier::try_parse(holder.registered_name()).expect("registered poi type names are identifiers"));
    Ok(show_locate_result(source, source_pos, found_pos, "commands.locate.poi.success", false, &name, started))
}

/// `showLocateResult(source, name: ResourceOrTagArgument.Result, ...)`'s `foundName`.
fn found_name(name: &ResourceOrTag, found_id: &Identifier) -> String {
    match name {
        ResourceOrTag::Resource(_) => name.as_printable(),
        ResourceOrTag::Tag { .. } => format!("{} ({})", name.as_printable(), found_id),
    }
}

/// `LocateCommand.showLocateResult(source, sourcePos, found, successMessageKey, includeY, foundName, duration)`.
fn show_locate_result(
    source: &CommandSourceStack,
    source_pos: BlockPos,
    found_pos: BlockPos,
    success_message_key: &'static str,
    include_y: bool,
    found_name: &str,
    started: Instant,
) -> i32 {
    let distance = if include_y {
        mth::floor_f32(mth::sqrt(source_pos.dist_sqr(&found_pos) as f32))
    } else {
        mth::floor_f32(dist(source_pos.x(), source_pos.z(), found_pos.x(), found_pos.z()))
    };
    let displayed_y = if include_y { found_pos.y().to_string() } else { "~".to_owned() };
    let teleport_command = format!("/tp @s {} {} {}", found_pos.x(), displayed_y, found_pos.z());
    let coordinates = mc_text::util::wrap_in_square_brackets(translatable!("chat.coordinates", found_pos.x(), displayed_y.clone(), found_pos.z())).with_style_fn(|s| {
        s.with_color_formatting(Some(ChatFormatting::Green))
            .with_click_event(Some(ClickEvent::SuggestCommand(teleport_command.clone())))
            .with_hover_event(Some(HoverEvent::ShowText(Box::new(translatable!("chat.coordinates.tooltip")))))
    });
    source.send_success(|| translatable!(success_message_key, found_name.to_owned(), coordinates.clone(), distance), false);
    log::info!("Locating element {} took {} ms", found_name, started.elapsed().as_millis());
    distance
}

fn dist(x1: i32, z1: i32, x2: i32, z2: i32) -> f32 {
    let dx = x2.wrapping_sub(x1);
    let dz = z2.wrapping_sub(z1);
    mth::sqrt(dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)) as f32)
}
