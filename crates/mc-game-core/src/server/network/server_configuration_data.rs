//! Builds the configuration-phase snapshot (`ConfigurationData`) from the live server: what
//! `ServerConfigurationPacketListenerImpl.startConfiguration` reads (known packs, enabled features, registries/tags,
//! code of conduct, server resource pack). The registry data and tags are packed once per snapshot, for the two cases
//! vanilla distinguishes (the client accepted the offered packs, or not).

use super::hooks::{ConfigurationData, RegistrySynchronization, ServerResourcePackInfo};
use crate::registry::synchronization::{self as sync, PackedRegistryEntry};
use crate::server::MinecraftServer;
use crate::server::packs::KnownPack;
use crate::server::registry_layer::RegistryLayer;
use indexmap::IndexMap;
use mc_core::ident::Identifier;
use mc_protocol::packets::common::TagNetworkPayload;
use mc_protocol::packets::configuration as cfg;
use std::collections::HashSet;
use std::sync::Arc;

fn protocol_pack(pack: &KnownPack) -> cfg::KnownPack {
    cfg::KnownPack {
        namespace: pack.namespace.clone(),
        id: pack.id.clone(),
        version: pack.version.clone(),
    }
}

/// `packRegistries` + `serializeTagsToNetwork` pre-encoded as packets.
struct PackedRegistries {
    requested: HashSet<cfg::KnownPack>,
    /// Packets when the client knows `requested`.
    known: Vec<cfg::ClientboundRegistryDataPacket>,
    /// Packets when it knows nothing.
    unknown: Vec<cfg::ClientboundRegistryDataPacket>,
    tags: cfg::ClientboundUpdateTagsPacket,
}

fn registry_packets(
    server: &MinecraftServer,
    negotiated: &HashSet<KnownPack>,
) -> Vec<cfg::ClientboundRegistryDataPacket> {
    let access = server.registries.get_access_from(RegistryLayer::World);
    sync::pack_registries(&access, negotiated)
        .into_iter()
        .map(|(registry, entries)| cfg::ClientboundRegistryDataPacket {
            registry: Identifier::parse(&registry)
                .unwrap_or_else(|e| panic!("invalid registry key {registry}: {e}")),
            entries: entries
                .into_iter()
                .map(|PackedRegistryEntry { id, data }| cfg::PackedRegistryEntry { id, data })
                .collect(),
        })
        .collect()
}

impl RegistrySynchronization for PackedRegistries {
    fn pack_registries(
        &self,
        negotiated: &HashSet<cfg::KnownPack>,
    ) -> Vec<cfg::ClientboundRegistryDataPacket> {
        if negotiated.is_empty() {
            self.unknown.clone()
        } else if *negotiated == self.requested {
            self.known.clone()
        } else {
            // `handleResponse` only ever passes the requested packs or none.
            unreachable!(
                "registry synchronization is only negotiated with all or none of the offered packs"
            )
        }
    }

    fn tags(&self) -> cfg::ClientboundUpdateTagsPacket {
        self.tags.clone()
    }
}

/// Snapshot of the server's current registries, packs and settings for the configuration phase.
pub fn configuration_data(server: &MinecraftServer) -> ConfigurationData {
    let packs: Vec<KnownPack> = {
        use crate::server::packs::{PackResources, ResourceManager};
        server
            .resources
            .resource_manager
            .list_packs()
            .iter()
            .filter_map(|p| PackResources::known_pack_info(&**p).cloned())
            .collect()
    };
    let requested: HashSet<KnownPack> = packs.iter().cloned().collect();
    let tags = cfg::ClientboundUpdateTagsPacket {
        tags: sync::serialize_tags_to_network(&server.registries)
            .into_iter()
            .map(|(registry, payload)| {
                (
                    Identifier::parse(&registry)
                        .unwrap_or_else(|e| panic!("invalid registry key {registry}: {e}")),
                    TagNetworkPayload {
                        tags: payload
                            .tags
                            .into_iter()
                            .map(|(id, ids)| (id, ids.into_iter().map(|i| i as i32).collect()))
                            .collect(),
                    },
                )
            })
            .collect(),
    };
    let registries = PackedRegistries {
        requested: requested.iter().map(protocol_pack).collect(),
        known: registry_packets(server, &requested),
        unknown: registry_packets(server, &HashSet::new()),
        tags,
    };
    let settings = server.dedicated.settings.get_properties();
    ConfigurationData {
        known_packs: packs.iter().map(protocol_pack).collect(),
        enabled_features: server
            .world_data
            .lock()
            .settings
            .data_configuration
            .enabled_features
            .names(),
        // `serverLinks()`: the dedicated server's link list is not available yet (empty = no packet).
        server_links: Vec::new(),
        code_of_conducts: server
            .dedicated
            .code_of_conduct_texts
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<IndexMap<_, _>>(),
        resource_pack: settings.server_resource_pack_info.as_ref().map(|p| {
            ServerResourcePackInfo {
                id: p.id,
                url: p.url.clone(),
                hash: p.hash.clone(),
                is_required: p.is_required,
                prompt: p.prompt.clone(),
            }
        }),
        registries: Arc::new(registries),
    }
}
