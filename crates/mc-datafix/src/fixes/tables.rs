//! The lookup tables of vanilla's data fixes (`static` maps, sets, lists and arrays of the fix and schema classes) as
//! data: `data/<class>.tsv.gz` is derived from the vanilla classes by `scripts/datafix-oracle.sh --regen` and never
//! edited. A table class is named by its snake case file stem (`item_id_fix`, `chunk_paletted_storage_fix_mapping_constants`).
//!
//! File format: a `# <Java class>` header line, then one line per entry, tab separated, every value typed SNBT:
//! `FIELD map KEY VALUE`, `FIELD set VALUE`, `FIELD list VALUE` (an empty value is a null element), `FIELD bits INDEX`
//! (a set bit of a `BitSet`), `FIELD value VALUE` (a single `Dynamic`), `FIELD default VALUE` (first line of a fastutil
//! map: its `defaultReturnValue`, what `get` returns for a missing key). Entries keep the Java iteration order, except
//! that `Map.of`/`Set.of` collections and collections keyed by `Dynamic`s (whose order changes with every JVM run) are
//! sorted, and so is every map nested inside a value. A `Pair` is `{l:..,r:..}`, a nested map a list of `{k:..,v:..}`,
//! a record a compound of its components, a `UUID` or `Identifier` its string.

use mc_nbt::{Tag, compress::gzip_decompress, snbt};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::OnceLock;

macro_rules! data_files {
    ($($name:literal),+ $(,)?) => {
        const FILES: &[(&str, &[u8])] = &[
            $(($name, include_bytes!(concat!("data/", $name, ".tsv.gz")))),+
        ];
    };
}

data_files!(
    "advancements_fix",
    "attribute_modifier_id_fix",
    "banner_pattern_format_fix",
    "biome_fix",
    "block_entity_id_fix",
    "block_state_data",
    "caves_and_cliffs_renames",
    "chunk_height_and_biome_fix",
    "chunk_paletted_storage_fix_mapping_constants",
    "chunk_structures_template_rename_fix",
    "dye_item_rename_fix",
    "entity_block_state_fix",
    "entity_health_fix",
    "entity_id_fix",
    "entity_the_renamening_fix",
    "entity_uuid_fix",
    "item_id_fix",
    "item_potion_fix",
    "item_shulker_box_color_fix",
    "item_spawn_egg_fix",
    "item_stack_componentization_fix",
    "item_stack_enchantment_names_fix",
    "item_stack_spawn_egg_fix",
    "item_stack_the_flattening_fix",
    "leaves_fix",
    "level_data_generator_options_fix",
    "mob_effect_id_fix",
    "options_key_lwjgl3_fix",
    "recipes_fix",
    "recipes_renamening_fix",
    "renamed_coral_fans_fix",
    "renamed_coral_fix",
    "scoreboard_display_slot_fix",
    "stats_counter_fix",
    "stonecutter_recipe_rename_fix",
    "structures_become_configured_fix",
    "tooltip_display_component_fix",
    "trial_spawner_config_in_registry_fix_vanilla_trial_chambers",
    "v704",
    "v705",
    "v99",
);

/// The file stems of every table, in the alphabetical order of `FILES`.
pub fn names() -> impl Iterator<Item = &'static str> {
    FILES.iter().map(|(name, _)| *name)
}

/// What a field of a table class holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Map,
    Set,
    List,
    Bits,
    Value,
}

/// One entry of a field: a map has a key and a value, a bit set its index as an int value, a list may hold a null.
#[derive(Debug)]
pub struct Entry {
    pub key: Option<Tag>,
    pub value: Option<Tag>,
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub kind: Kind,
    pub entries: Vec<Entry>,
    /// The `defaultReturnValue` of a fastutil map.
    pub default: Option<Tag>,
}

/// A parsed table class: its fields in name order.
#[derive(Debug)]
pub struct Table {
    /// The Java class name (`net.minecraft.util.datafix.fixes.ItemIdFix`).
    pub class_name: String,
    pub fields: Vec<Field>,
}

static LOADED: [OnceLock<Table>; FILES.len()] = [const { OnceLock::new() }; FILES.len()];

/// The table class with file stem `name`, parsed on first use.
///
/// # Panics
/// On an unknown name; a table file that does not parse fails the golden test, not a release.
pub fn table(name: &str) -> &'static Table {
    let index = FILES
        .iter()
        .position(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("no table class {name}"));
    LOADED[index].get_or_init(|| parse(name, FILES[index].1))
}

fn parse(name: &str, gz: &[u8]) -> Table {
    let bytes = gzip_decompress(gz, None).unwrap_or_else(|e| panic!("table {name}: {e}"));
    let text = String::from_utf8(bytes).unwrap_or_else(|e| panic!("table {name}: {e}"));
    let tag =
        |s: &str| snbt::parse(s).unwrap_or_else(|e| panic!("table {name}: {s}: {}", e.message()));
    let mut class_name = String::new();
    let mut fields: Vec<Field> = Vec::new();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix("# ") {
            class_name = header.to_string();
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let (kind, entry) = match (cols.get(1).copied(), cols.len()) {
            (Some("map"), 4) => (
                Kind::Map,
                Entry {
                    key: Some(tag(cols[2])),
                    value: Some(tag(cols[3])),
                },
            ),
            (Some("set"), 3) => (Kind::Set, value_entry(Some(tag(cols[2])))),
            (Some("list"), 3) => (
                Kind::List,
                value_entry((!cols[2].is_empty()).then(|| tag(cols[2]))),
            ),
            (Some("bits"), 3) => (
                Kind::Bits,
                value_entry(Some(Tag::Int(cols[2].parse().unwrap_or_else(|_| {
                    panic!("table {name}: bad bit index {}", cols[2])
                })))),
            ),
            (Some("value"), 3) => (Kind::Value, value_entry(Some(tag(cols[2])))),
            (Some("default"), 3) => {
                assert!(
                    fields.last().is_none_or(|f| f.name != cols[0]),
                    "table {name}: default of {} after its entries",
                    cols[0]
                );
                fields.push(Field {
                    name: cols[0].to_string(),
                    kind: Kind::Map,
                    entries: Vec::new(),
                    default: Some(tag(cols[2])),
                });
                continue;
            }
            _ => panic!("table {name}: malformed line {line}"),
        };
        match fields.last_mut() {
            Some(f) if f.name == cols[0] => {
                assert_eq!(f.kind, kind, "table {name}: field {} mixes kinds", f.name);
                f.entries.push(entry);
            }
            _ => fields.push(Field {
                name: cols[0].to_string(),
                kind,
                entries: vec![entry],
                default: None,
            }),
        }
    }
    Table { class_name, fields }
}

fn value_entry(value: Option<Tag>) -> Entry {
    Entry { key: None, value }
}

impl Table {
    /// # Panics
    /// When the class has no such field (or an empty one: the dump has no line for it).
    pub fn field(&self, name: &str) -> &Field {
        self.fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{} has no table field {name}", self.class_name))
    }
}

/// A Java `Map` as an ordered list of entries with lookup.
#[derive(Debug)]
pub struct OrderedMap<K, V> {
    entries: Vec<(K, V)>,
    index: HashMap<K, usize>,
    default: Option<V>,
}

/// A Java `Set` in Java iteration order.
pub type OrderedSet<K> = OrderedMap<K, ()>;

impl<K: Eq + Hash + Clone, V> OrderedMap<K, V> {
    fn new(entries: Vec<(K, V)>, default: Option<V>) -> Self {
        let index: HashMap<K, usize> = entries
            .iter()
            .enumerate()
            .map(|(i, (k, _))| (k.clone(), i))
            .collect();
        assert_eq!(index.len(), entries.len(), "duplicate key in a table");
        OrderedMap {
            entries,
            index,
            default,
        }
    }

    /// `get` of a fastutil map: the value, or its `defaultReturnValue` (`None` when that is null) for a missing key.
    pub fn get_or_default<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.get(key).or(self.default.as_ref())
    }

    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.index.get(key).map(|&i| &self.entries[i].1)
    }

    pub fn contains_key<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.index.contains_key(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<K: Eq + Hash + Clone> OrderedMap<K, ()> {
    pub fn contains<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.contains_key(key)
    }
}

fn string_of(t: &Tag) -> String {
    t.as_string()
        .unwrap_or_else(|| panic!("table value {t} is not a string"))
        .to_string()
}

fn int_of(t: &Tag) -> i32 {
    match t {
        Tag::Int(i) => *i,
        _ => panic!("table value {t} is not an int"),
    }
}

impl Field {
    fn pairs<K: Eq + Hash + Clone, V>(
        &self,
        key: impl Fn(&Tag) -> K,
        value: impl Fn(&Tag) -> V,
    ) -> OrderedMap<K, V> {
        assert_eq!(self.kind, Kind::Map, "{} is not a map", self.name);
        OrderedMap::new(
            self.entries
                .iter()
                .map(|e| {
                    (
                        key(e.key.as_ref().expect("map key")),
                        value(e.value.as_ref().expect("map value")),
                    )
                })
                .collect(),
            self.default.as_ref().map(value),
        )
    }

    /// `Map<String, String>`.
    pub fn string_map(&self) -> OrderedMap<String, String> {
        self.pairs(string_of, string_of)
    }

    /// `Int2ObjectMap<String>` / `Map<Integer, String>`.
    pub fn int_string_map(&self) -> OrderedMap<i32, String> {
        self.pairs(int_of, string_of)
    }

    /// `Map<String, Integer>` / `Object2IntMap<String>`.
    pub fn string_int_map(&self) -> OrderedMap<String, i32> {
        self.pairs(string_of, int_of)
    }

    /// `Set<String>`.
    pub fn string_set(&self) -> OrderedSet<String> {
        assert_eq!(self.kind, Kind::Set, "{} is not a set", self.name);
        OrderedMap::new(
            self.entries
                .iter()
                .map(|e| (string_of(e.value.as_ref().expect("set value")), ()))
                .collect(),
            None,
        )
    }

    /// `List<String>` / `String[]`; a null element is `None`.
    pub fn string_list(&self) -> Vec<Option<String>> {
        assert_eq!(self.kind, Kind::List, "{} is not a list", self.name);
        self.entries
            .iter()
            .map(|e| e.value.as_ref().map(string_of))
            .collect()
    }

    /// The set bits of a `BitSet`, ascending.
    pub fn bits(&self) -> Vec<usize> {
        assert_eq!(self.kind, Kind::Bits, "{} is not a bit set", self.name);
        self.entries
            .iter()
            .map(|e| int_of(e.value.as_ref().expect("bit")) as usize)
            .collect()
    }
}

/// `ItemIdFix.getItem`: the table of `ItemIdFix`.
pub mod item_id_fix {
    use super::{OrderedMap, table};
    use std::sync::LazyLock;

    static ITEM_NAMES: LazyLock<OrderedMap<i32, String>> =
        LazyLock::new(|| table("item_id_fix").field("ITEM_NAMES").int_string_map());

    /// `ItemIdFix.getItem(id)`: the item name of a legacy numeric id, `minecraft:air` for an unknown one.
    pub fn get_item(id: i32) -> &'static str {
        ITEM_NAMES
            .get_or_default(&id)
            .expect("ITEM_NAMES has a default return value")
    }
}

/// `ItemStackTheFlatteningFix.updateItem` and its `DAMAGE_IDS`.
pub mod item_stack_the_flattening_fix {
    use super::{OrderedMap, OrderedSet, table};
    use std::sync::LazyLock;

    struct Tables {
        map: OrderedMap<String, String>,
        ids: OrderedSet<String>,
        damage_ids: OrderedSet<String>,
    }

    static TABLES: LazyLock<Tables> = LazyLock::new(|| {
        let t = table("item_stack_the_flattening_fix");
        Tables {
            map: t.field("MAP").string_map(),
            ids: t.field("IDS").string_set(),
            damage_ids: t.field("DAMAGE_IDS").string_set(),
        }
    });

    /// `ItemStackTheFlatteningFix.updateItem(name, data)`: the flattened item name, `None` where Java returns null.
    pub fn update_item(name: &str, data: i32) -> Option<&'static str> {
        let t = &*TABLES;
        if t.ids.contains(name) {
            t.map
                .get(format!("{name}.{data}").as_str())
                .or_else(|| t.map.get(format!("{name}.0").as_str()))
                .map(String::as_str)
        } else {
            None
        }
    }

    /// `DAMAGE_IDS.contains(name)`: items whose `Damage` moves into the `tag`.
    pub fn is_damage_id(name: &str) -> bool {
        TABLES.damage_ids.contains(name)
    }
}

/// `EntityBlockStateFix.getBlockId`.
pub mod entity_block_state_fix {
    use super::{OrderedMap, table};
    use std::sync::LazyLock;

    static MAP: LazyLock<OrderedMap<String, i32>> = LazyLock::new(|| {
        table("entity_block_state_fix")
            .field("MAP")
            .string_int_map()
    });

    /// `EntityBlockStateFix.getBlockId(name)`: the legacy numeric block id, 0 when unknown.
    pub fn get_block_id(name: &str) -> i32 {
        MAP.get(name).copied().unwrap_or(0)
    }
}
