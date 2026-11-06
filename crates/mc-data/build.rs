//! Code generator for mc-data. Reads the vanilla reference material produced by `scripts/setup-ref.sh`
//! (`ref/extracted/*.json` from tools/extractor, `ref/src/{data,assets}`) and writes Rust tables to `OUT_DIR`.
//! The generated files are `include!`d by the modules of the crate; the extract schema is defined by `tools/extractor`.

use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

type Json = Value;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../..");
    let reference = root.join("ref");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rerun-if-changed={}",
        reference.join("extracted").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        reference.join("src/data").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        reference.join("src/assets").display()
    );

    let extracted = reference.join("extracted");
    if !extracted.join("manifest.json").exists() {
        panic!(
            "{} is missing: run scripts/setup-ref.sh (builds the gitignored ref/ reference material)",
            extracted.join("manifest.json").display()
        );
    }
    let g = Gen::load(&extracted, out);
    g.registries();
    g.enums_and_props();
    g.blocks();
    g.shapes();
    g.fluids();
    g.items();
    g.entities();
    g.registry_data();
    g.misc();
    datapack(&reference.join("src"), &g.out);
}

// ---------------------------------------------------------------------------------------------------------------
// helpers

fn read_json(p: &Path) -> Json {
    let s = fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn lit(s: &str) -> String {
    format!("{s:?}")
}

fn f32_lit(v: f64) -> String {
    assert!(v.is_finite());
    format!("{:?}", v as f32)
}

fn f64_lit(v: f64) -> String {
    assert!(v.is_finite());
    format!("{v:?}")
}

fn opt(v: Option<String>) -> String {
    match v {
        Some(s) => format!("Some({s})"),
        None => "None".into(),
    }
}

fn str_slice(items: impl IntoIterator<Item = String>) -> String {
    format!("&[{}]", items.into_iter().collect::<Vec<_>>().join(", "))
}

fn s<'a>(v: &'a Json, k: &str) -> &'a str {
    v[k].as_str()
        .unwrap_or_else(|| panic!("missing string {k} in {v}"))
}

fn flag(v: &Json, k: &str) -> bool {
    v.get(k).and_then(Json::as_bool).unwrap_or(false)
}

fn num(v: &Json, k: &str) -> f64 {
    v.get(k).and_then(Json::as_f64).unwrap_or(0.0)
}

fn int(v: &Json, k: &str) -> i64 {
    v.get(k).and_then(Json::as_i64).unwrap_or(0)
}

/// "TIP_MERGE" / "worldgen/feature_type" / "some.name" -> "TipMerge" / "WorldgenFeatureType" / "SomeName"
fn camel(s: &str) -> String {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut cs = p.chars();
            let first = cs.next().unwrap().to_ascii_uppercase();
            let rest: String = cs.as_str().to_ascii_lowercase();
            format!("{first}{rest}")
        })
        .collect()
}

/// "minecraft:oak_stairs" -> "OAK_STAIRS"
fn const_name(path: &str) -> String {
    let mut c: String = path
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    if c.starts_with(|c: char| c.is_ascii_digit()) {
        c.insert(0, '_');
    }
    c
}

fn strip_ns(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

/// Rust type name of the typed id newtype of a registry ("minecraft:menu" -> MenuType).
fn reg_type(reg: &str) -> String {
    match strip_ns(reg) {
        "menu" => "MenuType".into(),
        p => camel(p),
    }
}

fn reg_static(reg: &str) -> String {
    const_name(strip_ns(reg))
}

// ---------------------------------------------------------------------------------------------------------------

struct Registry {
    name: String,
    default: Option<String>,
    entries: Vec<String>,
}

struct Gen {
    extracted: PathBuf,
    out: PathBuf,
    regs: Vec<Registry>,
    /// registry name -> entry name -> id
    index: HashMap<String, HashMap<String, usize>>,
}

/// Java enum classes whose Rust counterpart lives in mc-core.
fn core_enum(class: &str) -> Option<&'static str> {
    match class {
        "net.minecraft.core.Direction" => Some("Direction"),
        "net.minecraft.core.Direction$Axis" => Some("Axis"),
        _ => None,
    }
}

fn enum_rust_name(class: &str) -> String {
    class.rsplit('.').next().unwrap().replace('$', "")
}

impl Gen {
    fn load(extracted: &Path, out: PathBuf) -> Gen {
        let reg_json = read_json(&extracted.join("registries.json"));
        let mut regs = Vec::new();
        let mut index = HashMap::new();
        for (name, v) in reg_json.as_object().unwrap() {
            let entries: Vec<String> = v["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.as_str().unwrap().to_string())
                .collect();
            index.insert(
                name.clone(),
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (e.clone(), i))
                    .collect(),
            );
            regs.push(Registry {
                name: name.clone(),
                default: v["default"].as_str().map(str::to_string),
                entries,
            });
        }
        Gen {
            extracted: extracted.to_path_buf(),
            out,
            regs,
            index,
        }
    }

    fn write(&self, file: &str, content: String) {
        fs::write(self.out.join(file), content).unwrap();
    }

    fn json(&self, file: &str) -> Json {
        read_json(&self.extracted.join(file))
    }

    /// Rust expression of a typed registry id.
    fn id(&self, reg: &str, entry: &str) -> String {
        let idx = self
            .index
            .get(reg)
            .and_then(|m| m.get(entry))
            .unwrap_or_else(|| panic!("{reg} has no entry {entry}"));
        format!("crate::ids::{}({idx})", reg_type(reg))
    }

    // -----------------------------------------------------------------------------------------------------------
    fn registries(&self) {
        let mut r = String::new();
        let mut ids = String::new();
        let mut all = Vec::new();
        for reg in &self.regs {
            let st = reg_static(&reg.name);
            let n = reg.entries.len();
            writeln!(
                r,
                "static {st}_ENTRIES: [&str; {n}] = [{}];",
                reg.entries
                    .iter()
                    .map(|e| lit(e))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
            let mut sorted: Vec<(&String, usize)> = reg
                .entries
                .iter()
                .enumerate()
                .map(|(i, e)| (e, i))
                .collect();
            sorted.sort();
            writeln!(
                r,
                "static {st}_BY_NAME: [(&str, u16); {n}] = [{}];",
                sorted
                    .iter()
                    .map(|(e, i)| format!("({}, {i})", lit(e)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
            writeln!(
                r,
                "pub static {st}: Registry = Registry::new({}, {}, &{st}_ENTRIES, &{st}_BY_NAME);",
                lit(&reg.name),
                opt(reg.default.as_deref().map(lit))
            )
            .unwrap();
            all.push(format!("&{st}"));

            let ty = reg_type(&reg.name);
            writeln!(ids, "#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]\npub struct {ty}(pub(crate) u16);").unwrap();
            writeln!(ids, "registry_id!({ty}, {st});").unwrap();
            writeln!(ids, "impl {ty} {{").unwrap();
            let mut seen = HashMap::new();
            for (i, e) in reg.entries.iter().enumerate() {
                let c = const_name(strip_ns(e));
                if let Some(prev) = seen.insert(c.clone(), e) {
                    panic!("const name collision in {}: {prev} / {e}", reg.name);
                }
                writeln!(ids, "    pub const {c}: {ty} = {ty}({i});").unwrap();
            }
            writeln!(ids, "}}").unwrap();
        }
        writeln!(
            r,
            "pub static ALL: [&Registry; {}] = [{}];",
            all.len(),
            all.join(", ")
        )
        .unwrap();
        self.write("registries.rs", r);
        self.write("ids.rs", ids);
    }

    // -----------------------------------------------------------------------------------------------------------
    fn enum_expr(class: &str, constant: &str) -> String {
        match core_enum(class) {
            Some(t) => format!("mc_core::direction::{t}::{}", camel(constant)),
            None => format!(
                "crate::enums::{}::{}",
                enum_rust_name(class),
                camel(constant)
            ),
        }
    }

    fn enums_and_props(&self) {
        let blocks = self.json("blocks.json");
        let misc = self.json("misc.json");
        // class -> (constant, serialized)
        let mut enums: BTreeMap<String, Vec<(String, Option<String>)>> = BTreeMap::new();
        for p in blocks["properties"].as_array().unwrap() {
            if p["type"] == "enum" {
                let class = s(p, "enum_class").to_string();
                let all = p["enum_all"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|k| {
                        (
                            s(k, "name").to_string(),
                            Some(s(k, "serialized").to_string()),
                        )
                    })
                    .collect();
                enums.insert(class, all);
            }
        }
        for (class, consts) in misc["enums"].as_object().unwrap() {
            enums.entry(class.clone()).or_insert_with(|| {
                consts
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|k| {
                        (
                            s(k, "name").to_string(),
                            k["serialized_name"].as_str().map(str::to_string),
                        )
                    })
                    .collect()
            });
        }
        let mut names = HashMap::new();
        let mut e = String::new();
        for (class, consts) in &enums {
            if core_enum(class).is_some() {
                continue;
            }
            let rust = enum_rust_name(class);
            if let Some(prev) = names.insert(rust.clone(), class.clone()) {
                panic!("enum name collision {rust}: {prev} / {class}");
            }
            writeln!(e, "/// Java `{class}`").unwrap();
            writeln!(e, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]\npub enum {rust} {{").unwrap();
            for (c, _) in consts {
                writeln!(e, "    {},", camel(c)).unwrap();
            }
            writeln!(e, "}}\nimpl {rust} {{").unwrap();
            writeln!(
                e,
                "    pub const ALL: &'static [{rust}] = &[{}];",
                consts
                    .iter()
                    .map(|(c, _)| format!("{rust}::{}", camel(c)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
            writeln!(
                e,
                "    const NAMES: &'static [&'static str] = &[{}];",
                consts
                    .iter()
                    .map(|(c, _)| lit(c))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
            writeln!(
                e,
                "    const SERIALIZED: &'static [Option<&'static str>] = &[{}];",
                consts
                    .iter()
                    .map(|(_, sn)| opt(sn.as_deref().map(lit)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
            writeln!(e, "    /// Java `Enum.ordinal()`.\n    pub const fn ordinal(self) -> usize {{ self as usize }}").unwrap();
            writeln!(
                e,
                "    pub fn from_ordinal(i: usize) -> Option<Self> {{ Self::ALL.get(i).copied() }}"
            )
            .unwrap();
            writeln!(e, "    /// Java `Enum.name()`.\n    pub fn name(self) -> &'static str {{ Self::NAMES[self as usize] }}").unwrap();
            writeln!(e, "    /// `StringRepresentable.getSerializedName()` (None if the Java enum is not StringRepresentable).\n    pub fn serialized_name(self) -> Option<&'static str> {{ Self::SERIALIZED[self as usize] }}").unwrap();
            writeln!(e, "    pub fn by_serialized_name(n: &str) -> Option<Self> {{ Self::SERIALIZED.iter().position(|s| *s == Some(n)).map(|i| Self::ALL[i]) }}").unwrap();
            writeln!(e, "}}\nimpl crate::props::EnumValue for {rust} {{\n    fn ordinal(self) -> usize {{ self as usize }}\n    fn from_ordinal(i: usize) -> Self {{ Self::ALL[i] }}\n}}").unwrap();
        }
        // vanilla SpawnPlacementTypes constants (instances of an interface, not a Java enum)
        let placements: Vec<&str> = misc["spawn_placement_types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        writeln!(e, "/// Static constants of `SpawnPlacementTypes`.\n#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]\npub enum SpawnPlacementType {{").unwrap();
        for c in &placements {
            writeln!(e, "    {},", camel(c)).unwrap();
        }
        writeln!(e, "}}\nimpl SpawnPlacementType {{\n    pub const ALL: &'static [SpawnPlacementType] = &[{}];\n    pub fn name(self) -> &'static str {{ [{}][self as usize] }}\n}}", placements.iter().map(|c| format!("SpawnPlacementType::{}", camel(c))).collect::<Vec<_>>().join(", "), placements.iter().map(|c| lit(c)).collect::<Vec<_>>().join(", ")).unwrap();
        self.write("enums.rs", e);

        // property definitions + typed consts
        let props = blocks["properties"].as_array().unwrap();
        let mut p = String::new();
        let mut defs = Vec::new();
        let mut consts = String::new();
        for (i, pr) in props.iter().enumerate() {
            let name = s(pr, "name");
            let values: Vec<&str> = pr["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            let (kind, min, enum_class) = match s(pr, "type") {
                "boolean" => ("PropertyKind::Boolean", 0, "None".to_string()),
                "integer" => (
                    "PropertyKind::Integer",
                    values[0].parse::<i32>().unwrap(),
                    "None".to_string(),
                ),
                _ => (
                    "PropertyKind::Enum",
                    0,
                    format!("Some({})", lit(s(pr, "enum_class"))),
                ),
            };
            defs.push(format!(
                "PropertyDef {{ name: {}, kind: {kind}, values: {}, min: {min}, enum_class: {enum_class} }}",
                lit(name),
                str_slice(values.iter().map(|v| lit(v)))
            ));
            let c = pr["const"].as_str().map(str::to_string).unwrap_or_else(|| {
                panic!("property {name} (#{i}) is not a BlockStateProperties constant")
            });
            match s(pr, "type") {
                "boolean" => writeln!(
                    consts,
                    "pub const {c}: BooleanProperty = BooleanProperty::new({i});"
                )
                .unwrap(),
                "integer" => writeln!(
                    consts,
                    "pub const {c}: IntegerProperty = IntegerProperty::new({i}, {min});"
                )
                .unwrap(),
                _ => {
                    let class = s(pr, "enum_class");
                    let ty = match core_enum(class) {
                        Some(t) => format!("mc_core::direction::{t}"),
                        None => format!("crate::enums::{}", enum_rust_name(class)),
                    };
                    let vals: Vec<String> = pr["enum_constants"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|k| Self::enum_expr(class, k.as_str().unwrap()))
                        .collect();
                    writeln!(
                        consts,
                        "pub const {c}: EnumProperty<{ty}> = EnumProperty::new({i}, &[{}]);",
                        vals.join(", ")
                    )
                    .unwrap();
                }
            }
        }
        writeln!(
            p,
            "pub static PROPERTY_DEFS: [PropertyDef; {}] = [{}];",
            defs.len(),
            defs.join(",\n    ")
        )
        .unwrap();
        p.push_str(&consts);
        self.write("props.rs", p);
    }

    // -----------------------------------------------------------------------------------------------------------
    fn blocks(&self) {
        let blocks = self.json("blocks.json");
        let sound_types = blocks["sound_types"].as_array().unwrap();
        let mut o = String::new();
        let mut sts = Vec::new();
        for t in sound_types {
            let ev = |k: &str| self.id("minecraft:sound_event", s(t, k));
            sts.push(format!(
                "SoundType {{ volume: {}, pitch: {}, break_sound: {}, step_sound: {}, place_sound: {}, hit_sound: {}, fall_sound: {} }}",
                f32_lit(num(t, "volume")),
                f32_lit(num(t, "pitch")),
                ev("break"),
                ev("step"),
                ev("place"),
                ev("hit"),
                ev("fall")
            ));
        }
        writeln!(
            o,
            "pub static SOUND_TYPES: [SoundType; {}] = [{}];",
            sts.len(),
            sts.join(",\n    ")
        )
        .unwrap();

        const BLOCK_FLAGS: &[&str] = &[
            "has_collision",
            "requires_correct_tool_for_drops",
            "random_ticks",
            "dynamic_shape",
            "ignited_by_lava",
            "replaceable",
            "can_occlude",
            "is_air",
            "liquid",
            "force_solid_on",
            "force_solid_off",
            "spawn_terrain_particles",
            "has_block_entity",
        ];
        const STATE_FLAGS: &[&str] = &[
            "can_occlude",
            "solid_render",
            "is_air",
            "propagates_skylight_down",
            "use_shape_for_light_occlusion",
            "is_redstone_conductor",
            "is_suffocating",
            "is_view_blocking",
            "has_large_collision_shape",
            "requires_correct_tool_for_drops",
            "is_collision_shape_full_block",
            "legacy_solid",
            "random_ticking",
            "signal_source",
            "has_analog_output_signal",
            "emissive_rendering",
            "render_model",
            "can_be_replaced",
            "liquid",
            "ignited_by_lava",
            "has_offset",
        ];

        let mut bd = Vec::new();
        let mut state_data: Vec<String> = Vec::new();
        let mut state_data_ids: HashMap<String, usize> = HashMap::new();
        let mut state_index: Vec<usize> = Vec::new();
        let mut state_block: Vec<usize> = Vec::new();
        let mut state_count = 0usize;
        for b in blocks["blocks"].as_array().unwrap() {
            let id = int(b, "id") as usize;
            let mut bflags = 0u32;
            for (i, f) in BLOCK_FLAGS.iter().enumerate() {
                if flag(b, f) {
                    bflags |= 1 << i;
                }
            }
            let name = s(b, "name");
            let first = int(b, "first_state") as usize;
            let count = int(b, "state_count") as usize;
            assert_eq!(
                first, state_count,
                "state ids must be contiguous in block order ({name})"
            );
            let item = b["item"].as_str().map(|i| self.id("minecraft:item", i));
            let loot = b["loot_table"].as_str().map(lit);
            bd.push(format!(
                "BlockData {{ id: {id}, name: {}, class: {}, classes: {}, description_id: {}, default_state: {}, first_state: {first}, state_count: {count}, \
                 props: &[{}], destroy_time: {}, explosion_resistance: {}, friction: {}, speed_factor: {}, jump_factor: {}, bounce_restitution: {}, \
                 fall_distance_reduction: {}, flags: {bflags}, push_reaction: crate::enums::PushReaction::{}, instrument: crate::enums::NoteBlockInstrument::{}, \
                 map_color: {}, sound_type: {}, max_horizontal_offset: {}, max_vertical_offset: {}, offset_type: OffsetType::{}, loot_table: {}, item: {}, \
                 required_features: {} }}",
                lit(name),
                lit(s(b, "class")),
                str_slice(b["classes"].as_array().unwrap().iter().map(|c| lit(c.as_str().unwrap()))),
                lit(s(b, "description_id")),
                int(b, "default_state"),
                b["props"].as_array().unwrap().iter().map(|p| p.as_i64().unwrap().to_string()).collect::<Vec<_>>().join(", "),
                f32_lit(num(b, "destroy_time")),
                f32_lit(num(b, "explosion_resistance")),
                f32_lit(num(b, "friction")),
                f32_lit(num(b, "speed_factor")),
                f32_lit(num(b, "jump_factor")),
                f32_lit(num(b, "bounce_restitution")),
                f32_lit(num(b, "fall_distance_reduction")),
                camel(s(b, "push_reaction")),
                camel(s(b, "instrument")),
                int(b, "map_color"),
                int(b, "sound_type"),
                f32_lit(num(b, "max_horizontal_offset")),
                f32_lit(num(b, "max_vertical_offset")),
                camel(s(b, "offset_type")),
                opt(loot),
                opt(item),
                str_slice(b["required_features"].as_array().unwrap().iter().map(|c| lit(c.as_str().unwrap()))),
            ));
            let def_sound = int(b, "sound_type");
            let def_color = int(b, "map_color");
            let states = b["states"].as_array().unwrap();
            assert_eq!(states.len(), count);
            for st in states {
                let mut flags = 0u32;
                for (i, f) in STATE_FLAGS.iter().enumerate() {
                    if flag(st, f) {
                        flags |= 1 << i;
                    }
                }
                let sh: Vec<i64> = st["shapes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_i64().unwrap())
                    .collect();
                assert!(
                    sh.iter().all(|&x| x >= 0),
                    "shape evaluation failed for a state of {name}"
                );
                let lit_state = format!(
                    "StateData {{ flags: {flags}, light_emission: {}, light_block: {}, destroy_speed: {}, shade_brightness: {}, face_sturdy: {}, pathfindable: {}, \
                     fluid_state: crate::fluid::FluidState({}), sound_type: {}, map_color: {}, shapes: [{}] }}",
                    int(st, "light"),
                    int(st, "light_block"),
                    f32_lit(num(st, "destroy_speed")),
                    f32_lit(num(st, "shade_brightness")),
                    int(st, "face_sturdy"),
                    int(st, "pathfindable"),
                    int(st, "fluid_state"),
                    st.get("sound_type")
                        .and_then(Json::as_i64)
                        .unwrap_or(def_sound),
                    st.get("map_color")
                        .and_then(Json::as_i64)
                        .unwrap_or(def_color),
                    sh.iter()
                        .map(|x| format!("ShapeId({x})"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                let next = state_data.len();
                let idx = *state_data_ids.entry(lit_state.clone()).or_insert_with(|| {
                    state_data.push(lit_state);
                    next
                });
                state_index.push(idx);
                state_block.push(id);
                state_count += 1;
            }
        }
        assert_eq!(state_count as i64, blocks["state_count"].as_i64().unwrap());
        writeln!(o, "pub const STATE_COUNT: usize = {state_count};").unwrap();
        writeln!(
            o,
            "pub static BLOCK_DATA: [BlockData; {}] = [{}];",
            bd.len(),
            bd.join(",\n    ")
        )
        .unwrap();
        writeln!(
            o,
            "pub static STATE_DATA: [StateData; {}] = [{}];",
            state_data.len(),
            state_data.join(",\n    ")
        )
        .unwrap();
        writeln!(
            o,
            "pub static STATE_DATA_INDEX: [u16; {state_count}] = [{}];",
            state_index
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
        .unwrap();
        writeln!(
            o,
            "pub static STATE_BLOCK: [u16; {state_count}] = [{}];",
            state_block
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
        .unwrap();
        self.write("blocks.rs", o);
    }

    // -----------------------------------------------------------------------------------------------------------
    fn shapes(&self) {
        let shapes = self.json("shapes.json");
        let mut o = String::new();
        let mut items = Vec::new();
        let f64s = |v: &Json| {
            str_slice(
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|x| f64_lit(x.as_f64().unwrap())),
            )
        };
        let u8s = |v: &Json| {
            format!(
                "[{}]",
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_u64().unwrap().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        for sh in shapes.as_array().unwrap() {
            let bits = s(sh, "bits");
            let bytes: Vec<String> = (0..bits.len() / 2)
                .map(|i| format!("0x{}", &bits[i * 2..i * 2 + 2]))
                .collect();
            let kind = match s(sh, "kind") {
                "cube" => "Cube",
                "array" => "Array",
                "slice" => "Slice",
                other => panic!("unknown shape kind {other}"),
            };
            let aabbs = sh["aabbs"].as_array().unwrap().iter().map(|b| {
                format!(
                    "[{}]",
                    b.as_array()
                        .unwrap()
                        .iter()
                        .map(|x| f64_lit(x.as_f64().unwrap()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
            items.push(format!(
                "ShapeData {{ kind: ShapeKind::{kind}, size: {}, first: {}, last: {}, bits: &[{}], coords: [{}, {}, {}], aabbs: {} }}",
                u8s(&sh["size"]),
                u8s(&sh["first"]),
                u8s(&sh["last"]),
                bytes.join(", "),
                f64s(&sh["xs"]),
                f64s(&sh["ys"]),
                f64s(&sh["zs"]),
                str_slice(aabbs)
            ));
        }
        writeln!(
            o,
            "pub static SHAPES: [ShapeData; {}] = [{}];",
            items.len(),
            items.join(",\n    ")
        )
        .unwrap();
        self.write("shapes.rs", o);
    }

    // -----------------------------------------------------------------------------------------------------------
    fn fluids(&self) {
        let fluids = self.json("fluids.json");
        let mut fd = Vec::new();
        let mut states: BTreeMap<usize, String> = BTreeMap::new();
        for f in fluids.as_array().unwrap() {
            let id = int(f, "id");
            let first = f["states"]
                .as_array()
                .unwrap()
                .iter()
                .map(|st| int(st, "id"))
                .min()
                .unwrap();
            let count = f["states"].as_array().unwrap().len();
            fd.push(format!(
                "FluidData {{ id: {id}, name: {}, class: {}, bucket: {}, explosion_resistance: {}, pickup_sound: {}, default_state: {}, first_state: {first}, state_count: {count} }}",
                lit(s(f, "name")),
                lit(s(f, "class")),
                opt(f["bucket"].as_str().map(|b| self.id("minecraft:item", b))),
                f32_lit(num(f, "explosion_resistance")),
                opt(f["pickup_sound"].as_str().map(|b| self.id("minecraft:sound_event", b))),
                int(f, "default_state"),
            ));
            for st in f["states"].as_array().unwrap() {
                let props: Vec<String> = st["properties"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| format!("({}, {})", lit(k), lit(v.as_str().unwrap())))
                    .collect();
                states.insert(
                    int(st, "id") as usize,
                    format!(
                        "FluidStateData {{ fluid: {}, properties: {}, amount: {}, is_source: {}, is_empty: {}, own_height: {}, random_ticking: {}, legacy_block_state: crate::block::BlockState({}), shape: ShapeId({}) }}",
                        self.id("minecraft:fluid", s(f, "name")),
                        str_slice(props),
                        int(st, "amount"),
                        flag(st, "is_source"),
                        flag(st, "is_empty"),
                        f32_lit(num(st, "own_height")),
                        flag(st, "random_ticking"),
                        int(st, "legacy_block_state"),
                        int(st, "shape"),
                    ),
                );
            }
        }
        for (i, k) in states.keys().enumerate() {
            assert_eq!(i, *k, "fluid state ids must be contiguous");
        }
        let mut o = String::new();
        writeln!(
            o,
            "pub static FLUID_DATA: [FluidData; {}] = [{}];",
            fd.len(),
            fd.join(",\n    ")
        )
        .unwrap();
        writeln!(
            o,
            "pub static FLUID_STATE_DATA: [FluidStateData; {}] = [{}];",
            states.len(),
            states.values().cloned().collect::<Vec<_>>().join(",\n    ")
        )
        .unwrap();
        self.write("fluids.rs", o);
    }

    // -----------------------------------------------------------------------------------------------------------
    fn items(&self) {
        let items = self.json("items.json");
        let mut v = Vec::new();
        for (i, it) in items.as_array().unwrap().iter().enumerate() {
            assert_eq!(int(it, "id") as usize, i);
            v.push(format!(
                "ItemData {{ name: {}, class: {}, classes: {}, description_id: {}, max_stack_size: {}, block: {}, crafting_remainder: {}, required_features: {}, components_json: {} }}",
                lit(s(it, "name")),
                lit(s(it, "class")),
                str_slice(it["classes"].as_array().unwrap().iter().map(|c| lit(c.as_str().unwrap()))),
                lit(s(it, "description_id")),
                int(it, "max_stack_size"),
                opt(it["block"].as_str().map(|b| self.id("minecraft:block", b))),
                opt(it["crafting_remainder"].as_str().map(|b| self.id("minecraft:item", b))),
                str_slice(it["required_features"].as_array().unwrap().iter().map(|c| lit(c.as_str().unwrap()))),
                lit(&serde_json::to_string(&it["components"]).unwrap()),
            ));
        }
        self.write(
            "items.rs",
            format!(
                "pub static ITEM_DATA: [ItemData; {}] = [{}];\n",
                v.len(),
                v.join(",\n    ")
            ),
        );
    }

    // -----------------------------------------------------------------------------------------------------------
    fn entities(&self) {
        let j = self.json("entity_types.json");
        let mut o = String::new();
        let cats: Vec<String> = j["mob_categories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                format!(
                    "MobCategoryData {{ category: crate::enums::MobCategory::{}, serialized_name: {}, max_instances_per_chunk: {}, friendly: {}, persistent: {}, despawn_distance: {}, no_despawn_distance: {} }}",
                    camel(s(c, "name")),
                    lit(s(c, "serialized_name")),
                    int(c, "max_instances_per_chunk"),
                    flag(c, "friendly"),
                    flag(c, "persistent"),
                    int(c, "despawn_distance"),
                    int(c, "no_despawn_distance"),
                )
            })
            .collect();
        writeln!(
            o,
            "pub static MOB_CATEGORIES: [MobCategoryData; {}] = [{}];",
            cats.len(),
            cats.join(", ")
        )
        .unwrap();
        let mut v = Vec::new();
        for (i, t) in j["entity_types"].as_array().unwrap().iter().enumerate() {
            assert_eq!(int(t, "id") as usize, i);
            let d = &t["dimensions"];
            let att = &d["attachments"];
            let pts = |k: &str| {
                str_slice(att[k].as_array().unwrap().iter().map(|p| {
                    format!(
                        "[{}]",
                        p.as_array()
                            .unwrap()
                            .iter()
                            .map(|x| f64_lit(x.as_f64().unwrap()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }))
            };
            let spawn = t.get("spawn_placement").map(|sp| {
                format!("SpawnPlacement {{ placement: crate::enums::SpawnPlacementType::{}, heightmap: crate::enums::HeightmapTypes::{} }}", camel(s(sp, "placement")), camel(s(sp, "heightmap")))
            });
            let attrs: Vec<String> = t
                .get("default_attributes")
                .map(|a| {
                    a.as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| {
                            format!(
                                "({}, {})",
                                self.id("minecraft:attribute", k),
                                f64_lit(v.as_f64().unwrap())
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            v.push(format!(
                "EntityTypeData {{ name: {}, description_id: {}, category: crate::enums::MobCategory::{}, width: {}, height: {}, eye_height: {}, fixed: {}, \
                 attachments: [{}, {}, {}, {}], spawn_dimensions_scale: {}, client_tracking_range: {}, update_interval: {}, track_deltas: {}, fire_immune: {}, \
                 summonable: {}, serializable: {}, can_spawn_far_from_player: {}, allowed_in_peaceful: {}, immune_to: {}, loot_table: {}, required_features: {}, \
                 spawn_placement: {}, default_attributes: {} }}",
                lit(s(t, "name")),
                lit(s(t, "description_id")),
                camel(s(t, "category")),
                f32_lit(num(d, "width")),
                f32_lit(num(d, "height")),
                f32_lit(num(d, "eye_height")),
                flag(d, "fixed"),
                pts("PASSENGER"),
                pts("VEHICLE"),
                pts("NAME_TAG"),
                pts("WARDEN_CHEST"),
                f32_lit(num(t, "spawn_dimensions_scale")),
                int(t, "client_tracking_range"),
                int(t, "update_interval"),
                flag(t, "track_deltas"),
                flag(t, "fire_immune"),
                flag(t, "summonable"),
                flag(t, "serializable"),
                flag(t, "can_spawn_far_from_player"),
                flag(t, "allowed_in_peaceful"),
                lit(s(t, "immune_to")),
                opt(t["loot_table"].as_str().map(lit)),
                str_slice(t["required_features"].as_array().unwrap().iter().map(|c| lit(c.as_str().unwrap()))),
                opt(spawn),
                str_slice(attrs),
            ));
        }
        writeln!(
            o,
            "pub static ENTITY_TYPE_DATA: [EntityTypeData; {}] = [{}];",
            v.len(),
            v.join(",\n    ")
        )
        .unwrap();
        self.write("entities.rs", o);
    }

    // -----------------------------------------------------------------------------------------------------------
    fn registry_data(&self) {
        let j = self.json("registry_data.json");
        let mut o = String::new();
        let table =
            |o: &mut String, key: &str, ty: &str, stat: &str, f: &dyn Fn(&Json) -> String| {
                let items: Vec<String> = j[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(i, x)| {
                        assert_eq!(int(x, "id") as usize, i, "{key} ids");
                        f(x)
                    })
                    .collect();
                writeln!(
                    o,
                    "pub static {stat}: [{ty}; {}] = [{}];",
                    items.len(),
                    items.join(",\n    ")
                )
                .unwrap();
            };
        let feats = |x: &Json| {
            str_slice(
                x["required_features"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| lit(c.as_str().unwrap())),
            )
        };
        table(
            &mut o,
            "attributes",
            "AttributeData",
            "ATTRIBUTE_DATA",
            &|a| {
                format!(
                    "AttributeData {{ description_id: {}, default_value: {}, min_value: {}, max_value: {}, syncable: {}, sentiment: crate::enums::AttributeSentiment::{} }}",
                    lit(s(a, "description_id")),
                    f64_lit(num(a, "default")),
                    opt(a.get("min").map(|v| f64_lit(v.as_f64().unwrap()))),
                    opt(a.get("max").map(|v| f64_lit(v.as_f64().unwrap()))),
                    flag(a, "syncable"),
                    camel(s(a, "sentiment")),
                )
            },
        );
        table(
            &mut o,
            "mob_effects",
            "MobEffectData",
            "MOB_EFFECT_DATA",
            &|e| {
                let mods = e["attribute_modifiers"].as_array().unwrap().iter().map(|m| {
                format!(
                    "AttributeModifierTemplate {{ attribute: {}, id: {}, amount: {}, operation: crate::enums::AttributeModifierOperation::{} }}",
                    self.id("minecraft:attribute", s(m, "attribute")),
                    lit(s(m, "id")),
                    f64_lit(num(m, "amount")),
                    camel(s(m, "operation"))
                )
            });
                format!(
                    "MobEffectData {{ class: {}, description_id: {}, category: crate::enums::MobEffectCategory::{}, color: {}, instantaneous: {}, blend_in: {}, blend_out: {}, \
                 blend_out_advance: {}, sound_on_added: {}, required_features: {}, attribute_modifiers: {} }}",
                    lit(s(e, "class")),
                    lit(s(e, "description_id")),
                    camel(s(e, "category")),
                    int(e, "color"),
                    flag(e, "instantaneous"),
                    int(e, "blend_in"),
                    int(e, "blend_out"),
                    int(e, "blend_out_advance"),
                    opt(e["sound_on_added"]
                        .as_str()
                        .map(|n| self.id("minecraft:sound_event", n))),
                    feats(e),
                    str_slice(mods)
                )
            },
        );
        table(&mut o, "potions", "PotionData", "POTION_DATA", &|p| {
            let effects = p["effects"].as_array().unwrap().iter().map(|m| {
                format!(
                    "MobEffectInstanceData {{ effect: {}, duration: {}, amplifier: {}, ambient: {}, visible: {}, show_icon: {} }}",
                    self.id("minecraft:mob_effect", s(m, "effect")),
                    int(m, "duration"),
                    int(m, "amplifier"),
                    flag(m, "ambient"),
                    flag(m, "visible"),
                    flag(m, "show_icon")
                )
            });
            format!(
                "PotionData {{ base_name: {}, required_features: {}, effects: {} }}",
                lit(s(p, "base_name")),
                feats(p),
                str_slice(effects)
            )
        });
        table(
            &mut o,
            "game_events",
            "GameEventData",
            "GAME_EVENT_DATA",
            &|g| {
                format!(
                    "GameEventData {{ notification_radius: {} }}",
                    int(g, "notification_radius")
                )
            },
        );
        table(
            &mut o,
            "sound_events",
            "SoundEventData",
            "SOUND_EVENT_DATA",
            &|x| {
                format!(
                    "SoundEventData {{ fixed_range: {} }}",
                    opt(x.get("fixed_range").map(|v| f32_lit(v.as_f64().unwrap())))
                )
            },
        );
        table(
            &mut o,
            "particle_types",
            "ParticleTypeData",
            "PARTICLE_TYPE_DATA",
            &|x| {
                format!(
                    "ParticleTypeData {{ class: {}, override_limiter: {} }}",
                    lit(s(x, "class")),
                    flag(x, "override_limiter")
                )
            },
        );
        table(
            &mut o,
            "menu_types",
            "MenuTypeData",
            "MENU_TYPE_DATA",
            &|x| format!("MenuTypeData {{ required_features: {} }}", feats(x)),
        );
        table(
            &mut o,
            "data_component_types",
            "DataComponentTypeData",
            "DATA_COMPONENT_TYPE_DATA",
            &|x| {
                format!(
                    "DataComponentTypeData {{ const_name: {}, persistent: {}, network_synchronized: {}, ignore_swap_animation: {} }}",
                    opt(x["const"].as_str().map(lit)),
                    flag(x, "persistent"),
                    flag(x, "network_synchronized"),
                    flag(x, "ignore_swap_animation")
                )
            },
        );
        table(
            &mut o,
            "block_entity_types",
            "BlockEntityTypeData",
            "BLOCK_ENTITY_TYPE_DATA",
            &|x| {
                let blocks = x["valid_blocks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| self.id("minecraft:block", b.as_str().unwrap()));
                format!(
                    "BlockEntityTypeData {{ valid_blocks: {}, only_op_can_set_nbt: {} }}",
                    str_slice(blocks),
                    flag(x, "only_op_can_set_nbt")
                )
            },
        );
        self.write("registry_data.rs", o);
    }

    // -----------------------------------------------------------------------------------------------------------
    fn misc(&self) {
        let j = self.json("misc.json");
        let mut o = String::new();
        let block = |n: &str| self.id("minecraft:block", n);

        // sorted by block id so lookups can binary search
        let mut fire: Vec<(usize, String)> = j["fire_odds"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    self.index["minecraft:block"][k],
                    format!(
                        "({}, {}, {})",
                        block(k),
                        int(v, "ignite_odds"),
                        int(v, "burn_odds")
                    ),
                )
            })
            .collect();
        fire.sort();
        writeln!(
            o,
            "pub static FIRE_ODDS: [(Block, i32, i32); {}] = [{}];",
            fire.len(),
            fire.iter()
                .map(|x| x.1.clone())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        for (key, name) in [
            ("weathering_next", "WEATHERING_NEXT"),
            ("waxables", "WAXABLES"),
            ("potted_by_content", "POTTED_BY_CONTENT"),
        ] {
            let mut m: Vec<(usize, String)> = j[key]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| {
                    (
                        self.index["minecraft:block"][k],
                        format!("({}, {})", block(k), block(v.as_str().unwrap())),
                    )
                })
                .collect();
            m.sort();
            writeln!(
                o,
                "pub static {name}: [(Block, Block); {}] = [{}];",
                m.len(),
                m.iter().map(|x| x.1.clone()).collect::<Vec<_>>().join(", ")
            )
            .unwrap();
        }

        let rules: Vec<String> = j["game_rules"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, r)| {
                assert_eq!(int(r, "id") as usize, i);
                let (_ty, default) = if s(r, "type") == "BOOL" {
                    (
                        "Bool",
                        format!("GameRuleValue::Bool({})", flag(r, "default")),
                    )
                } else {
                    ("Int", format!("GameRuleValue::Int({})", int(r, "default")))
                };
                format!(
                    "GameRuleData {{ default: {default}, min: {}, max: {}, category: {} }}",
                    opt(r.get("min").map(|v| v.as_i64().unwrap().to_string())),
                    opt(r.get("max").map(|v| v.as_i64().unwrap().to_string())),
                    lit(s(r, "category"))
                )
            })
            .collect();
        writeln!(
            o,
            "pub static GAME_RULE_DATA: [GameRuleData; {}] = [{}];",
            rules.len(),
            rules.join(",\n    ")
        )
        .unwrap();

        let mc: Vec<String> = j["map_colors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| format!("({}, {})", int(c, "id"), int(c, "col")))
            .collect();
        writeln!(
            o,
            "pub static MAP_COLORS: [(u8, i32); {}] = [{}];",
            mc.len(),
            mc.join(", ")
        )
        .unwrap();

        let dyes: Vec<String> = j["dye_colors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                format!(
                    "DyeColorData {{ id: {}, name: {}, map_color: {}, terracotta_color: {}, texture_diffuse_color: {}, firework_color: {}, text_color: {} }}",
                    int(d, "id"),
                    lit(s(d, "name")),
                    int(d, "map_color"),
                    int(d, "terracotta_color"),
                    int(d, "texture_diffuse_color"),
                    int(d, "firework_color"),
                    int(d, "text_color")
                )
            })
            .collect();
        writeln!(
            o,
            "pub static DYE_COLORS: [DyeColorData; {}] = [{}];",
            dyes.len(),
            dyes.join(",\n    ")
        )
        .unwrap();

        let inst: Vec<String> = j["note_block_instruments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| {
                format!(
                    "NoteBlockInstrumentData {{ instrument: crate::enums::NoteBlockInstrument::{}, sound: {}, tunable: {}, has_custom_sound: {}, works_above_note_block: {} }}",
                    camel(s(n, "const")),
                    self.id("minecraft:sound_event", s(n, "sound")),
                    flag(n, "tunable"),
                    flag(n, "has_custom_sound"),
                    flag(n, "works_above_note_block")
                )
            })
            .collect();
        writeln!(
            o,
            "pub static NOTE_BLOCK_INSTRUMENTS: [NoteBlockInstrumentData; {}] = [{}];",
            inst.len(),
            inst.join(",\n    ")
        )
        .unwrap();

        let hm: Vec<String> = j["heightmap_types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| format!("HeightmapTypeData {{ ty: crate::enums::HeightmapTypes::{}, serialized_name: {}, usage: crate::enums::HeightmapUsage::{} }}", camel(s(h, "const")), lit(s(h, "serialized_name")), camel(s(h, "usage"))))
            .collect();
        writeln!(
            o,
            "pub static HEIGHTMAP_TYPES: [HeightmapTypeData; {}] = [{}];",
            hm.len(),
            hm.join(", ")
        )
        .unwrap();

        let poi: Vec<String> = j["poi_types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                format!(
                    "PoiTypeData {{ name: {}, matching_states: &[{}], max_tickets: {}, valid_range: {} }}",
                    lit(s(p, "name")),
                    p["matching_states"].as_array().unwrap().iter().map(|x| x.as_i64().unwrap().to_string()).collect::<Vec<_>>().join(", "),
                    int(p, "max_tickets"),
                    int(p, "valid_range")
                )
            })
            .collect();
        writeln!(
            o,
            "pub static POI_TYPES: [PoiTypeData; {}] = [{}];",
            poi.len(),
            poi.join(",\n    ")
        )
        .unwrap();

        let poses: Vec<String> = j["avatar_pose_dimensions"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, d)| {
                format!(
                    "(crate::enums::Pose::{}, {}, {}, {}, {})",
                    camel(k),
                    f32_lit(num(d, "width")),
                    f32_lit(num(d, "height")),
                    f32_lit(num(d, "eye_height")),
                    flag(d, "fixed")
                )
            })
            .collect();
        writeln!(o, "pub static AVATAR_POSE_DIMENSIONS: [(crate::enums::Pose, f32, f32, f32, bool); {}] = [{}];", poses.len(), poses.join(", ")).unwrap();

        let sers: Vec<String> = j["entity_data_serializers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| lit(x["const"].as_str().unwrap_or("")))
            .collect();
        writeln!(
            o,
            "pub static ENTITY_DATA_SERIALIZERS: [&str; {}] = [{}];",
            sers.len(),
            sers.join(", ")
        )
        .unwrap();
        let accs: Vec<String> = j["entity_data_accessors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                format!(
                    "EntityDataAccessorDef {{ class: {}, field: {}, id: {}, serializer: {}, serializer_id: {} }}",
                    lit(s(a, "class")),
                    lit(s(a, "field")),
                    int(a, "id"),
                    lit(s(a, "serializer")),
                    int(a, "serializer_id")
                )
            })
            .collect();
        writeln!(
            o,
            "pub static ENTITY_DATA_ACCESSORS: [EntityDataAccessorDef; {}] = [{}];",
            accs.len(),
            accs.join(",\n    ")
        )
        .unwrap();
        self.write("misc.rs", o);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// vanilla data pack + assets, embedded as an archive of individually deflated files

fn collect(dir: &Path, rel: &str, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().into_string().unwrap();
        let path = format!("{rel}/{name}");
        if e.file_type().unwrap().is_dir() {
            collect(&e.path(), &path, out);
        } else {
            out.push((path, e.path()));
        }
    }
}

fn datapack(src: &Path, out: &Path) {
    use flate2::{Compression, write::DeflateEncoder};
    use std::io::Write;
    let mut files = Vec::new();
    collect(&src.join("data"), "data", &mut files);
    collect(&src.join("assets"), "assets", &mut files);
    files.push(("version.json".to_string(), src.join("version.json")));
    files.sort();
    let mut blob: Vec<u8> = Vec::new();
    let mut index = String::new();
    for (path, file) in &files {
        let raw = fs::read(file).unwrap();
        // .nbt structures are already gzip-compressed; tiny files are not worth the deflate header
        let (compressed, stored) = if path.ends_with(".nbt") || raw.len() < 64 {
            (false, raw.clone())
        } else {
            let mut e = DeflateEncoder::new(Vec::new(), Compression::new(3));
            e.write_all(&raw).unwrap();
            (true, e.finish().unwrap())
        };
        writeln!(
            index,
            "({}, {compressed}, {}, {}, {}),",
            lit(path),
            blob.len(),
            stored.len(),
            raw.len()
        )
        .unwrap();
        blob.extend_from_slice(&stored);
    }
    fs::write(out.join("datapack.bin"), &blob).unwrap();
    fs::write(
        out.join("datapack_index.rs"),
        format!(
            "pub static INDEX: [(&str, bool, u32, u32, u32); {}] = [\n{index}];\n",
            files.len()
        ),
    )
    .unwrap();
}
