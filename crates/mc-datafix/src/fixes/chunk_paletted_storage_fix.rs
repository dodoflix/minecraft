//! `ChunkPalettedStorageFix`: converts the `Blocks`/`Data`/`Add` byte arrays of every chunk section into a block state
//! palette and packed `BlockStates`, flattening the legacy `id << 4 | data` through `BlockStateData` and fixing the
//! states that depend on their neighbours or on a block entity (snowy grass, doors, beds, banners, skulls, flower pots,
//! note blocks, double plants, pumpkins).
//!
//! Vanilla compares block states by identity (an identity hash set and `CrudeIncrementalIntIdentityHashBiMap` in the
//! sections): a state is a `&'static Tag` here and its address is its identity; `BlockStateData::get_tag` hands out
//! the same reference for the slots that share an object in vanilla.

use crate::datafix::DataFix;
use crate::fixes::block_state_data::{self, FILTER_ME};
use crate::fixes::references::References;
use crate::fixes::tables::table;
use crate::packed_bit_storage::PackedBitStorage;
use mc_core::math::mth::ceillog2;
use mc_dfu::{Dyn, Dynamic, NBT};
use mc_nbt::Tag;
use std::collections::HashMap;
use std::sync::LazyLock;

type State = &'static Tag;

const SIZE: usize = 4096;

struct Constants {
    fix: [bool; 256],
    virtual_: [bool; 256],
    air: State,
    pumpkin: State,
    snowy_podzol: State,
    snowy_grass: State,
    snowy_mycelium: State,
    upper_sunflower: State,
    upper_lilac: State,
    upper_tall_grass: State,
    upper_large_fern: State,
    upper_rose_bush: State,
    upper_peony: State,
    flower_pot_map: HashMap<&'static str, State>,
    skull_map: HashMap<&'static str, State>,
    door_map: HashMap<&'static str, State>,
    note_block_map: HashMap<&'static str, State>,
    bed_block_map: HashMap<&'static str, State>,
    banner_block_map: HashMap<&'static str, State>,
}

static CONSTANTS: LazyLock<Constants> = LazyLock::new(|| {
    let t = table("chunk_paletted_storage_fix_mapping_constants");
    let value = |name: &str| -> State {
        t.field(name).entries[0]
            .value
            .as_ref()
            .unwrap_or_else(|| panic!("{name} is not null"))
    };
    let map = |name: &str| -> HashMap<&'static str, State> {
        t.field(name)
            .entries
            .iter()
            .map(|e| {
                (
                    e.key.as_ref().and_then(Tag::as_string).expect("string key"),
                    e.value.as_ref().expect("state"),
                )
            })
            .collect()
    };
    let bits = |name: &str| -> [bool; 256] {
        let mut set = [false; 256];
        for bit in t.field(name).bits() {
            set[bit] = true;
        }
        set
    };
    Constants {
        fix: bits("FIX"),
        virtual_: bits("VIRTUAL"),
        air: value("AIR"),
        pumpkin: value("PUMPKIN"),
        snowy_podzol: value("SNOWY_PODZOL"),
        snowy_grass: value("SNOWY_GRASS"),
        snowy_mycelium: value("SNOWY_MYCELIUM"),
        upper_sunflower: value("UPPER_SUNFLOWER"),
        upper_lilac: value("UPPER_LILAC"),
        upper_tall_grass: value("UPPER_TALL_GRASS"),
        upper_large_fern: value("UPPER_LARGE_FERN"),
        upper_rose_bush: value("UPPER_ROSE_BUSH"),
        upper_peony: value("UPPER_PEONY"),
        flower_pot_map: map("FLOWER_POT_MAP"),
        skull_map: map("SKULL_MAP"),
        door_map: map("DOOR_MAP"),
        note_block_map: map("NOTE_BLOCK_MAP"),
        bed_block_map: map("BED_BLOCK_MAP"),
        banner_block_map: map("BANNER_BLOCK_MAP"),
    }
});

/// `getName(state)`.
fn get_name(state: &Tag) -> &str {
    state
        .as_compound()
        .and_then(|c| c.get_string("Name"))
        .unwrap_or("")
}

/// `getProperty(state, property)`.
fn get_property<'a>(state: &'a Tag, property: &str) -> &'a str {
    state
        .as_compound()
        .and_then(|c| c.get("Properties"))
        .and_then(Tag::as_compound)
        .and_then(|p| p.get_string(property))
        .unwrap_or("")
}

/// `getSideMask(west, east, north, south)`.
fn get_side_mask(west: bool, east: bool, north: bool, south: bool) -> i32 {
    if north {
        if east {
            2
        } else if west {
            128
        } else {
            1
        }
    } else if south {
        if west {
            32
        } else if east {
            8
        } else {
            16
        }
    } else if east {
        4
    } else if west {
        64
    } else {
        0
    }
}

/// The state a map lookup that vanilla does not null-check delivers: its `getName` throws on `null`.
fn non_null(state: Option<State>) -> State {
    state.unwrap_or_else(|| {
        panic!(
            "java.lang.NullPointerException: Cannot invoke \"com.mojang.serialization.Dynamic.get(String)\" because \"state\" is null"
        )
    })
}

/// `DataLayer`: a 2048 byte nibble array.
struct DataLayer(Vec<i8>);

impl DataLayer {
    fn empty() -> DataLayer {
        DataLayer(vec![0; 2048])
    }

    /// `new DataLayer(byte[])`, or the empty layer for a field that is absent or no byte array.
    fn of_field(section: &Dynamic, key: &str) -> DataLayer {
        match section.get(key).as_byte_buffer_opt().result() {
            None => DataLayer::empty(),
            Some(data) => {
                assert!(
                    data.len() == 2048,
                    "java.lang.IllegalArgumentException: ChunkNibbleArrays should be 2048 bytes not: {}",
                    data.len()
                );
                DataLayer(data)
            }
        }
    }

    fn get(&self, x: i32, y: i32, z: i32) -> i32 {
        let position = y << 8 | z << 4 | x;
        let byte = i32::from(self.0[(position >> 1) as usize]);
        if position & 1 == 0 {
            byte & 15
        } else {
            byte >> 4 & 15
        }
    }
}

fn state_dynamic(state: State) -> Dynamic {
    Dynamic::new(NBT, Dyn::from(state))
}

struct Section {
    /// The palette and the identity set of the states seen, in insertion order (an entry's index is its id).
    palette: Vec<State>,
    palette_ids: HashMap<*const Tag, usize>,
    section: Dynamic,
    has_data: bool,
    /// `toFix`: block ids in the order they first occur, with the positions that need a fix.
    to_fix: Vec<(i32, Vec<i32>)>,
    update: Vec<i32>,
    y: i32,
    buffer: Vec<usize>,
}

impl Section {
    fn new(section: Dynamic) -> Section {
        let y = section.get("Y").as_int_or(0);
        let has_data = section.get("Blocks").result().is_some();
        Section {
            palette: Vec::new(),
            palette_ids: HashMap::new(),
            section,
            has_data,
            to_fix: Vec::new(),
            update: Vec::new(),
            y,
            buffer: vec![0; SIZE],
        }
    }

    fn get_block(&self, pos: i32) -> State {
        let c = &*CONSTANTS;
        match usize::try_from(pos).ok().filter(|&p| p < SIZE) {
            Some(pos) => self.palette.get(self.buffer[pos]).copied().unwrap_or(c.air),
            None => c.air,
        }
    }

    /// `idFor(palette, state)`: the id of an identical state, or the next id.
    fn id_for(&mut self, state: State) -> usize {
        let next = self.palette.len();
        *self
            .palette_ids
            .entry(std::ptr::from_ref(state))
            .or_insert_with(|| {
                self.palette.push(state);
                next
            })
    }

    fn set_block(&mut self, idx: i32, state: State) {
        let id = self.id_for(state);
        self.buffer[idx as usize] = id;
    }

    fn upgrade(&mut self, mut sides: i32) -> i32 {
        if !self.has_data {
            return sides;
        }
        let c = &*CONSTANTS;
        let Some(blocks) = self.section.get("Blocks").as_byte_buffer_opt().result() else {
            panic!("java.util.NoSuchElementException: No value present");
        };
        let data = DataLayer::of_field(&self.section, "Data");
        let add_blocks = DataLayer::of_field(&self.section, "Add");
        self.id_for(c.air);
        for idx in 0..SIZE as i32 {
            let (xx, yy, zz) = (idx & 15, idx >> 8 & 15, idx >> 4 & 15);
            // `ByteBuffer.get(int)` throws without a message
            let Some(&block) = blocks.get(idx as usize) else {
                panic!("java.lang.IndexOutOfBoundsException: null");
            };
            let id = add_blocks.get(xx, yy, zz) << 12
                | (i32::from(block) & 255) << 4
                | data.get(xx, yy, zz);
            let block_id = (id >> 4) as usize;
            if c.fix.get(block_id).copied().unwrap_or(false) {
                self.add_fix(block_id as i32, idx);
            }
            if c.virtual_.get(block_id).copied().unwrap_or(false) {
                let s = get_side_mask(xx == 0, xx == 15, zz == 0, zz == 15);
                if s == 0 {
                    self.update.push(idx);
                } else {
                    sides |= s;
                }
            }
            self.set_block(idx, block_state_data::get_tag(id));
        }
        sides
    }

    fn add_fix(&mut self, id: i32, position: i32) {
        match self.to_fix.iter_mut().find(|(i, _)| *i == id) {
            Some((_, list)) => list.push(position),
            None => self.to_fix.push((id, vec![position])),
        }
    }

    fn write(&self) -> Dynamic {
        let c = &*CONSTANTS;
        let mut section = self.section.clone();
        if !self.has_data {
            return section;
        }
        let list_tag = self.palette.iter().map(|state| {
            state_dynamic(if get_name(state) == FILTER_ME {
                c.air
            } else {
                state
            })
        });
        section = section.set("Palette", section.create_list(list_tag));
        let size = ceillog2(self.palette.len() as i32).max(4);
        let mut storage = PackedBitStorage::new(size, SIZE as i32);
        for (j, id) in self.buffer.iter().enumerate() {
            storage.set(j as i32, *id as i32);
        }
        section = section.set("BlockStates", section.create_long_list(storage.into_raw()));
        section = section.remove("Blocks");
        section = section.remove("Data");
        section.remove("Add")
    }
}

/// An `Int2ObjectLinkedOpenHashMap<Dynamic>`: insertion order, a replaced key keeps its place.
#[derive(Default)]
struct BlockEntities {
    entries: Vec<Option<(i32, Dynamic)>>,
    index: HashMap<i32, usize>,
}

impl BlockEntities {
    /// `put(key, entity)`: whether a value was replaced.
    fn put(&mut self, key: i32, entity: Dynamic) -> bool {
        match self.index.get(&key) {
            Some(&i) => {
                self.entries[i] = Some((key, entity));
                true
            }
            None => {
                self.index.insert(key, self.entries.len());
                self.entries.push(Some((key, entity)));
                false
            }
        }
    }

    fn get(&self, key: i32) -> Option<&Dynamic> {
        let i = *self.index.get(&key)?;
        self.entries[i].as_ref().map(|(_, e)| e)
    }

    fn remove(&mut self, key: i32) -> Option<Dynamic> {
        let i = self.index.remove(&key)?;
        self.entries[i].take().map(|(_, e)| e)
    }

    fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    fn values(&self) -> impl Iterator<Item = &Dynamic> {
        self.entries.iter().flatten().map(|(_, e)| e)
    }
}

struct UpgradeChunk {
    sides: i32,
    sections: [Option<Section>; 16],
    level: Dynamic,
    block_entities: BlockEntities,
}

/// `relative(pos, UP)` / `relative(pos, DOWN)`: the position above or below, `-1` off the chunk.
fn relative_y(pos: i32, step: i32) -> i32 {
    let y = (pos >> 8) + step;
    if (0..=255).contains(&y) {
        pos & 0xFF | y << 8
    } else {
        -1
    }
}

impl UpgradeChunk {
    fn new(level: &Dynamic) -> UpgradeChunk {
        let x = level.get("xPos").as_int_or(0) << 4;
        let z = level.get("zPos").as_int_or(0) << 4;
        let mut block_entities = BlockEntities::default();
        if let Some(entities) = level.get("TileEntities").as_stream_opt().result() {
            for entity in entities {
                let ex = entity.get("x").as_int_or(0).wrapping_sub(x) & 15;
                let ey = entity.get("y").as_int_or(0);
                let ez = entity.get("z").as_int_or(0).wrapping_sub(z) & 15;
                let key = ey << 8 | ez << 4 | ex;
                if block_entities.put(key, entity) {
                    log::warn!(
                        "In chunk: {x}x{z} found a duplicate block entity at position: [{ex}, {ey}, {ez}]"
                    );
                }
            }
        }
        let converted_from_alpha_format =
            level.get("convertedFromAlphaFormat").as_boolean_or(false);
        let mut chunk = UpgradeChunk {
            sides: 0,
            sections: std::array::from_fn(|_| None),
            level: level.clone(),
            block_entities,
        };
        if let Some(sections) = level.get("Sections").as_stream_opt().result() {
            for sec in sections {
                let mut section = Section::new(sec);
                chunk.sides = section.upgrade(chunk.sides);
                let y = section.y;
                let Some(slot) = usize::try_from(y)
                    .ok()
                    .and_then(|y| chunk.sections.get_mut(y))
                else {
                    panic!(
                        "java.lang.ArrayIndexOutOfBoundsException: Index {y} out of bounds for length 16"
                    );
                };
                *slot = Some(section);
            }
        }
        for i in 0..16 {
            let Some(to_fix) = chunk.sections[i].as_ref().map(|s| s.to_fix.clone()) else {
                continue;
            };
            let dy = (i as i32) << 12;
            for (block_id, positions) in to_fix {
                for pos in positions {
                    chunk.fix_block(block_id, pos | dy, converted_from_alpha_format);
                }
            }
        }
        chunk
    }

    /// The body of the `switch` over the legacy block id for one position.
    fn fix_block(&mut self, block_id: i32, pos: i32, converted_from_alpha_format: bool) {
        let c = &*CONSTANTS;
        let is_snow = |name: &str| name == "minecraft:snow" || name == "minecraft:snow_layer";
        match block_id {
            2 | 3 | 110 => {
                let (block, snowy) = match block_id {
                    2 => ("minecraft:grass_block", c.snowy_grass),
                    3 => ("minecraft:podzol", c.snowy_podzol),
                    _ => ("minecraft:mycelium", c.snowy_mycelium),
                };
                if get_name(self.get_block(pos)) == block
                    && is_snow(get_name(self.get_block(relative_y(pos, 1))))
                {
                    self.set_block(pos, snowy);
                }
            }
            25 => {
                if let Some(entity) = self.block_entities.remove(pos) {
                    let key = format!(
                        "{}{}",
                        entity.get("powered").as_boolean_or(false),
                        entity.get("note").as_int_or(0).clamp(0, 24)
                    );
                    let state = c
                        .note_block_map
                        .get(key.as_str())
                        .or_else(|| c.note_block_map.get("false0"));
                    self.set_block(pos, non_null(state.copied()));
                }
            }
            26 => {
                let state = self.get_block(pos);
                if let Some(entity) = self.block_entities.get(pos) {
                    let color = entity.get("color").as_int_or(0);
                    if color != 14 && (0..16).contains(&color) {
                        let key = format!(
                            "{}{}{}{color}",
                            get_property(state, "facing"),
                            get_property(state, "occupied"),
                            get_property(state, "part")
                        );
                        if let Some(bed) = c.bed_block_map.get(key.as_str()) {
                            self.set_block(pos, bed);
                        }
                    }
                }
            }
            64 | 71 | 193..=197 => {
                let state = self.get_block(pos);
                if get_name(state).ends_with("_door") {
                    let lower = self.get_block(pos);
                    if get_property(lower, "half") == "lower" {
                        let above_pos = relative_y(pos, 1);
                        let upper = self.get_block(above_pos);
                        let name = get_name(lower);
                        if name == get_name(upper) {
                            let facing = get_property(lower, "facing");
                            let open = get_property(lower, "open");
                            let hinge = if converted_from_alpha_format {
                                "left"
                            } else {
                                get_property(upper, "hinge")
                            };
                            let powered = if converted_from_alpha_format {
                                "false"
                            } else {
                                get_property(upper, "powered")
                            };
                            let lower_state = c
                                .door_map
                                .get(format!("{name}{facing}lower{hinge}{open}{powered}").as_str())
                                .copied();
                            self.set_block(pos, non_null(lower_state));
                            let upper_state = c
                                .door_map
                                .get(format!("{name}{facing}upper{hinge}{open}{powered}").as_str())
                                .copied();
                            self.set_block(above_pos, non_null(upper_state));
                        }
                    }
                }
            }
            86 => {
                if get_name(self.get_block(pos)) == "minecraft:carved_pumpkin" {
                    let name = get_name(self.get_block(relative_y(pos, -1)));
                    if name == "minecraft:grass_block" || name == "minecraft:dirt" {
                        self.set_block(pos, c.pumpkin);
                    }
                }
            }
            140 => {
                if let Some(entity) = self.block_entities.remove(pos) {
                    let key = format!(
                        "{}{}",
                        entity.get("Item").as_string_or("").as_lossy_str(),
                        entity.get("Data").as_int_or(0)
                    );
                    let state = c
                        .flower_pot_map
                        .get(key.as_str())
                        .or_else(|| c.flower_pot_map.get("minecraft:air0"));
                    self.set_block(pos, non_null(state.copied()));
                }
            }
            144 => {
                if let Some(entity) = self.block_entities.get(pos) {
                    let skull_type = entity.get("SkullType").as_int_or(0);
                    let facing = get_property(self.get_block(pos), "facing");
                    // `entity.remove(..)` results are discarded in vanilla: the block entity stays as it is
                    let key = if facing != "up" && facing != "down" {
                        format!("{skull_type}{facing}")
                    } else {
                        format!("{skull_type}{}", entity.get("Rot").as_int_or(0))
                    };
                    let state = c
                        .skull_map
                        .get(key.as_str())
                        .or_else(|| c.skull_map.get("0north"));
                    self.set_block(pos, non_null(state.copied()));
                }
            }
            175 => {
                let block = self.get_block(pos);
                if get_property(block, "half") == "upper" {
                    let below = self.get_block(relative_y(pos, -1));
                    let upper = match get_name(below) {
                        "minecraft:sunflower" => Some(c.upper_sunflower),
                        "minecraft:lilac" => Some(c.upper_lilac),
                        "minecraft:tall_grass" => Some(c.upper_tall_grass),
                        "minecraft:large_fern" => Some(c.upper_large_fern),
                        "minecraft:rose_bush" => Some(c.upper_rose_bush),
                        "minecraft:peony" => Some(c.upper_peony),
                        _ => None,
                    };
                    if let Some(upper) = upper {
                        self.set_block(pos, upper);
                    }
                }
            }
            176 | 177 => {
                let state = self.get_block(pos);
                if let Some(entity) = self.block_entities.get(pos) {
                    let color = entity.get("Base").as_int_or(0);
                    if color != 15 && (0..16).contains(&color) {
                        let key = format!(
                            "{}_{color}",
                            get_property(
                                state,
                                if block_id == 176 {
                                    "rotation"
                                } else {
                                    "facing"
                                }
                            )
                        );
                        if let Some(banner) = c.banner_block_map.get(key.as_str()) {
                            self.set_block(pos, banner);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn section_at(&self, pos: i32) -> Option<&Section> {
        self.sections
            .get(usize::try_from(pos >> 12).ok()?)?
            .as_ref()
    }

    fn get_block(&self, pos: i32) -> State {
        let c = &*CONSTANTS;
        if (0..=65535).contains(&pos) {
            self.section_at(pos)
                .map_or(c.air, |s| s.get_block(pos & 4095))
        } else {
            c.air
        }
    }

    fn set_block(&mut self, pos: i32, block: State) {
        if (0..=65535).contains(&pos)
            && let Some(Some(section)) = self.sections.get_mut((pos >> 12) as usize)
        {
            section.set_block(pos & 4095, block);
        }
    }

    fn write(&self) -> Dynamic {
        let mut level = self.level.clone();
        if self.block_entities.is_empty() {
            level = level.remove("TileEntities");
        } else {
            level = level.set(
                "TileEntities",
                level.create_list(self.block_entities.values().cloned()),
            );
        }
        let mut indices = level.empty_map();
        let mut sections = Vec::new();
        for section in self.sections.iter().flatten() {
            sections.push(section.write());
            indices = indices.set(
                &section.y.to_string(),
                indices.create_int_list(section.update.iter().copied()),
            );
        }
        let mut tag = level.empty_map();
        tag = tag.set("Sides", tag.create_byte(self.sides as i8));
        tag = tag.set("Indices", indices);
        let sections = tag.create_list(sections);
        level.set("UpgradeData", tag).set("Sections", sections)
    }
}

fn fix(input: Dynamic) -> Dynamic {
    match input.get("Level").result() {
        Some(level) if level.get("Sections").as_stream_opt().result().is_some() => {
            input.set("Level", UpgradeChunk::new(&level).write())
        }
        _ => input,
    }
}

/// `new ChunkPalettedStorageFix(outputSchema, changesType)`.
pub fn new(output_schema: &crate::schema::Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "ChunkPalettedStorageFix",
        output_schema,
        changes_type,
        |df| {
            let old_type = df.input_schema().get_type(&References::CHUNK);
            let new_type = df.output_schema().get_type(&References::CHUNK);
            df.write_fix_and_read("ChunkPalettedStorageFix", &old_type, &new_type, fix)
        },
    )
}
