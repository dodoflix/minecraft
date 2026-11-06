//! `net.minecraft.server.RegistryLayer`: the ordered layers of a server's registries.

use std::sync::LazyLock;

use crate::registry::{LayeredRegistryAccess, RegistryAccess, static_registries};

/// `RegistryLayer`: static (code-defined) -> world (worldgen etc. from the data packs) -> dimensions -> reloadable.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RegistryLayer {
    Static,
    World,
    Dimensions,
    Reloadable,
}

impl RegistryLayer {
    pub const VALUES: [RegistryLayer; 4] = [
        RegistryLayer::Static,
        RegistryLayer::World,
        RegistryLayer::Dimensions,
        RegistryLayer::Reloadable,
    ];

    /// `RegistryLayer.createRegistryAccess()`: all layers empty except `Static`.
    pub fn create_registry_access() -> LayeredRegistryAccess<RegistryLayer> {
        static STATIC_ACCESS: LazyLock<RegistryAccess> = LazyLock::new(static_registries);
        LayeredRegistryAccess::new(RegistryLayer::VALUES.to_vec())
            .replace_from(RegistryLayer::Static, vec![STATIC_ACCESS.clone()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_layer_has_all_builtin_registries() {
        let layers = RegistryLayer::create_registry_access();
        assert_eq!(
            layers.get_layer(RegistryLayer::Static).registries().count(),
            mc_data::registry::registries::ALL.len()
        );
        assert_eq!(
            layers.get_layer(RegistryLayer::World).registries().count(),
            0
        );
        assert!(
            layers
                .composite_access()
                .registry("minecraft:block")
                .is_some()
        );
        assert!(
            layers
                .get_access_for_loading(RegistryLayer::Static)
                .registries()
                .next()
                .is_none()
        );
    }
}
