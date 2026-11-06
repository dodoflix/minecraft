//! The environment attribute system against the REAL vanilla `EnvironmentAttributeSystem`
//! (`tests/oracle/AttributeOracle.java` -> `tests/fixtures/attribute_oracle.txt`): every attribute of the overworld, nether and
//! end sampled at 18 world clock values x 5 weather levels x 5 positions (biome layers, timelines, weather layers, modifiers,
//! ranges, defaults), compared as canonical text.
//! Regenerate: `scripts/entity-oracle.sh crates/mc-game/tests/oracle/AttributeOracle.java > crates/mc-game/tests/fixtures/attribute_oracle.txt`.

use std::collections::HashMap;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::math::java::java_float_to_string as fs;
use mc_game::registry::codec::{Lookup, RegistryDecodeContext, RegistryLookup};
use mc_game::registry::typed::HolderSetRef;
use mc_game::registry::typed::dimension_type::DimensionType;
use mc_game::world::attribute::{
    AttrValue, AttributeEnv, EnvironmentAttributeSystem, Timeline, TriState,
};
use mc_game::world::clock::ClockTotals;
use mc_world::palette::BiomeId;
use serde_json::Value;

struct AllKnown;
impl RegistryLookup for AllKnown {
    fn contains(&self, _: &str, _: &Identifier) -> Lookup {
        Lookup::Yes
    }
    fn tag_exists(&self, _: &str, _: &Identifier) -> Lookup {
        Lookup::Yes
    }
}

fn json(path: &str) -> Value {
    serde_json::from_str(
        &mc_data::datapack::read_string(path).unwrap_or_else(|| panic!("missing {path}")),
    )
    .unwrap()
}

/// Resolves a `timeline` tag (nested tags expanded in place).
fn timeline_ids(entry: &str, out: &mut Vec<String>) {
    if let Some(tag) = entry.strip_prefix('#') {
        let id = Identifier::parse(tag).unwrap();
        for v in json(&format!(
            "data/{}/tags/timeline/{}.json",
            id.namespace(),
            id.path()
        ))["values"]
            .as_array()
            .unwrap()
        {
            timeline_ids(v.as_str().unwrap(), out);
        }
    } else {
        out.push(entry.to_owned());
    }
}

fn format(value: &AttrValue) -> String {
    match value {
        AttrValue::Bool(b) => b.to_string(),
        AttrValue::Float(f) => fs(*f),
        AttrValue::Int(i) => i.to_string(),
        AttrValue::TriState(t) => match t {
            TriState::True => "true",
            TriState::False => "false",
            TriState::Default => "default",
        }
        .into(),
        AttrValue::MoonPhase(i) => {
            mc_game::world::attribute::value::MOON_PHASES[usize::from(*i)].into()
        }
        AttrValue::Activity(id) => id.to_string(),
        AttrValue::Rgb(c) => format!("{},{},{}", fs(c[0]), fs(c[1]), fs(c[2])),
        AttrValue::Argb(c) => format!("{},{},{},{}", fs(c[0]), fs(c[1]), fs(c[2]), fs(c[3])),
        AttrValue::BedRule(r) => {
            let name = |rule| match rule {
                mc_game::world::attribute::BedRuleRule::Always => "always",
                mc_game::world::attribute::BedRuleRule::WhenDark => "when_dark",
                mc_game::world::attribute::BedRuleRule::Never => "never",
            };
            let error = r
                .error_message
                .as_ref()
                .map_or("-".to_owned(), |m| serde_json::to_string(m).unwrap());
            format!(
                "{}/{}/{}/{}/{}",
                name(r.can_sleep),
                name(r.can_set_spawn),
                r.destroy_on_use,
                r.destroy_on_leave,
                error
            )
        }
        AttrValue::MobSpawnSettings(s) => {
            let mut text = String::from("categories:");
            for category in mc_game::world::attribute::value::MOB_CATEGORIES {
                if let Some(list) = s.spawns_by_category.get(category) {
                    text.push_str(&format!("{category}={},", list.len()));
                }
            }
            format!("{text};costs:{}", s.spawn_costs.len())
        }
        AttrValue::AmbientParticles(l) => format!("list{}", l.len()),
        AttrValue::Particle(_) => "set".into(),
        AttrValue::BackgroundMusic(v) | AttrValue::AmbientSounds(v) => {
            if v.as_object().is_some_and(|o| o.is_empty()) {
                "empty"
            } else {
                "set"
            }
            .into()
        }
    }
}

#[test]
fn environment_attributes_match_vanilla() {
    let fixture = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../mc-game/tests/fixtures/attribute_oracle.txt"
    ))
    .unwrap();
    // biome ids = order of first appearance; probes per dimension: (x, z) -> (idx, full biome, noise biome)
    let mut biome_ids: HashMap<String, u32> = HashMap::new();
    let mut probes: HashMap<String, Vec<(i32, i32, String, String)>> = HashMap::new();
    for line in fixture.lines().filter(|l| l.starts_with("B ")) {
        let t: Vec<&str> = line.split(' ').collect();
        let (full, noise) = (
            t[6].strip_prefix("full=").unwrap().to_owned(),
            t[7].strip_prefix("noise=").unwrap().to_owned(),
        );
        for b in [&full, &noise] {
            let next = biome_ids.len() as u32;
            biome_ids.entry(b.clone()).or_insert(next);
        }
        probes.entry(t[1].to_owned()).or_default().push((
            t[3].parse().unwrap(),
            t[5].parse().unwrap(),
            full,
            noise,
        ));
    }
    let mut biome_json: Vec<Option<Value>> = vec![None; biome_ids.len()];
    for (name, id) in &biome_ids {
        let path = format!(
            "data/minecraft/worldgen/biome/{}.json",
            name.strip_prefix("minecraft:").unwrap()
        );
        biome_json[*id as usize] = json(&path).get("attributes").cloned();
    }
    let biome_attributes: Vec<Option<&Value>> = biome_json.iter().map(Option::as_ref).collect();

    let mut systems: HashMap<
        String,
        (
            EnvironmentAttributeSystem,
            Arc<ClockTotals>,
            Option<Identifier>,
            bool,
        ),
    > = HashMap::new();
    for dim in ["overworld", "the_nether", "the_end"] {
        let dimension_json = json(&format!("data/minecraft/dimension_type/{dim}.json"));
        let ctx_lookup = AllKnown;
        let ctx = RegistryDecodeContext::new(&ctx_lookup);
        let dimension_type = DimensionType::from_json(&dimension_json, &ctx).unwrap();
        let mut ids = Vec::new();
        match &dimension_type.timelines {
            HolderSetRef::Tag(tag) => timeline_ids(&format!("#{tag}"), &mut ids),
            HolderSetRef::Elements(list) => ids.extend(list.iter().map(|i| i.to_string())),
        }
        let timelines: Vec<Timeline> = ids
            .iter()
            .map(|id| {
                let id = Identifier::parse(id).unwrap();
                Timeline::from_json(&json(&format!(
                    "data/{}/timeline/{}.json",
                    id.namespace(),
                    id.path()
                )))
                .unwrap_or_else(|| panic!("timeline {id}"))
            })
            .collect();
        let can_have_weather =
            dimension_type.has_sky_light && !dimension_type.has_ceiling && dim != "the_end";
        let clocks = Arc::new(ClockTotals::default());
        let system = EnvironmentAttributeSystem::build(
            &dimension_type.attributes,
            &biome_attributes,
            &timelines,
            can_have_weather,
            clocks.clone(),
        );
        systems.insert(
            format!("minecraft:{dim}"),
            (
                system,
                clocks,
                dimension_type.default_clock.clone(),
                can_have_weather,
            ),
        );
    }

    let (mut checked, mut failures) = (0, Vec::new());
    for line in fixture.lines().filter(|l| l.starts_with("V ")) {
        let t: Vec<&str> = line.splitn(7, ' ').collect();
        let (dim, ticks, rain, thunder, attr, expected) = (
            t[1],
            t[2].parse::<i64>().unwrap(),
            t[3].parse::<f32>().unwrap(),
            t[4].parse::<f32>().unwrap(),
            t[5],
            t[6],
        );
        let (system, clocks, clock, _) = &systems[dim];
        if let Some(clock) = clock {
            clocks.set(clock, ticks);
        }
        system.invalidate_tick_cache();
        // `Level.getThunderLevel(1.0F)` = thunder * rain
        let (rain_level, thunder_level) = (rain, thunder * rain);
        let dim_probes = &probes[dim];
        let def = mc_game::registry::typed::attributes::ATTRIBUTES
            .iter()
            .find(|d| d.name == attr)
            .unwrap();
        let values: Vec<String> = if def.positional {
            dim_probes
                .iter()
                .map(|(x, z, _, _)| {
                    let biome_at = |pos: [f64; 3], full: bool| {
                        let (px, pz) = (pos[0].floor() as i32, pos[2].floor() as i32);
                        dim_probes
                            .iter()
                            .find(|p| p.0 == px && p.1 == pz)
                            .map(|p| BiomeId(biome_ids[if full { &p.2 } else { &p.3 }]))
                    };
                    let env = AttributeEnv {
                        rain_level,
                        thunder_level,
                        biome_at: &biome_at,
                    };
                    format(&system.get_value(
                        attr,
                        [f64::from(*x) + 0.5, 64.5, f64::from(*z) + 0.5],
                        &env,
                    ))
                })
                .collect()
        } else {
            let biome_at = |_: [f64; 3], _: bool| None;
            let env = AttributeEnv {
                rain_level,
                thunder_level,
                biome_at: &biome_at,
            };
            vec![format(&system.get_dimension_value(attr, &env))]
        };
        let actual = if values.iter().all(|v| *v == values[0]) {
            values[0].clone()
        } else {
            values.join("|")
        };
        checked += 1;
        if actual != expected {
            failures.push(format!("{dim} ticks={ticks} rain={rain} thunder={thunder} {attr}\n  expected {expected}\n  actual   {actual}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} differ:\n{}",
        failures.len(),
        failures
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(checked > 9000);
}
