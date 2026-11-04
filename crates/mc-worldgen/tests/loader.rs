//! Loading of the vanilla worldgen JSON (`ref/src/data`) and the codec error cases.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_worldgen::data::{ClosureDataSource, FsDataSource, WorldgenRegistries};
use mc_worldgen::densityfunction::codec::density_function_from_json;
use mc_worldgen::densityfunction::cubic_spline::CubicSpline;
use mc_worldgen::densityfunction::generator::ConstantFunction;
use mc_worldgen::densityfunction::op::{UnaryFunction, UnaryType};
use mc_worldgen::densityfunction::{DensityFunction, DfNode};
use serde_json::{Value, json};

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data")
}

fn vanilla() -> WorldgenRegistries {
    WorldgenRegistries::new(Arc::new(FsDataSource::new(data_root())))
}

fn id(s: &str) -> Identifier {
    Identifier::parse(s).expect("valid id")
}

/// Ids of every `<dir>/**/*.json` of the `minecraft` namespace, e.g. `minecraft:overworld/caves/entrances`.
fn ids_in(registry: &str) -> Vec<Identifier> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<Identifier>) {
        for entry in std::fs::read_dir(dir).expect("readable dir") {
            let entry = entry.expect("dir entry");
            let name = entry.file_name().into_string().expect("utf-8 name");
            if entry.file_type().expect("file type").is_dir() {
                walk(&entry.path(), &format!("{prefix}{name}/"), out);
            } else if let Some(stem) = name.strip_suffix(".json") {
                out.push(
                    Identifier::new("minecraft", &format!("{prefix}{stem}")).expect("valid id"),
                );
            }
        }
    }
    let mut ids = Vec::new();
    walk(&data_root().join("minecraft").join(registry), "", &mut ids);
    ids
}

/// Registries over in-memory density functions `test:<name>`.
fn in_memory(entries: Vec<(&'static str, Value)>) -> WorldgenRegistries {
    WorldgenRegistries::new(Arc::new(ClosureDataSource::new(
        move |registry: &str, id: &Identifier| {
            if registry != "worldgen/density_function" || id.namespace() != "test" {
                return None;
            }
            entries
                .iter()
                .find(|(name, _)| *name == id.path())
                .map(|(_, v)| v.clone())
        },
    )))
}

fn decode(json: Value) -> Result<DensityFunction, String> {
    density_function_from_json(&json, &in_memory(vec![])).map_err(|e| e.to_string())
}

fn constant(f: &DensityFunction) -> f32 {
    match f.node() {
        DfNode::Constant(ConstantFunction { value }) => *value,
        other => panic!("expected constant, got {other:?}"),
    }
}

#[test]
fn loads_all_vanilla_noises() {
    let registries = vanilla();
    let ids = ids_in("worldgen/noise");
    assert_eq!(ids.len(), 64);
    for noise in &ids {
        registries.noise(noise).unwrap_or_else(|e| panic!("{e}"));
    }
    let a = registries
        .noise(&id("minecraft:continentalness"))
        .expect("noise");
    let b = registries.noise(&id("continentalness")).expect("noise");
    assert!(
        Arc::ptr_eq(&a.value, &b.value),
        "noise holders are memoized"
    );
}

#[test]
fn loads_all_vanilla_density_functions() {
    let registries = vanilla();
    let ids = ids_in("worldgen/density_function");
    assert_eq!(ids.len(), 55);
    assert!(ids.contains(&id("minecraft:overworld/caves/entrances")));
    for function in &ids {
        registries
            .density_function(function)
            .unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn loads_all_vanilla_noise_settings() {
    let registries = vanilla();
    let ids = ids_in("worldgen/noise_settings");
    assert_eq!(ids.len(), 7);
    for settings in &ids {
        registries
            .noise_settings(settings)
            .unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn noise_settings_shapes() {
    let registries = vanilla();
    let overworld = registries
        .noise_settings(&id("overworld"))
        .expect("overworld");
    assert_eq!((overworld.noise.min_y, overworld.noise.height), (-64, 384));
    assert_eq!(overworld.sea_level, 63);
    assert!(!overworld.legacy_random_source);
    assert!(!overworld.disable_mob_generation);
    assert!(overworld.aquifers.is_some());
    assert_eq!(overworld.default_block.name, id("minecraft:stone"));
    assert_eq!(overworld.default_fluid.name, id("minecraft:water"));
    assert!(overworld.default_block.properties.is_empty());
    assert_eq!(overworld.material_rule, json!("minecraft:overworld"));
    assert_eq!(overworld.spawn_target.as_array().map(Vec::len), Some(2));
    let labels: Vec<&str> = overworld
        .debug_functions
        .iter()
        .map(|(label, _)| label.as_str())
        .collect();
    assert_eq!(labels, ["N", "T", "V", "C", "E", "D", "W", "PV", "PS"]);

    // The router fields are references to registered functions, shared with direct loads.
    let continents = registries
        .density_function(&id("minecraft:overworld/continents"))
        .expect("continents");
    match overworld.noise_router.continents.node() {
        DfNode::HolderHolder(h) => {
            assert_eq!(h.function.key, id("minecraft:overworld/continents"));
            assert!(h.function.value.ptr_eq(&continents.value));
        }
        other => panic!("expected reference, got {other:?}"),
    }

    for name in ["nether", "end"] {
        let settings = registries.noise_settings(&id(name)).expect(name);
        assert!(settings.legacy_random_source, "{name}");
        assert!(settings.aquifers.is_none(), "{name}");
    }
    let nether = registries.noise_settings(&id("nether")).expect("nether");
    assert_eq!(
        (nether.noise.min_y, nether.noise.height, nether.sea_level),
        (0, 128, 32)
    );
    assert_eq!(nether.default_block.name, id("minecraft:netherrack"));
    let end = registries.noise_settings(&id("end")).expect("end");
    assert!(end.disable_mob_generation);
    assert_eq!(end.default_fluid.name, id("minecraft:air"));
    let floating = registries
        .noise_settings(&id("floating_islands"))
        .expect("floating_islands");
    assert_eq!((floating.noise.min_y, floating.noise.height), (0, 256));
}

#[test]
fn references_share_one_function() {
    let registries = vanilla();
    let reference = json!("minecraft:overworld/continents");
    let a = density_function_from_json(&reference, &registries).expect("a");
    let b = density_function_from_json(&reference, &registries).expect("b");
    let (DfNode::HolderHolder(a), DfNode::HolderHolder(b)) = (a.node(), b.node()) else {
        panic!("references decode to HolderHolder");
    };
    assert!(a.function.value.ptr_eq(&b.function.value));
    let direct = registries
        .density_function(&id("minecraft:overworld/continents"))
        .expect("direct");
    assert!(direct.value.ptr_eq(&a.function.value));
}

#[test]
fn shorthands() {
    // A bare number is a constant, rounded to float.
    assert_eq!(
        constant(&decode(json!(-0.6666667)).expect("number")).to_bits(),
        (-0.6666667f32).to_bits()
    );
    assert_eq!(constant(&decode(json!(3)).expect("integer")), 3.0);
    // `type` with or without namespace, and the explicit `constant` object, decode identically.
    let expect = DensityFunction::new(DfNode::Unary(UnaryFunction {
        ty: UnaryType::Abs,
        input: DensityFunction::constant(1.0),
    }));
    assert_eq!(
        decode(json!({"type": "minecraft:abs", "input": 1})).expect("namespaced"),
        expect
    );
    assert_eq!(
        decode(json!({"type": "abs", "input": {"type": "constant", "value": 1.0}})).expect("bare"),
        expect
    );
    // String references resolve through the registry.
    let registries = in_memory(vec![("one", json!(1.0))]);
    let reference = density_function_from_json(&json!("test:one"), &registries).expect("reference");
    let DfNode::HolderHolder(holder) = reference.node() else {
        panic!("expected HolderHolder")
    };
    assert_eq!(holder.function.key, id("test:one"));
    assert_eq!(constant(&holder.function.value), 1.0);
}

#[test]
fn optional_fields_take_defaults() {
    let f = decode(json!({"type": "round", "input": 2.5})).expect("round");
    let DfNode::Round(round) = f.node() else {
        panic!("expected Round")
    };
    assert_eq!(constant(&round.multiple), 1.0);

    let f = decode(
        json!({"type": "gradient", "axis": "y", "from_coordinate": -8, "to_coordinate": 8,
                          "from_value": 1.0, "to_value": -1.0}),
    )
    .expect("gradient");
    let DfNode::Gradient(g) = f.node() else {
        panic!("expected Gradient")
    };
    assert_eq!(
        g.tiling,
        mc_worldgen::densityfunction::generator::TilingMode::ClampToEdge
    );
    assert_eq!(
        (g.from_coordinate, g.to_coordinate, g.from_value, g.to_value),
        (-8, 8, 1.0, -1.0)
    );
}

#[test]
fn spline_shape() {
    let f = decode(json!({"type": "minecraft:spline", "spline": {
    "coordinate": 0.5,
    "points": [
        {"location": -1.0, "value": 0.25, "derivative": 0.0},
        {"location": 1.0, "derivative": 2.0, "value": {
            "coordinate": {"type": "abs", "input": 1},
            "points": [{"location": 0.0, "value": 7, "derivative": 0.5}]}}
    ]}}))
    .expect("spline");
    let DfNode::Spline(spline) = f.node() else {
        panic!("expected Spline")
    };
    let CubicSpline::Multipoint(m) = &spline.spline else {
        panic!("expected Multipoint")
    };
    assert_eq!(constant(&m.coordinate), 0.5);
    assert_eq!(m.locations, [-1.0, 1.0]);
    assert_eq!(m.derivatives, [0.0, 2.0]);
    assert!(matches!(m.values[0], CubicSpline::Constant(v) if v == 0.25));
    let CubicSpline::Multipoint(inner) = &m.values[1] else {
        panic!("expected nested Multipoint")
    };
    assert_eq!(inner.derivatives, [0.5]);
    assert!(matches!(inner.values[0], CubicSpline::Constant(v) if v == 7.0));
}

#[test]
fn rejects_malformed_functions() {
    let cases: Vec<(Value, &str)> = vec![
        (
            json!({"type": "minecraft:nonexistent"}),
            "type: Unknown registry key: minecraft:nonexistent",
        ),
        (
            json!({"type": "other:abs", "input": 1}),
            "type: Unknown registry key: other:abs",
        ),
        (json!({"input": 1}), "type: missing required field"),
        (json!({"type": "abs"}), "input: missing required field"),
        (
            json!({"type": "add", "left": 1}),
            "right: missing required field",
        ),
        (json!(true), "expected number, string or object"),
        (json!(2000000.0), "outside of range"),
        (json!("not an id!"), "Not a valid resource location"),
        (json!("test:missing"), "no such registry entry"),
        (
            json!({"type": "clamp", "input": 0, "min": 1.0, "max": 0.5}),
            "must be less than or equal to max",
        ),
        (
            json!({"type": "gradient", "axis": "y", "from_coordinate": 3, "to_coordinate": 3, "from_value": 0, "to_value": 1}),
            "from_coordinate cannot be equal to to_coordinate",
        ),
        (
            json!({"type": "gradient", "axis": "w", "from_coordinate": 0, "to_coordinate": 3, "from_value": 0, "to_value": 1}),
            "axis: Unknown element name: w",
        ),
        (
            json!({"type": "interpolated", "input": 0, "cell_size_xz": 0, "cell_size_y": 8}),
            "cell_size_xz: Value must be positive",
        ),
        (
            json!({"type": "find_top_surface", "density": 0, "upper_bound": 0, "lower_bound": 4063, "cell_height": 4}),
            "lower_bound: Value 4063 outside of range [-4064:4062]",
        ),
        (
            json!({"type": "distance_to_point", "point": [1, 2], "metric": "manhattan"}),
            "point: Input is not a list of 3 ints",
        ),
        (
            json!({"type": "noise", "noise": {"base_octave": 0}, "xz_scale": 1, "y_scale": 1}),
            "noise: inline noise definitions are not supported",
        ),
        (
            json!({"type": "old_blended_noise", "xz_scale": 0.0001, "y_scale": 1, "xz_factor": 1, "y_factor": 1,
                "smear_scale_multiplier": 2}),
            "xz_scale: Value 0.0001 outside of range [0.001:1000]",
        ),
        (
            json!({"type": "interval_select", "input": 0, "thresholds": [0.0, 1.0], "functions": [1, 2]}),
            "Expected 1 thresholds for 2 functions, but got 2",
        ),
        (
            json!({"type": "interval_select", "input": 0, "thresholds": [], "functions": [1]}),
            "functions: List is too short: 1, expected at least 2",
        ),
        (
            json!({"type": "interval_select", "input": 0, "thresholds": [1.0, 0.0], "functions": [1, 2, 3]}),
            "Threshold values must be ordered from smallest to largest",
        ),
        (
            json!({"type": "spline", "spline": {"coordinate": 0, "points": []}}),
            "spline.points: List must have contents",
        ),
        (
            json!({"type": "spline", "spline": {"coordinate": 0, "points": [
            {"location": 0, "derivative": 0, "value": 1},
            {"location": 1, "derivative": 0, "value": {"coordinate": 0, "points": [
                {"location": 0, "derivative": 0, "value": 1},
                {"location": 0, "derivative": 0, "value": 1},
                {"location": 0, "derivative": 0, "value": 1},
                {"location": 0, "derivative": 0, "value": "x"}]}}]}}),
            "spline.points[1].value.points[3].value: expected number or object",
        ),
    ];
    for (json, expected) in cases {
        let error = decode(json.clone()).expect_err(&format!("{json} should fail"));
        assert!(
            error.contains(expected),
            "{json}: got `{error}`, expected `{expected}`"
        );
    }
}

#[test]
fn errors_name_the_registry_entry() {
    let registries = in_memory(vec![(
        "bad",
        json!({"type": "mul", "left": 1, "right": {"type": "abs"}}),
    )]);
    let error = registries
        .density_function(&id("test:bad"))
        .expect_err("must fail")
        .to_string();
    assert_eq!(
        error,
        "worldgen/density_function/test:bad: right.input: missing required field"
    );
    let error = registries
        .density_function(&id("test:absent"))
        .expect_err("must fail")
        .to_string();
    assert_eq!(
        error,
        "worldgen/density_function/test:absent: no such registry entry"
    );
}

#[test]
fn rejects_reference_cycles() {
    let registries = in_memory(vec![
        ("a", json!({"type": "abs", "input": "test:b"})),
        ("b", json!({"type": "negate", "input": "test:a"})),
        ("self", json!({"type": "square", "input": "test:self"})),
    ]);
    for start in ["test:a", "test:self"] {
        let error = registries
            .density_function(&id(start))
            .expect_err("cycle")
            .to_string();
        assert!(error.contains("reference cycle"), "{start}: {error}");
    }
    // A failed load leaves nothing behind: it fails again identically.
    assert!(registries.density_function(&id("test:b")).is_err());
}

#[test]
fn noise_parameters_codec() {
    let noises = |entries: Vec<(&'static str, Value)>| {
        WorldgenRegistries::new(Arc::new(ClosureDataSource::new(
            move |registry: &str, id: &Identifier| {
                if registry != "worldgen/noise" {
                    return None;
                }
                entries
                    .iter()
                    .find(|(name, _)| *name == id.path())
                    .map(|(_, v)| v.clone())
            },
        )))
    };
    let registries = noises(vec![
        ("minimal", json!({"base_octave": -3})),
        (
            "legacy",
            json!({"base_octave": -3, "octave_count": 2, "normalize": "legacy", "amplitude_modifiers": [1.0, 0.5]}),
        ),
        (
            "disabled",
            json!({"base_octave": 0, "normalize": false, "base_amplitude": 2}),
        ),
        ("no_octave", json!({"octave_count": 2})),
        ("octave_range", json!({"base_octave": 33})),
        ("count_range", json!({"base_octave": 0, "octave_count": 0})),
        (
            "amplitude_range",
            json!({"base_octave": 0, "base_amplitude": 0.0}),
        ),
        (
            "bad_normalize",
            json!({"base_octave": 0, "normalize": "strict"}),
        ),
        (
            "modifier_size",
            json!({"base_octave": 0, "octave_count": 3, "amplitude_modifiers": [1.0]}),
        ),
        (
            "modifier_range",
            json!({"base_octave": 0, "amplitude_modifiers": [-1.0]}),
        ),
    ]);
    for ok in ["minimal", "legacy", "disabled"] {
        registries
            .noise(&id(&format!("test:{ok}")))
            .unwrap_or_else(|e| panic!("{ok}: {e}"));
    }
    let expected = [
        ("no_octave", "base_octave: missing required field"),
        (
            "octave_range",
            "base_octave: Value 33 outside of range [-32:32]",
        ),
        (
            "count_range",
            "octave_count: Value 0 outside of range [1:32]",
        ),
        (
            "amplitude_range",
            "base_amplitude: Value 0 outside of range",
        ),
        (
            "bad_normalize",
            "normalize: Invalid normalization type: strict",
        ),
        (
            "modifier_size",
            "amplitude_modifiers had size 1, but octave_count was 3",
        ),
        (
            "modifier_range",
            "amplitude_modifiers[0]: Value -1 outside of range [0:1000000]",
        ),
    ];
    for (name, message) in expected {
        let error = registries
            .noise(&id(&format!("test:{name}")))
            .expect_err(name)
            .to_string();
        assert!(
            error.starts_with(&format!("worldgen/noise/test:{name}: ")) && error.contains(message),
            "{name}: {error}"
        );
    }
}

#[test]
fn noise_settings_validation() {
    let router: Value = [
        "temperature",
        "vegetation",
        "continents",
        "erosion",
        "depth",
        "ridges",
        "chunk_surface_level",
        "final_density",
    ]
    .into_iter()
    .map(|name| (name.to_owned(), json!(0.0)))
    .collect::<serde_json::Map<_, _>>()
    .into();
    let settings = move |noise: Value| {
        json!({"noise": noise, "default_block": {"Name": "minecraft:stone", "Properties": {"a": "b"}},
               "default_fluid": "minecraft:water", "noise_router": router, "material_rule": "minecraft:overworld",
               "spawn_target": [], "sea_level": 5, "disable_mob_generation": true, "legacy_random_source": false})
    };
    let registries = WorldgenRegistries::new(Arc::new(ClosureDataSource::new(
        move |registry: &str, id: &Identifier| {
            (registry == "worldgen/noise_settings").then(|| match id.path() {
                "ok" => settings(json!({"min_y": -64, "height": 384})),
                "odd_min" => settings(json!({"min_y": -60, "height": 384})),
                "odd_height" => settings(json!({"min_y": 0, "height": 100})),
                "too_high" => settings(json!({"min_y": 1024, "height": 1024})),
                "bad_min" => settings(json!({"min_y": -3000, "height": 16})),
                _ => Value::Null,
            })
        },
    )));
    let ok = registries.noise_settings(&id("test:ok")).expect("ok");
    assert_eq!(
        ok.default_block.properties,
        [("a".to_owned(), "b".to_owned())]
    );
    assert!(ok.aquifers.is_none() && ok.debug_functions.is_empty());
    assert_eq!((ok.sea_level, ok.disable_mob_generation), (5, true));
    for (name, message) in [
        ("odd_min", "min_y has to be a multiple of 16"),
        ("odd_height", "height has to be a multiple of 16"),
        ("too_high", "min_y + height cannot be higher than: 2032"),
        (
            "bad_min",
            "noise.min_y: Value -3000 outside of range [-2032:2031]",
        ),
        ("null", "expected object"),
    ] {
        let error = registries
            .noise_settings(&id(&format!("test:{name}")))
            .expect_err(name)
            .to_string();
        assert!(error.contains(message), "{name}: {error}");
    }
}
