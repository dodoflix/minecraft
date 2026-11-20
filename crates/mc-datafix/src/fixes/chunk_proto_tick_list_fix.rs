//! `ChunkProtoTickListFix`: moves the proto-chunk tick lists into `fluid_ticks` / `TileTicks` and sets `yPos`.

use crate::datafix::DataFix;
use crate::fixes::chunk_height_and_biome_fix::ceillog2;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_dfu::Dynamic;
use std::cell::OnceCell;
use std::collections::HashMap;

const ALWAYS_WATERLOGGED: [&str; 5] = [
    "minecraft:bubble_column",
    "minecraft:kelp",
    "minecraft:kelp_plant",
    "minecraft:seagrass",
    "minecraft:tall_seagrass",
];

/// `new ChunkProtoTickListFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ChunkProtoTickListFix", output_schema, false, |df| {
        let chunk_type = df.input_schema().get_type(&References::CHUNK);
        let level_finder = chunk_type.find_field("Level");
        let sections_finder = level_finder.ty().find_field("Sections");
        let section_finder = sections_finder
            .ty()
            .as_list()
            .unwrap_or_else(|| {
                panic!("java.lang.ClassCastException: the type of Sections is not a list type")
            })
            .finder();
        let block_state_container_finder = section_finder.ty().find_field("block_states");
        let biome_container_finder = section_finder.ty().find_field("biomes");
        let block_state_palette_finder = block_state_container_finder.ty().find_field("palette");
        let tile_tick_finder = level_finder.ty().find_field("TileTicks");
        df.fix_type_everywhere_typed("ChunkProtoTickListFix", &chunk_type, move |chunk| {
            chunk.update_typed(&level_finder, |level| {
                fix_level(
                    level,
                    &sections_finder,
                    &section_finder,
                    &block_state_container_finder,
                    &biome_container_finder,
                    &block_state_palette_finder,
                    &tile_tick_finder,
                )
            })
        })
    })
}

type Containers = HashMap<i32, (Typed, OnceCell<PoorMansPalettedContainer>)>;

fn fix_level(
    level: Typed,
    sections_finder: &crate::finder::OpticFinder,
    section_finder: &crate::finder::OpticFinder,
    block_state_container_finder: &crate::finder::OpticFinder,
    biome_container_finder: &crate::finder::OpticFinder,
    block_state_palette_finder: &crate::finder::OpticFinder,
    tile_tick_finder: &crate::finder::OpticFinder,
) -> Typed {
    let mut level = level.update_remainder(|tag| match tag.get("LiquidTicks").result() {
        Some(v) => tag.set("fluid_ticks", v).remove("LiquidTicks"),
        None => tag,
    });
    let chunk_tag = level.get_remainder();
    let mut lowest_y: i32 = 0;
    let mut paletted_containers: Containers = HashMap::new();
    if let Some(sections) = level.get_optional_typed(sections_finder) {
        for section in sections.get_all_typed(section_finder) {
            let section_remainder = section.get_remainder();
            let section_y = section_remainder.get("Y").as_int_or(i32::MAX);
            if section_y != i32::MAX {
                if section.get_optional_typed(biome_container_finder).is_some() {
                    lowest_y = section_y.min(lowest_y);
                }
                if let Some(block_container) =
                    section.get_optional_typed(block_state_container_finder)
                {
                    paletted_containers.insert(section_y, (block_container, OnceCell::new()));
                }
            }
        }
    }
    let section_min_y = lowest_y as i8;
    level = level
        .update_remainder(|remainder| remainder.update("yPos", |y| y.create_byte(section_min_y)));
    if level.get_optional_typed(tile_tick_finder).is_none()
        && chunk_tag.get("fluid_ticks").result().is_none()
    {
        let section_x = chunk_tag.get("xPos").as_int_or(0);
        let section_z = chunk_tag.get("zPos").as_int_or(0);
        let fluid_ticks = make_tick_list(
            &chunk_tag,
            &paletted_containers,
            block_state_palette_finder,
            section_min_y,
            section_x,
            section_z,
            "LiquidsToBeTicked",
            get_liquid,
        );
        let block_ticks = make_tick_list(
            &chunk_tag,
            &paletted_containers,
            block_state_palette_finder,
            section_min_y,
            section_x,
            section_z,
            "ToBeTicked",
            get_block,
        );
        if let Some((parsed, _)) = tile_tick_finder.ty().read_typed(&block_ticks).result() {
            level = level.set_typed(tile_tick_finder, parsed);
        }
        level.update_remainder(|remainder| {
            remainder
                .remove("ToBeTicked")
                .remove("LiquidsToBeTicked")
                .set("fluid_ticks", fluid_ticks.clone())
        })
    } else {
        level
    }
}

#[allow(clippy::too_many_arguments, reason = "mirrors the Java helper")]
fn make_tick_list(
    tag: &Dynamic,
    paletted_containers: &Containers,
    palette_finder: &crate::finder::OpticFinder,
    section_min_y: i8,
    section_x: i32,
    section_z: i32,
    proto_tick_list_tag: &str,
    type_getter: fn(Option<&Dynamic>) -> String,
) -> Dynamic {
    let mut new_tick_list: Vec<Dynamic> = Vec::new();
    let ticks_per_section = tag.get(proto_tick_list_tag).as_list(|d| d);
    for (section_y_index, ticks) in ticks_per_section.iter().enumerate() {
        let section_y = (section_y_index as i32).wrapping_add(i32::from(section_min_y));
        let container = paletted_containers.get(&section_y);
        for pos in ticks.as_stream() {
            let pos = i32::from(pos.as_short_or(-1));
            if pos > 0 {
                new_tick_list.push(create_tick(
                    tag,
                    container,
                    palette_finder,
                    section_x,
                    section_y,
                    section_z,
                    pos,
                    type_getter,
                ));
            }
        }
    }
    tag.create_list(new_tick_list)
}

fn get_block(block_state: Option<&Dynamic>) -> String {
    match block_state {
        Some(b) => b.get("Name").as_string_or_lossy("minecraft:air"),
        None => "minecraft:air".to_string(),
    }
}

fn get_liquid(block_state: Option<&Dynamic>) -> String {
    let Some(block_state) = block_state else {
        return "minecraft:empty".to_string();
    };
    let block = block_state.get("Name").as_string_or("");
    let level_is_zero = || block_state.get("Properties").get("level").as_int_or(0) == 0;
    let s = if block == "minecraft:water" {
        if level_is_zero() {
            "minecraft:water"
        } else {
            "minecraft:flowing_water"
        }
    } else if block == "minecraft:lava" {
        if level_is_zero() {
            "minecraft:lava"
        } else {
            "minecraft:flowing_lava"
        }
    } else if !ALWAYS_WATERLOGGED.contains(&block.as_lossy_str())
        && !block_state
            .get("Properties")
            .get("waterlogged")
            .as_boolean_or(false)
    {
        "minecraft:empty"
    } else {
        "minecraft:water"
    };
    s.to_string()
}

#[allow(clippy::too_many_arguments, reason = "mirrors the Java helper")]
fn create_tick(
    tag: &Dynamic,
    container: Option<&(Typed, OnceCell<PoorMansPalettedContainer>)>,
    palette_finder: &crate::finder::OpticFinder,
    section_x: i32,
    section_y: i32,
    section_z: i32,
    pos: i32,
    type_getter: fn(Option<&Dynamic>) -> String,
) -> Dynamic {
    let relative_x = pos & 15;
    let relative_y = (pos as u32 >> 4 & 15) as i32;
    let relative_z = (pos as u32 >> 8 & 15) as i32;
    let ty = match container {
        Some((block_container, cell)) => {
            let container = cell.get_or_init(|| {
                let palette = block_container
                    .get_optional_typed(palette_finder)
                    .map(|x| {
                        x.write()
                            .result()
                            .map(|r| r.as_list(|d| d))
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                let data = block_container.get_remainder().get("data").as_long_stream();
                PoorMansPalettedContainer::new(palette, data)
            });
            type_getter(container.get(relative_x, relative_y, relative_z))
        }
        None => type_getter(None),
    };
    tag.create_map([
        (tag.create_string("i"), tag.create_string(ty)),
        (
            tag.create_string("x"),
            tag.create_int(section_x.wrapping_mul(16).wrapping_add(relative_x)),
        ),
        (
            tag.create_string("y"),
            tag.create_int(section_y.wrapping_mul(16).wrapping_add(relative_y)),
        ),
        (
            tag.create_string("z"),
            tag.create_int(section_z.wrapping_mul(16).wrapping_add(relative_z)),
        ),
        (tag.create_string("t"), tag.create_int(0)),
        (tag.create_string("p"), tag.create_int(0)),
    ])
}

/// `ChunkProtoTickListFix.PoorMansPalettedContainer`.
pub struct PoorMansPalettedContainer {
    palette: Vec<Dynamic>,
    data: Vec<i64>,
    bits: i32,
    mask: i64,
    values_per_long: i32,
}

impl PoorMansPalettedContainer {
    pub fn new(palette: Vec<Dynamic>, data: Vec<i64>) -> Self {
        let bits = 4.max(ceillog2(palette.len() as i32));
        PoorMansPalettedContainer {
            palette,
            data,
            bits,
            mask: (1i64 << bits) - 1,
            values_per_long: 64 / bits,
        }
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> Option<&Dynamic> {
        let entry_count = self.palette.len();
        if entry_count < 1 {
            None
        } else if entry_count == 1 {
            self.palette.first()
        } else {
            let index = (y << 4 | z) << 4 | x;
            let cell_index = index / self.values_per_long;
            let cell = usize::try_from(cell_index)
                .ok()
                .and_then(|i| self.data.get(i))?;
            let bit_index = (index - cell_index * self.values_per_long) * self.bits;
            let palette_index = (cell >> bit_index & self.mask) as i32;
            usize::try_from(palette_index)
                .ok()
                .and_then(|i| self.palette.get(i))
        }
    }
}
