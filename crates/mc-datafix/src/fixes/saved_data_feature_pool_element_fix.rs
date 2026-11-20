//! `net.minecraft.util.datafix.fixes.SavedDataFeaturePoolElementFix`: replaces the inline configured features of
//! jigsaw feature pool elements by their placed feature ids.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::{DataResult, Dynamic, OptionalDynamic};

const PIECE_TYPE: [&str; 5] = [
    "minecraft:jigsaw",
    "minecraft:nvi",
    "minecraft:pcp",
    "minecraft:bastionremnant",
    "minecraft:runtime",
];
const FEATURES: [&str; 4] = [
    "minecraft:tree",
    "minecraft:flower",
    "minecraft:block_pile",
    "minecraft:random_patch",
];

/// `new SavedDataFeaturePoolElementFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "SavedDataFeaturePoolElementFix",
        output_schema,
        false,
        |df| {
            df.write_fix_and_read(
                "SavedDataFeaturePoolElementFix",
                &df.input_schema().get_type(&References::STRUCTURE_FEATURE),
                &df.output_schema().get_type(&References::STRUCTURE_FEATURE),
                fix_tag,
            )
        },
    )
}

fn fix_tag(input: Dynamic) -> Dynamic {
    input.update("Children", update_children)
}

fn update_children(input: Dynamic) -> Dynamic {
    match input.as_stream_opt().result() {
        Some(children) => input.create_list(children.into_iter().map(fix_child)),
        None => input,
    }
}

fn fix_child(child: Dynamic) -> Dynamic {
    let id = child.get("id").as_string_or("");
    if !PIECE_TYPE.contains(&id.as_lossy_str()) {
        return child;
    }
    let pool_element = child.get("pool_element");
    if pool_element.get("element_type").as_string_or("") != "minecraft:feature_pool_element" {
        child
    } else {
        child.update("pool_element", |pool| pool.update("feature", fix_feature))
    }
}

/// The index of a `[<digits>]` path element (`INDEX_PATTERN.matcher(element).matches()`).
fn index_of(element: &str) -> Option<i32> {
    let digits = element.strip_prefix('[')?.strip_suffix(']')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(digits.parse().unwrap_or_else(|_| {
        panic!("java.lang.NumberFormatException: For input string: \"{digits}\"")
    }))
}

/// `get(input, path...)`: `path[0]` must exist; `[n]` elements index into the list found so far.
fn get(input: &Dynamic, path: &[&str]) -> OptionalDynamic {
    let mut output = input.get(path[0]);
    for element in &path[1..] {
        output = match index_of(element) {
            Some(id) => {
                let dynamics = output.as_list(|d| d);
                match usize::try_from(id).ok().and_then(|i| dynamics.get(i)) {
                    Some(d) => OptionalDynamic::new(input.ops(), DataResult::success(d.clone())),
                    None => OptionalDynamic::new(
                        input.ops(),
                        DataResult::error(move || format!("Missing id:{id}")),
                    ),
                }
            }
            None => output.get(element),
        };
    }
    output
}

/// `fixFeature(value)`.
pub fn fix_feature(value: Dynamic) -> Dynamic {
    let replacement = get_replacement(
        &get(&value, &["type"]).as_string_or_lossy(""),
        &get(&value, &["name"]).as_string_or_lossy(""),
        &get(&value, &["config", "state_provider", "type"]).as_string_or_lossy(""),
        &get(&value, &["config", "state_provider", "state", "Name"]).as_string_or_lossy(""),
        &get(
            &value,
            &["config", "state_provider", "entries", "[0]", "data", "Name"],
        )
        .as_string_or_lossy(""),
        &get(&value, &["config", "foliage_placer", "type"]).as_string_or_lossy(""),
        &get(&value, &["config", "leaves_provider", "state", "Name"]).as_string_or_lossy(""),
    );
    match replacement {
        Some(r) => value.create_string(r),
        None => value,
    }
}

fn get_replacement(
    ty: &str,
    name: &str,
    state_provider_type: &str,
    state_provider_state: &str,
    state_provider_first_weighed_state: &str,
    foliage_placer_type: &str,
    leaves_provider_state: &str,
) -> Option<&'static str> {
    let feature = if !ty.is_empty() {
        ty
    } else {
        if name.is_empty() {
            return None;
        }
        if name == "minecraft:normal_tree" {
            "minecraft:tree"
        } else {
            name
        }
    };

    if FEATURES.contains(&feature) {
        if feature == "minecraft:random_patch" {
            if state_provider_type == "minecraft:simple_state_provider" {
                if state_provider_state == "minecraft:sweet_berry_bush" {
                    return Some("minecraft:patch_berry_bush");
                }
                if state_provider_state == "minecraft:cactus" {
                    return Some("minecraft:patch_cactus");
                }
            } else if state_provider_type == "minecraft:weighted_state_provider"
                && (state_provider_first_weighed_state == "minecraft:grass"
                    || state_provider_first_weighed_state == "minecraft:fern")
            {
                return Some("minecraft:patch_taiga_grass");
            }
        } else if feature == "minecraft:block_pile" {
            if state_provider_type != "minecraft:simple_state_provider"
                && state_provider_type != "minecraft:rotated_block_provider"
            {
                if state_provider_type == "minecraft:weighted_state_provider" {
                    if state_provider_first_weighed_state == "minecraft:packed_ice"
                        || state_provider_first_weighed_state == "minecraft:blue_ice"
                    {
                        return Some("minecraft:pile_ice");
                    }
                    if state_provider_first_weighed_state == "minecraft:jack_o_lantern"
                        || state_provider_first_weighed_state == "minecraft:pumpkin"
                    {
                        return Some("minecraft:pile_pumpkin");
                    }
                }
            } else {
                if state_provider_state == "minecraft:hay_block" {
                    return Some("minecraft:pile_hay");
                }
                if state_provider_state == "minecraft:melon" {
                    return Some("minecraft:pile_melon");
                }
                if state_provider_state == "minecraft:snow" {
                    return Some("minecraft:pile_snow");
                }
            }
        } else {
            if feature == "minecraft:flower" {
                return Some("minecraft:flower_plain");
            }
            if feature == "minecraft:tree" {
                if foliage_placer_type == "minecraft:acacia_foliage_placer" {
                    return Some("minecraft:acacia");
                }
                if foliage_placer_type == "minecraft:blob_foliage_placer"
                    && leaves_provider_state == "minecraft:oak_leaves"
                {
                    return Some("minecraft:oak");
                }
                if foliage_placer_type == "minecraft:pine_foliage_placer" {
                    return Some("minecraft:pine");
                }
                if foliage_placer_type == "minecraft:spruce_foliage_placer" {
                    return Some("minecraft:spruce");
                }
            }
        }
    }
    None
}
