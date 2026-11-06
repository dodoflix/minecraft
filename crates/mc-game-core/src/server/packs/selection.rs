//! `MinecraftServer.configurePackRepository`: choose the packs of a world from its `WorldDataConfiguration`
//! (`level.dat` `DataPacks` / `enabled_features`, or `server.properties` `initial-*-packs` for a new world).

use std::collections::HashSet;

use mc_world::level_storage::{DataPackConfig, FeatureFlagSet, WorldDataConfiguration};

use super::repository::{PackRepository, PackSource};
use crate::registry::feature_flags::{FeatureFlagSetExt, print_missing_flags};

/// `configurePackRepository(packRepository, initialDataConfig, initMode, safeMode)`.
///
/// Reloads the repository, selects the packs and returns the data configuration to persist (`DataPacks.Enabled/Disabled`
/// + the enabled features requested by the selected packs).
pub fn configure_pack_repository(
    pack_repository: &mut PackRepository,
    initial_data_config: &WorldDataConfiguration,
    init_mode: bool,
    safe_mode: bool,
) -> WorldDataConfiguration {
    let data_pack_config = initial_data_config.data_packs();
    let forced_features = if init_mode {
        FeatureFlagSet::EMPTY
    } else {
        initial_data_config.enabled_features
    };
    let allowed_features = if init_mode {
        FeatureFlagSet::ALL
    } else {
        initial_data_config.enabled_features
    };
    pack_repository.reload();
    if safe_mode {
        return configure_repository_with_selection(
            pack_repository,
            vec!["vanilla".to_owned()],
            forced_features,
            false,
        );
    }
    // `LinkedHashSet`
    let mut selected: Vec<String> = Vec::new();
    let add = |selected: &mut Vec<String>, id: &str| {
        if !selected.iter().any(|s| s == id) {
            selected.push(id.to_owned());
        }
    };
    for id in &data_pack_config.enabled {
        if pack_repository.is_available(id) {
            add(&mut selected, id);
        } else {
            log::warn!("Missing data pack {id}");
        }
    }
    let available: Vec<_> = pack_repository.available_packs().cloned().collect();
    for pack in &available {
        let pack_id = pack.id();
        if data_pack_config.disabled.iter().any(|d| d == pack_id) {
            continue;
        }
        let pack_features = pack.requested_features();
        let is_selected = selected.iter().any(|s| s == pack_id);
        if !is_selected && pack.pack_source().should_add_automatically() {
            if pack_features.is_subset_of(allowed_features) {
                log::info!("Found new data pack {pack_id}, loading it automatically");
                add(&mut selected, pack_id);
            } else {
                log::info!(
                    "Found new data pack {pack_id}, but can't load it due to missing features {}",
                    print_missing_flags(allowed_features, pack_features)
                );
            }
        }
        if is_selected && !pack_features.is_subset_of(allowed_features) {
            log::warn!(
                "Pack {pack_id} requires features {} that are not enabled for this world, disabling pack.",
                print_missing_flags(allowed_features, pack_features)
            );
            selected.retain(|s| s != pack_id);
        }
    }
    if selected.is_empty() {
        log::info!("No datapacks selected, forcing vanilla");
        selected.push("vanilla".to_owned());
    }
    configure_repository_with_selection(pack_repository, selected, forced_features, true)
}

fn configure_repository_with_selection(
    pack_repository: &mut PackRepository,
    selected: Vec<String>,
    forced_features: FeatureFlagSet,
    disable_inactive: bool,
) -> WorldDataConfiguration {
    pack_repository.set_selected(&selected);
    enable_forced_feature_packs(pack_repository, forced_features);
    let pack_config = get_selected_packs(pack_repository, disable_inactive);
    let pack_requested_features = pack_repository
        .requested_feature_flags()
        .join(forced_features);
    WorldDataConfiguration {
        data_packs: Some(pack_config),
        enabled_features: pack_requested_features,
    }
}

fn enable_forced_feature_packs(
    pack_repository: &mut PackRepository,
    forced_features: FeatureFlagSet,
) {
    let provided_features = pack_repository.requested_feature_flags();
    let mut missing_features = forced_features.subtract(provided_features);
    if missing_features.is_empty() {
        return;
    }
    let mut selected = pack_repository.selected_ids();
    let available: Vec<_> = pack_repository.available_packs().cloned().collect();
    for pack in &available {
        if missing_features.is_empty() {
            break;
        }
        if pack.pack_source() == PackSource::Feature {
            let pack_features = pack.requested_features();
            if !pack_features.is_empty()
                && pack_features.intersects(missing_features)
                && pack_features.is_subset_of(forced_features)
            {
                assert!(
                    !selected.iter().any(|s| s == pack.id()),
                    "Tried to force '{}', but it was already enabled",
                    pack.id()
                );
                selected.push(pack.id().to_owned());
                log::info!(
                    "Found feature pack ('{}') for requested feature, forcing to enabled",
                    pack.id()
                );
                missing_features = missing_features.subtract(pack_features);
            }
        }
    }
    pack_repository.set_selected(&selected);
}

/// `getSelectedPacks(packRepository, disableInactive)`: the `DataPacks` value for the current selection.
pub fn get_selected_packs(
    pack_repository: &PackRepository,
    disable_inactive: bool,
) -> DataPackConfig {
    let enabled = pack_repository.selected_ids();
    let selected: HashSet<&str> = enabled.iter().map(String::as_str).collect();
    let disabled = if disable_inactive {
        pack_repository
            .available_ids()
            .filter(|id| !selected.contains(id))
            .map(str::to_owned)
            .collect()
    } else {
        Vec::new()
    };
    DataPackConfig {
        enabled: enabled.clone(),
        disabled,
    }
}
