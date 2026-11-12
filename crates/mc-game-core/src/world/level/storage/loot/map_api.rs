//! `ExplorationMapFunction.run` (`level.findNearestMapStructure` + `MapItem.applyNewSavedData` / `renderBiomePreviewMap` /
//! `MapItemSavedData.addTargetDecoration`): the stack is returned untouched when the level finds no structure.

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_protocol::components::{HolderSet, ItemStack, NoDirect};

use super::functions::items::ExplorationMapFunction;
use crate::server::level::ServerLevel;
use crate::world::item::map_item;
use crate::world::level::saveddata::maps::{MapItemSavedData, decoration_type};

/// The structures of `destination` (the members of a tag in registry order; the vanilla tag data when the level's registries
/// have no structure registry).
fn destination_ids(level: &ServerLevel, destination: &HolderSet<NoDirect>) -> Vec<Identifier> {
    match destination {
        HolderSet::Direct(entries) => entries.iter().filter_map(|h| h.name().cloned()).collect(),
        HolderSet::Named(tag) => match level.registries.registry("worldgen/structure") {
            Some(registry) => registry
                .tag(tag)
                .map(|ids| ids.iter().filter_map(|id| registry.name_of(*id)).collect())
                .unwrap_or_default(),
            None => mc_data::tags::names("worldgen/structure", &tag.to_string())
                .unwrap_or_default()
                .iter()
                .filter_map(|name| Identifier::parse(name).ok())
                .collect(),
        },
    }
}

pub fn explore(
    level: &ServerLevel,
    mut stack: ItemStack,
    origin: Vec3,
    function: &ExplorationMapFunction,
) -> ItemStack {
    let loot_pos = BlockPos::containing(origin.x, origin.y, origin.z);
    let wanted = destination_ids(level, &function.destination);
    if function.skip_known_structures {
        level.claim_structure_at(&wanted, loot_pos);
    }
    let Some((nearest, _)) = level.find_nearest_map_structure_shared(
        &wanted,
        loot_pos,
        function.search_radius,
        function.skip_known_structures,
    ) else {
        return stack;
    };
    map_item::apply_new_saved_data(
        level,
        &mut stack,
        nearest.x(),
        nearest.z(),
        function.zoom,
        true,
        true,
    );
    map_item::render_biome_preview_map(level, &stack);
    MapItemSavedData::add_target_decoration(
        &mut stack,
        nearest,
        "+",
        decoration_type(function.map_decoration.path()),
    );
    stack
}
