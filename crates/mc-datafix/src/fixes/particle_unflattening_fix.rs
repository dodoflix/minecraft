//! `ParticleUnflatteningFix`: turns the flat particle command string (`minecraft:dust 1 0 0 1`) into a particle object.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use mc_brigadier::exceptions::CommandSyntaxException;
use mc_brigadier::string_reader::{ImmutableStringReader, StringReader};
use mc_core::java_collections::JavaHashMap;
use mc_core::math::mth::floor_f32;
use mc_dfu::{Dyn, Dynamic, NBT};
use mc_nbt::snbt;

/// `new ParticleUnflatteningFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ParticleUnflatteningFix", output_schema, true, |df| {
        let old_type = df.input_schema().get_type(&References::PARTICLE);
        let new_type = df.output_schema().get_type(&References::PARTICLE);
        df.write_fix_and_read("ParticleUnflatteningFix", &old_type, &new_type, fix)
    })
}

fn fix(input: Dynamic) -> Dynamic {
    let Some(particle_description) = input.as_string().result() else {
        return input;
    };
    // `String.split(" ", 2)`
    let mut parts = particle_description.splitn(2, ' ');
    let part0 = parts.next().unwrap_or("");
    let part1 = parts.next();
    let id = ensure_namespaced(part0);
    let result = input.create_map([(input.create_string("type"), input.create_string(id.clone()))]);
    let Some(contents) = part1 else {
        return result;
    };
    match id.as_str() {
        "minecraft:item" => update_item(&result, contents),
        "minecraft:block"
        | "minecraft:block_marker"
        | "minecraft:falling_dust"
        | "minecraft:dust_pillar" => update_block(&result, contents),
        "minecraft:dust" => update_dust(&result, contents),
        "minecraft:dust_color_transition" => update_dust_transition(&result, contents),
        "minecraft:sculk_charge" => update_sculk_charge(&result, contents),
        "minecraft:vibration" => update_vibration(&result, contents),
        "minecraft:shriek" => update_shriek(&result, contents),
        _ => result,
    }
}

fn update_item(result: &Dynamic, contents: &str) -> Dynamic {
    let tag_part_start = contents.find('{');
    let mut item_stack = result.create_map([(result.create_string("Count"), result.create_int(1))]);
    match tag_part_start {
        None => {
            item_stack = item_stack.set("id", result.create_string(contents));
        }
        Some(start) => {
            item_stack = item_stack.set("id", result.create_string(&contents[..start]));
            if let Some(item_tag) = parse_tag(result, &contents[start..]) {
                item_stack = item_stack.set("tag", item_tag);
            }
        }
    }
    result.set("item", item_stack)
}

/// `parseTag`: `TagParser.create(ops).parseFully(contents)`, `None` (and a warning) when it fails.
fn parse_tag(result: &Dynamic, contents: &str) -> Option<Dynamic> {
    match snbt::parse(contents) {
        Ok(tag) => {
            let parsed = Dynamic::new(NBT, Dyn::from(&tag));
            Some(if result.ops() == NBT {
                parsed
            } else {
                parsed.convert(&result.ops())
            })
        }
        Err(e) => {
            log::warn!("Failed to parse tag: {contents}: {e}");
            None
        }
    }
}

fn update_block(result: &Dynamic, contents: &str) -> Dynamic {
    let state_part_start = contents.find('[');
    let mut block_state = result.empty_map();
    match state_part_start {
        None => {
            block_state =
                block_state.set("Name", result.create_string(ensure_namespaced(contents)));
        }
        Some(start) => {
            block_state = block_state.set(
                "Name",
                result.create_string(ensure_namespaced(&contents[..start])),
            );
            let properties = parse_block_properties(result, &contents[start..]);
            if !properties.is_empty() {
                block_state = block_state.set(
                    "Properties",
                    result.create_map(properties.iter().map(|(k, v)| (k.clone(), v.clone()))),
                );
            }
        }
    }
    result.set("block_state", block_state)
}

fn parse_block_properties(dynamic: &Dynamic, contents: &str) -> JavaHashMap<Dynamic, Dynamic> {
    let attempt = || -> Result<JavaHashMap<Dynamic, Dynamic>, CommandSyntaxException> {
        let mut result = JavaHashMap::new(Dynamic::hash_code);
        let mut reader = StringReader::new(contents);
        reader.expect('[')?;
        reader.skip_whitespace();
        while reader.can_read() && reader.peek() != ']' {
            reader.skip_whitespace();
            let key = reader.read_string()?;
            reader.skip_whitespace();
            reader.expect('=')?;
            reader.skip_whitespace();
            let value = reader.read_string()?;
            reader.skip_whitespace();
            result.insert(dynamic.create_string(key), dynamic.create_string(value));
            if reader.can_read() {
                if reader.peek() != ',' {
                    break;
                }
                reader.skip();
            }
        }
        reader.expect(']')?;
        Ok(result)
    };
    match attempt() {
        Ok(result) => result,
        Err(e) => {
            log::warn!("Failed to parse block properties: {contents}: {e}");
            JavaHashMap::new(Dynamic::hash_code)
        }
    }
}

fn read_vector(
    result: &Dynamic,
    reader: &mut StringReader,
) -> Result<Dynamic, CommandSyntaxException> {
    let x = reader.read_float()?;
    reader.expect(' ')?;
    let y = reader.read_float()?;
    reader.expect(' ')?;
    let z = reader.read_float()?;
    Ok(result.create_list([x, y, z].map(|v| result.create_float(v))))
}

/// Runs `f`; a failure is logged (`Failed to parse particle options`) and `result` returned unchanged.
fn or_unchanged(
    result: &Dynamic,
    contents: &str,
    f: impl FnOnce(&mut StringReader) -> Result<Dynamic, CommandSyntaxException>,
) -> Dynamic {
    let mut reader = StringReader::new(contents);
    match f(&mut reader) {
        Ok(fixed) => fixed,
        Err(e) => {
            log::warn!("Failed to parse particle options: {contents}: {e}");
            result.clone()
        }
    }
}

fn update_dust(result: &Dynamic, contents: &str) -> Dynamic {
    or_unchanged(result, contents, |reader| {
        let vector = read_vector(result, reader)?;
        reader.expect(' ')?;
        let scale = reader.read_float()?;
        Ok(result
            .set("color", vector)
            .set("scale", result.create_float(scale)))
    })
}

fn update_dust_transition(result: &Dynamic, contents: &str) -> Dynamic {
    or_unchanged(result, contents, |reader| {
        let from = read_vector(result, reader)?;
        reader.expect(' ')?;
        let scale = reader.read_float()?;
        reader.expect(' ')?;
        let to = read_vector(result, reader)?;
        Ok(result
            .set("from_color", from)
            .set("to_color", to)
            .set("scale", result.create_float(scale)))
    })
}

fn update_sculk_charge(result: &Dynamic, contents: &str) -> Dynamic {
    or_unchanged(result, contents, |reader| {
        let roll = reader.read_float()?;
        Ok(result.set("roll", result.create_float(roll)))
    })
}

fn update_vibration(result: &Dynamic, contents: &str) -> Dynamic {
    or_unchanged(result, contents, |reader| {
        let dest_x = reader.read_double()? as f32;
        reader.expect(' ')?;
        let dest_y = reader.read_double()? as f32;
        reader.expect(' ')?;
        let dest_z = reader.read_double()? as f32;
        reader.expect(' ')?;
        let arrival_in_ticks = reader.read_int()?;
        let block_pos =
            result.create_int_list([floor_f32(dest_x), floor_f32(dest_y), floor_f32(dest_z)]);
        let position_source = result.create_map([
            (
                result.create_string("type"),
                result.create_string("minecraft:block"),
            ),
            (result.create_string("pos"), block_pos),
        ]);
        Ok(result
            .set("destination", position_source)
            .set("arrival_in_ticks", result.create_int(arrival_in_ticks)))
    })
}

fn update_shriek(result: &Dynamic, contents: &str) -> Dynamic {
    or_unchanged(result, contents, |reader| {
        let delay = reader.read_int()?;
        Ok(result.set("delay", result.create_int(delay)))
    })
}
