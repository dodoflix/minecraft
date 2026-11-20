//! `LeavesFix`: gives every leaf block of a chunk its `persistent` and `distance` block state (the distance to the
//! nearest log, at most 7) and records on `UpgradeData.Sides` which chunk borders hold leaves.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::fixes::tables::{OrderedMap, OrderedSet, table};
use crate::packed_bit_storage::PackedBitStorage;
use crate::schema::Schema;
use crate::ty::Type;
use crate::typed::Typed;
use crate::value::Value;
use mc_core::java_collections::{FastOpenHashMap, hash_map_ordered};
use mc_core::math::java::parse_int;
use mc_core::math::mth::ceillog2;
use mc_dfu::Dynamic;
use mc_nbt::Tag;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

const SIZE: i32 = 4096;
const DECAY_DISTANCE: i32 = 7;

static DIRECTIONS: LazyLock<Vec<[i32; 3]>> = LazyLock::new(|| {
    table("leaves_fix")
        .field("DIRECTIONS")
        .entries
        .iter()
        .map(|e| {
            let list = e
                .value
                .as_ref()
                .and_then(Tag::as_list)
                .expect("direction list");
            [
                list.get_int(0).expect("x"),
                list.get_int(1).expect("y"),
                list.get_int(2).expect("z"),
            ]
        })
        .collect()
});

static LEAVES: LazyLock<OrderedMap<String, i32>> =
    LazyLock::new(|| table("leaves_fix").field("LEAVES").string_int_map());

static LOGS: LazyLock<OrderedSet<String>> =
    LazyLock::new(|| table("leaves_fix").field("LOGS").string_set());

/// `new IntOpenHashSet()`.
fn new_int_set() -> FastOpenHashMap<i32, ()> {
    FastOpenHashMap::with_null_key(16, |k| *k, |k| *k == 0)
}

/// `getIndex(x, y, z)`.
pub fn get_index(x: i32, y: i32, z: i32) -> i32 {
    y << 8 | z << 4 | x
}

fn get_x(index: i32) -> i32 {
    index & 15
}

fn get_y(index: i32) -> i32 {
    index >> 8 & 0xFF
}

fn get_z(index: i32) -> i32 {
    index >> 4 & 15
}

/// `getSideMask(west, east, north, south)`.
pub fn get_side_mask(west: bool, east: bool, north: bool, south: bool) -> i32 {
    let mut s = 0;
    if north {
        if east {
            s |= 2;
        } else if west {
            s |= 128;
        } else {
            s |= 1;
        }
    } else if south {
        if west {
            s |= 32;
        } else if east {
            s |= 8;
        } else {
            s |= 16;
        }
    } else if east {
        s |= 4;
    } else if west {
        s |= 64;
    }
    s
}

/// What `Section` keeps per fix: the input schema, the block state type it must equal and the `Palette` finder.
pub(crate) struct Ctx {
    schema: Schema,
    block_state_type: Type,
    palette_finder: OpticFinder,
}

impl Ctx {
    pub(crate) fn new(schema: Schema) -> Ctx {
        let block_state_type =
            DSL::named_type(References::BLOCK_STATE.type_name(), &DSL::remainder_type());
        Ctx {
            palette_finder: DSL::field_finder("Palette", &DSL::list_type(&block_state_type)),
            block_state_type,
            schema,
        }
    }

    pub(crate) fn schema(&self) -> &Schema {
        &self.schema
    }
}

/// `LeavesFix.Section`: the palette, the index and the packed block states of a chunk section.
pub(crate) struct Section {
    palette: Vec<Dynamic>,
    pub(crate) index: i32,
    storage: Option<PackedBitStorage>,
}

impl Section {
    /// `new Section(section, inputSchema)`; `skippable` is the subclass's `skippable()`, which `readStorage` calls
    /// with the palette (it may rewrite it).
    pub(crate) fn new(
        section: &Typed,
        ctx: &Ctx,
        skippable: impl FnOnce(&mut Vec<Dynamic>) -> bool,
    ) -> Section {
        if ctx.schema.get_type(&References::BLOCK_STATE) != ctx.block_state_type {
            panic!("java.lang.IllegalStateException: Block state type is not what was expected.");
        }
        let mut palette: Vec<Dynamic> = section
            .get_optional(&ctx.palette_finder)
            .map(|p| {
                p.as_list()
                    .iter()
                    .map(|b| b.second().as_dynamic().clone())
                    .collect()
            })
            .unwrap_or_default();
        let tag = section.get_remainder();
        // `readStorage(tag)`
        let storage = if skippable(&mut palette) {
            None
        } else {
            let states = tag.get("BlockStates").as_long_stream();
            let size = ceillog2(palette.len() as i32).max(4);
            Some(PackedBitStorage::from_data(size, SIZE, states))
        };
        Section {
            palette,
            index: tag.get("Y").as_int_or(0),
            storage,
        }
    }

    pub(crate) fn is_skippable(&self) -> bool {
        self.storage.is_none()
    }

    pub(crate) fn get_block(&self, pos: i32) -> i32 {
        self.storage
            .as_ref()
            .expect("java.lang.NullPointerException")
            .get(pos)
    }

    /// `write(section)`.
    fn write(&self, ctx: &Ctx, section: Typed) -> Typed {
        let Some(storage) = &self.storage else {
            return section;
        };
        section
            .update_remainder(|tag| {
                tag.set("BlockStates", tag.create_long_list(storage.raw().to_vec()))
            })
            .set(
                &ctx.palette_finder,
                Value::List(
                    self.palette
                        .iter()
                        .map(|b| {
                            Value::named(
                                References::BLOCK_STATE.type_name(),
                                Value::Dynamic(b.clone()),
                            )
                        })
                        .collect(),
                ),
            )
    }
}

/// `LeavesFix.LeavesSection`.
struct LeavesSection {
    base: Section,
    leave_ids: HashSet<i32>,
    log_ids: HashSet<i32>,
    state_to_id_map: HashMap<i32, i32>,
}

impl LeavesSection {
    /// `new LeavesSection(section, inputSchema)`.
    fn new(section: &Typed, ctx: &Ctx) -> LeavesSection {
        let mut leave_ids = HashSet::new();
        let mut log_ids = HashSet::new();
        let mut state_to_id_map = HashMap::new();
        // `skippable()`: rewrites the leaf palette entries and collects the leaf and log ids
        let base = Section::new(section, ctx, |palette| {
            for (i, entry) in palette.iter_mut().enumerate() {
                let palette_tag = entry.clone();
                let block_name = palette_tag.get("Name").as_string_or("");
                if LEAVES.contains_key(block_name.as_lossy_str()) {
                    let persistent = palette_tag
                        .get("Properties")
                        .get("decayable")
                        .as_string_or("")
                        == "false";
                    leave_ids.insert(i as i32);
                    state_to_id_map.insert(
                        get_state_id(block_name.as_lossy_str(), persistent, DECAY_DISTANCE),
                        i as i32,
                    );
                    *entry = make_leaf_tag(
                        &palette_tag,
                        block_name.as_lossy_str(),
                        persistent,
                        DECAY_DISTANCE,
                    );
                }
                if LOGS.contains(block_name.as_lossy_str()) {
                    log_ids.insert(i as i32);
                }
            }
            leave_ids.is_empty() && log_ids.is_empty()
        });
        LeavesSection {
            base,
            leave_ids,
            log_ids,
            state_to_id_map,
        }
    }

    fn is_log(&self, block: i32) -> bool {
        self.log_ids.contains(&block)
    }

    fn is_leaf(&self, block: i32) -> bool {
        self.leave_ids.contains(&block)
    }

    /// `getDistance(block)`.
    fn get_distance(&self, block: i32) -> i32 {
        if self.is_log(block) {
            return 0;
        }
        let distance = self.base.palette[block as usize]
            .get("Properties")
            .get("distance")
            .as_string_or("");
        parse_int(distance.as_lossy_str()).unwrap_or_else(|| {
            panic!(
                "java.lang.NumberFormatException: For input string: \"{}\"",
                distance.as_lossy_str()
            )
        })
    }

    /// `setDistance(pos, block, distance)`.
    fn set_distance(&mut self, pos: i32, block: i32, distance: i32) {
        let base_tag = self.base.palette[block as usize].clone();
        let block_name = base_tag.get("Name").as_string_or("");
        let persistent = base_tag
            .get("Properties")
            .get("persistent")
            .as_string_or("")
            == "true";
        let state_id = get_state_id(block_name.as_lossy_str(), persistent, distance);
        if !self.state_to_id_map.contains_key(&state_id) {
            let id = self.base.palette.len() as i32;
            self.leave_ids.insert(id);
            self.state_to_id_map.insert(state_id, id);
            self.base.palette.push(make_leaf_tag(
                &base_tag,
                block_name.as_lossy_str(),
                persistent,
                distance,
            ));
        }
        let id = self.state_to_id_map[&state_id];
        let storage = self
            .base
            .storage
            .as_mut()
            .expect("java.lang.NullPointerException");
        if 1i32.wrapping_shl(storage.bits() as u32) <= id {
            let mut new_storage = PackedBitStorage::new(storage.bits() + 1, SIZE);
            for i in 0..SIZE {
                new_storage.set(i, storage.get(i));
            }
            *storage = new_storage;
        }
        storage.set(pos, id);
    }
}

/// `Section.getStateId(blockName, persistent, distance)`.
fn get_state_id(block_name: &str, persistent: bool, distance: i32) -> i32 {
    LEAVES.get_or_default(block_name).copied().unwrap_or(0) << 5
        | if persistent { 16 } else { 0 }
        | distance
}

/// `makeLeafTag(input, blockName, persistent, distance)`.
fn make_leaf_tag(input: &Dynamic, block_name: &str, persistent: bool, distance: i32) -> Dynamic {
    let mut properties = input.empty_map();
    properties = properties.set(
        "persistent",
        properties.create_string(if persistent { "true" } else { "false" }),
    );
    properties = properties.set("distance", properties.create_string(distance.to_string()));
    let mut tag = input.empty_map();
    tag = tag.set("Properties", properties);
    tag.set("Name", tag.create_string(block_name))
}

/// The `sections` lambda of `makeRule()`: propagates the log distance through the leaves of all sections.
fn fix_sections(
    ctx: &Ctx,
    section_finder: &OpticFinder,
    sections: Typed,
    sides: &mut i32,
) -> Typed {
    // `Collectors.toMap` into a `HashMap`, copied into an `Int2ObjectOpenHashMap`
    let mut by_index: Vec<LeavesSection> = Vec::new();
    for section in sections.get_all_typed(section_finder) {
        let section = LeavesSection::new(&section, ctx);
        if by_index.iter().any(|s| s.base.index == section.base.index) {
            panic!(
                "java.lang.IllegalStateException: Duplicate key {} (attempted merging values)",
                section.base.index
            );
        }
        by_index.push(section);
    }
    let ordered = hash_map_ordered(by_index, |s| s.base.index, 16);
    let mut section_map: FastOpenHashMap<i32, LeavesSection> =
        FastOpenHashMap::with_null_key(ordered.len(), |k| *k, |k| *k == 0);
    section_map.put_all(ordered.into_iter().map(|s| (s.base.index, s)).collect());

    if section_map.iter().all(|(_, s)| s.base.is_skippable()) {
        return sections;
    }

    let mut queue: Vec<FastOpenHashMap<i32, ()>> = (0..7).map(|_| new_int_set()).collect();

    for (_, section) in section_map.iter() {
        if !section.base.is_skippable() {
            for i in 0..SIZE {
                let block = section.base.get_block(i);
                if section.is_log(block) {
                    queue[0].put(section.base.index << 12 | i, ());
                } else if section.is_leaf(block) {
                    let x = get_x(i);
                    let z = get_z(i);
                    *sides |= get_side_mask(x == 0, x == 15, z == 0, z == 15);
                }
            }
        }
    }

    for ix in 1..7 {
        let (before, after) = queue.split_at_mut(ix);
        let set = &before[ix - 1];
        let new_set = &mut after[0];
        for (&pos_chunk, _) in set.iter() {
            let x = get_x(pos_chunk);
            let y = get_y(pos_chunk);
            let z = get_z(pos_chunk);
            for direction in DIRECTIONS.iter() {
                let nx = x + direction[0];
                let ny_chunk = y + direction[1];
                let nz = z + direction[2];
                if (0..=15).contains(&nx) && (0..=15).contains(&nz) && (0..=255).contains(&ny_chunk)
                {
                    let Some(section) = section_map.get_mut(&(ny_chunk >> 4)) else {
                        continue;
                    };
                    if section.base.is_skippable() {
                        continue;
                    }
                    let pos_section = get_index(nx, ny_chunk & 15, nz);
                    let block = section.base.get_block(pos_section);
                    if section.is_leaf(block) {
                        let old_distance = section.get_distance(block);
                        if old_distance > ix as i32 {
                            section.set_distance(pos_section, block, ix as i32);
                            new_set.put(get_index(nx, ny_chunk, nz), ());
                        }
                    }
                }
            }
        }
    }

    sections.update_typed(section_finder, |section| {
        let y = section.get_remainder().get("Y").as_int_or(0);
        section_map
            .get(&y)
            .expect("java.lang.NullPointerException")
            .base
            .write(ctx, section)
    })
}

/// `new LeavesFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("LeavesFix", output_schema, changes_type, |df| {
        let input_schema = df.input_schema();
        let chunk_type = input_schema.get_type(&References::CHUNK);
        let level_finder = chunk_type.find_field("Level");
        let sections_finder = level_finder.ty().find_field("Sections");
        let Some(section_type) = sections_finder.ty().as_list().cloned() else {
            panic!("java.lang.IllegalStateException: Expecting sections to be a list.");
        };
        let section_finder = DSL::type_finder(&section_type);
        let ctx = Ctx::new(input_schema);
        df.fix_type_everywhere_typed("Leaves fix", &chunk_type, move |chunk| {
            chunk.update_typed(&level_finder, |level| {
                let mut sides = 0i32;
                let mut new_level = level.update_typed(&sections_finder, |sections| {
                    fix_sections(&ctx, &section_finder, sections, &mut sides)
                });
                if sides != 0 {
                    new_level = new_level.update_remainder(|tag| {
                        let upgrade_data = tag
                            .get("UpgradeData")
                            .result()
                            .unwrap_or_else(|| tag.empty_map());
                        let old_sides = i32::from(upgrade_data.get("Sides").as_byte_or(0));
                        tag.set(
                            "UpgradeData",
                            upgrade_data.set("Sides", tag.create_byte((old_sides | sides) as i8)),
                        )
                    });
                }
                new_level
            })
        })
    })
}
