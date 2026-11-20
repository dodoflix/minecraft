//! `BlockStateData`: the 4 096 legacy `id << 4 | data` slots and their block states (`MAP`), the legacy block states
//! and names that map back to a slot (`ID_BY_OLD`, `ID_BY_OLD_NAME`). The tables are derived from the vanilla class
//! (`fixes/data/block_state_data.tsv.gz`, `scripts/datafix-oracle.sh --regen`); only the accessors are code.
//!
//! The Java accessors take and return `Dynamic<?>` over `NbtOps`; these take and return the tag.

use super::tables::table;
use mc_nbt::Tag;
use std::collections::HashMap;
use std::sync::LazyLock;

/// `BlockStateData.FILTER_ME`: the name that marks a state to be replaced by air.
pub const FILTER_ME: &str = "%%FILTER_ME%%";

/// A legacy block state `{Name, Properties?}` in a form that is `Eq + Hash` (the `Dynamic` key of `ID_BY_OLD`; compound
/// equality ignores key order).
#[derive(PartialEq, Eq, Hash)]
struct LegacyKey {
    name: String,
    properties: Option<Vec<(String, String)>>,
}

impl LegacyKey {
    /// `None` for a tag that is not a compound of a string `Name` and optionally string `Properties`: no key of
    /// `ID_BY_OLD` is equal to it.
    fn of(tag: &Tag) -> Option<LegacyKey> {
        let compound = tag.as_compound()?;
        let name = compound.get("Name")?.as_string()?.to_string();
        let properties = match compound.get("Properties") {
            None => None,
            Some(p) => {
                let mut v = Vec::new();
                for (k, value) in p.as_compound()?.iter() {
                    v.push((k.clone(), value.as_string()?.to_string()));
                }
                v.sort();
                Some(v)
            }
        };
        let extra = compound.len() - 1 - usize::from(properties.is_some());
        (extra == 0).then_some(LegacyKey { name, properties })
    }
}

struct Data {
    /// `MAP` after `finalizeMaps`: a slot is `None` where no block of its id is registered.
    map: Vec<Option<Tag>>,
    /// Per slot the lowest slot that holds the identical `Dynamic` in vanilla (an unregistered data value shares the
    /// object of its block's first registered state), `-1` for a `None` slot.
    first: Vec<i32>,
    id_by_old: HashMap<LegacyKey, i32>,
    id_by_old_name: HashMap<String, i32>,
}

static DATA: LazyLock<Data> = LazyLock::new(|| {
    let t = table("block_state_data");
    let map: Vec<Option<Tag>> = t
        .field("MAP")
        .entries
        .iter()
        .map(|e| e.value.clone())
        .collect();
    assert_eq!(map.len(), 4096, "BlockStateData.MAP has 4096 slots");
    let first: Vec<i32> = t
        .field("MAP_FIRST")
        .entries
        .iter()
        .map(|e| e.value.as_ref().and_then(Tag::as_int).expect("slot index"))
        .collect();
    assert_eq!(first.len(), 4096, "one identity slot per map slot");
    let old_states = &t.field("ID_BY_OLD").entries;
    let id_by_old: HashMap<LegacyKey, i32> = old_states
        .iter()
        .map(|e| {
            let key = LegacyKey::of(e.key.as_ref().expect("key")).expect("legacy block state");
            (key, e.value.as_ref().and_then(Tag::as_int).expect("id"))
        })
        .collect();
    assert_eq!(id_by_old.len(), old_states.len(), "duplicate legacy state");
    let id_by_old_name = t
        .field("ID_BY_OLD_NAME")
        .string_int_map()
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    Data {
        map,
        first,
        id_by_old,
        id_by_old_name,
    }
});

fn slot(id: i32) -> Option<&'static Tag> {
    let i = usize::try_from(id).ok()?;
    let first = usize::try_from(*DATA.first.get(i)?).ok()?;
    DATA.map[first].as_ref()
}

fn name_of(tag: &Tag) -> &str {
    tag.as_compound()
        .and_then(|c| c.get_string("Name"))
        .unwrap_or("")
}

/// `upgradeBlockStateTag(oldTag)`: the new block state of a legacy one, `old_tag` itself when none is registered.
/// Vanilla's lookup only hits a `Dynamic` over `NbtOps` (`Dynamic.equals` compares the ops): callers pass the tag of
/// such a `Dynamic`.
pub fn upgrade_block_state_tag(old_tag: &Tag) -> &Tag {
    LegacyKey::of(old_tag)
        .and_then(|k| DATA.id_by_old.get(&k))
        .and_then(|&id| slot(id))
        .unwrap_or(old_tag)
}

/// `upgradeBlock(oldName)`: the new block name of a legacy block name, `old_name` itself when none is registered.
pub fn upgrade_block(old_name: &str) -> &str {
    DATA.id_by_old_name
        .get(old_name)
        .and_then(|&id| slot(id))
        .map_or(old_name, name_of)
}

/// `upgradeBlock(id)`: the block name of a legacy slot, `minecraft:air` for an unregistered or out of range one.
pub fn upgrade_block_id(id: i32) -> &'static str {
    slot(id).map_or("minecraft:air", name_of)
}

/// `getTag(id)`: the block state of a legacy slot, the one of slot 0 (air) for an unregistered or out of range one.
/// Slots that vanilla fills with the same object return the same reference: two states are the same palette entry of
/// `ChunkPalettedStorageFix` only when they are the same object.
pub fn get_tag(id: i32) -> &'static Tag {
    slot(id).unwrap_or_else(|| slot(0).expect("slot 0 is registered"))
}
