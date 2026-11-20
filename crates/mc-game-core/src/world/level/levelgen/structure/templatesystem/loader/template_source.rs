//! `TemplateSource`.
//!
//! `readStructure(tag)` upgrades a template through `DataFixTypes.STRUCTURE` from its `DataVersion` (500 when absent).
//! SNBT sources (`loadAsText`, only used for the development test-structure directory) are not supported.

use crate::world::level::levelgen::structure::templatesystem::structure_template::StructureTemplate;
use mc_core::ident::Identifier;
use mc_datafix::{DataFixTypes, update_to_current};
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
        Ok(tag) => match read_structure(&tag) {
            Ok(template) => Some(template),
            Err(e) => {
                on_error(e);
                None
            }
        },
        Err(e) => {
            on_error(e.to_string());
            None
        }
    }
}

/// `readStructure(tag)`: the template of the tag upgraded to the current version.
pub fn read_structure(tag: &CompoundTag) -> Result<StructureTemplate, String> {
    let version = mc_nbt::nbt_utils::get_data_version_or(tag, 500);
    let fixed = update_to_current(DataFixTypes::STRUCTURE, tag, version);
    let mut template = StructureTemplate::new();
    template.load(&fixed);
    Ok(template)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_data::block::Block;

    /// A one-block template whose palette entry is `Name:` (the field name before data version 5006).
    fn template(version: i32) -> Vec<u8> {
        let mut tag = mc_nbt::snbt::parse_compound(
            "{size:[1,1,1],palette:[{Name:\"minecraft:stone\"}],blocks:[{state:0,pos:[0,0,0]}],entities:[]}",
        )
        .unwrap();
        tag.put_int("DataVersion", version);
        mc_nbt::nbt_io::write_compressed(&tag)
    }

    fn load(bytes: &[u8]) -> (Option<StructureTemplate>, Vec<String>) {
        let mut errors = Vec::new();
        let loaded = load_from_bytes(bytes, |e| errors.push(e));
        (loaded, errors)
    }

    #[test]
    fn an_older_template_is_upgraded_before_it_is_read() {
        let (loaded, errors) = load(&template(5005));
        assert!(errors.is_empty(), "{errors:?}");
        let palettes = loaded.unwrap();
        let blocks = palettes.palettes()[0].blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].state, Block::STONE.default_state());
    }
}
