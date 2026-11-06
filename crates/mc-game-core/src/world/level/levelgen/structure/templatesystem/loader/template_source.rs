//! `TemplateSource`.
//!
//! The `DataFixer` upgrade of `readStructure(tag)` is not ported: a template written by an older data version is read
//! as if it were current (a warning is logged). Every template of the vanilla data pack is at the current version.
//! SNBT sources (`loadAsText`, only used for the development test-structure directory) are not supported.

use crate::world::level::levelgen::structure::templatesystem::structure_template::{
    CURRENT_DATA_VERSION, StructureTemplate,
};
use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, NbtAccounter};

pub trait TemplateSource: Send + Sync {
    /// `load(id)`.
    fn load(&self, id: &Identifier) -> Option<StructureTemplate>;

    /// `list()`.
    fn list(&self) -> Vec<Identifier>;
}

/// `TemplateSource.load(opener, asText = false, onError)` once the bytes are read: `readCompressed` + `readStructure`.
/// Malformed data is reported through `on_error` and yields `None`.
pub(super) fn load_from_bytes(
    bytes: &[u8],
    on_error: impl FnOnce(String),
) -> Option<StructureTemplate> {
    match mc_nbt::nbt_io::read_compressed(bytes, &mut NbtAccounter::unlimited_heap()) {
        Ok(tag) => Some(read_structure(&tag)),
        Err(e) => {
            on_error(e.to_string());
            None
        }
    }
}

/// `readStructure(tag)`.
pub fn read_structure(tag: &CompoundTag) -> StructureTemplate {
    let version = mc_nbt::nbt_utils::get_data_version_or(tag, 500);
    if version < CURRENT_DATA_VERSION {
        log::warn!(
            "Structure template has data version {version} (current {CURRENT_DATA_VERSION}); data fixing is not supported, loading as is"
        );
    }
    let mut template = StructureTemplate::new();
    template.load(tag);
    template
}
