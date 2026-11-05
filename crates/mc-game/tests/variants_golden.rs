//! Mob variants vs the real vanilla (fixture from `scripts/golden-variants.sh`, `tests/oracle/VariantsOracle.java`): every entry of
//! the 12 variant registries (typed value re-encoded with `DIRECT_CODEC`, and the `registry_data` network tag = `NETWORK_CODEC`),
//! `Registry.getRandom` (random sound variants) and `PriorityProvider.pick` over biome x moon phase x structure contexts.
mod common;

use std::collections::HashMap;

use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::random::LegacyRandomSource;
use mc_game::registry::{RegistryAccess, keys};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::animal::chicken::{ChickenSoundVariant, ChickenVariant};
use mc_game::world::entity::animal::cow::{CowSoundVariant, CowVariant};
use mc_game::world::entity::animal::feline::{CatSoundVariant, CatVariant};
use mc_game::world::entity::animal::frog::FrogVariant;
use mc_game::world::entity::animal::nautilus::ZombieNautilusVariant;
use mc_game::world::entity::animal::pig::{PigSoundVariant, PigVariant};
use mc_game::world::entity::animal::wolf::{WolfSoundVariant, WolfVariant};
use mc_game::world::entity::variant::{SpawnContext, SpawnContextLevel, pick_random_entry, select_variant_to_spawn};
use mc_nbt::Tag;
use mc_world::level_storage::WorldDataConfiguration;
use sha1::{Digest, Sha1};

const MOON_BRIGHTNESS_PER_PHASE: [f32; 8] = [1.0, 0.75, 0.5, 0.25, 0.0, 0.25, 0.5, 0.75];

fn canon(tag: &Tag, out: &mut String) {
    use std::fmt::Write;
    match tag {
        Tag::Compound(c) => {
            let mut entries: Vec<_> = c.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            out.push('{');
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write!(out, "{k}:").unwrap();
                canon(v, out);
            }
            out.push('}');
        }
        Tag::List(l) => {
            out.push('[');
            for (i, v) in l.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canon(v, out);
            }
            out.push(']');
        }
        Tag::Byte(v) => write!(out, "{v}b").unwrap(),
        Tag::Short(v) => write!(out, "{v}s").unwrap(),
        Tag::Int(v) => write!(out, "{v}i").unwrap(),
        Tag::Long(v) => write!(out, "{v}l").unwrap(),
        Tag::Float(v) => write!(out, "{:x}f", v.to_bits()).unwrap(),
        Tag::Double(v) => write!(out, "{:x}d", v.to_bits()).unwrap(),
        Tag::String(s) => write!(out, "\"{s}\"").unwrap(),
        other => panic!("unexpected tag {other:?}"),
    }
}

fn digest(tag: &Tag) -> String {
    let mut s = String::new();
    canon(tag, &mut s);
    Sha1::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// The level side of a spawn context: registries, a moon phase and the structures with a piece at the position.
struct StubLevel<'a> {
    registries: &'a RegistryAccess,
    moon_phase: usize,
    structures: Vec<Identifier>,
}

impl SpawnContextLevel for StubLevel<'_> {
    fn variant_registries(&self) -> &RegistryAccess {
        self.registries
    }
    fn biome_at(&self, _pos: BlockPos) -> Option<Identifier> {
        None
    }
    fn moon_brightness_at(&self, _pos: BlockPos) -> f32 {
        MOON_BRIGHTNESS_PER_PHASE[self.moon_phase]
    }
    fn has_structure_piece_at(&self, _pos: BlockPos, matcher: &dyn Fn(&Identifier) -> bool) -> bool {
        self.structures.iter().any(|s| matcher(s))
    }
}

/// Typed access to one variant registry: the `DIRECT_CODEC` encoding of an entry and the selection over it.
struct Reg {
    key: &'static str,
    name: &'static str,
    direct: fn(&RegistryAccess, &Identifier) -> Tag,
    select: Option<fn(&SpawnContext<'_>, &mut LegacyRandomSource) -> Option<Identifier>>,
}

macro_rules! reg {
    ($key:expr, $name:literal, $ty:ty, $codec:expr, select) => {
        Reg {
            key: $key,
            name: $name,
            direct: |r, id| mc_dfu::encode_nbt(&$codec, r.get::<$ty>($key, id).expect("typed entry")).into_result().expect("encodes"),
            select: Some(|c, rng| select_variant_to_spawn::<$ty>(c, $key, rng)),
        }
    };
    ($key:expr, $name:literal, $ty:ty, $codec:expr) => {
        Reg {
            key: $key,
            name: $name,
            direct: |r, id| mc_dfu::encode_nbt(&$codec, r.get::<$ty>($key, id).expect("typed entry")).into_result().expect("encodes"),
            select: None,
        }
    };
}

fn regs() -> Vec<Reg> {
    vec![
        reg!(keys::WOLF_VARIANT, "wolf_variant", WolfVariant, WolfVariant::direct_codec(), select),
        reg!(keys::WOLF_SOUND_VARIANT, "wolf_sound_variant", WolfSoundVariant, WolfSoundVariant::codec()),
        reg!(keys::CAT_VARIANT, "cat_variant", CatVariant, CatVariant::direct_codec(), select),
        reg!(keys::CAT_SOUND_VARIANT, "cat_sound_variant", CatSoundVariant, CatSoundVariant::codec()),
        reg!(keys::FROG_VARIANT, "frog_variant", FrogVariant, FrogVariant::direct_codec(), select),
        reg!(keys::PIG_VARIANT, "pig_variant", PigVariant, PigVariant::direct_codec(), select),
        reg!(keys::PIG_SOUND_VARIANT, "pig_sound_variant", PigSoundVariant, PigSoundVariant::codec()),
        reg!(keys::COW_VARIANT, "cow_variant", CowVariant, CowVariant::direct_codec(), select),
        reg!(keys::COW_SOUND_VARIANT, "cow_sound_variant", CowSoundVariant, CowSoundVariant::codec()),
        reg!(keys::CHICKEN_VARIANT, "chicken_variant", ChickenVariant, ChickenVariant::direct_codec(), select),
        reg!(keys::CHICKEN_SOUND_VARIANT, "chicken_sound_variant", ChickenSoundVariant, ChickenSoundVariant::codec()),
        reg!(keys::ZOMBIE_NAUTILUS_VARIANT, "zombie_nautilus_variant", ZombieNautilusVariant, ZombieNautilusVariant::direct_codec(), select),
    ]
}

#[test]
fn variants_equal_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_config, stem) = common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true).expect("default world loads");
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = stem.registries.composite_access().clone();
    let registries = &level.registries;
    let regs = regs();

    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/variants_golden.txt")).unwrap();
    let mut failures: Vec<String> = Vec::new();
    let mut digests: HashMap<(char, String, String), String> = HashMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        if matches!(f[0], "E" | "N") {
            digests.insert((f[0].chars().next().unwrap(), f[1].to_owned(), f[2].to_owned()), f[3].to_owned());
        }
    }

    // registry contents: DIRECT_CODEC re-encoding and the registry_data tag
    let mut checked = 0;
    for r in &regs {
        for entry in registries.registry(r.key).expect("registry").entries() {
            let id = entry.id.to_string();
            let direct = digest(&(r.direct)(registries, &entry.id));
            if digests.get(&('E', r.name.to_owned(), id.clone())) != Some(&direct) {
                failures.push(format!("E {} {id}: ours {direct}", r.name));
            }
            let network = entry.network.as_ref().map(digest);
            if digests.get(&('N', r.name.to_owned(), id.clone())) != network.as_ref() {
                failures.push(format!("N {} {id}: ours {network:?}", r.name));
            }
            checked += 1;
        }
    }
    assert_eq!(checked * 2, digests.len(), "entry counts differ");

    // Registry.getRandom and the selections
    let mut context: Option<(String, usize, Vec<Identifier>, BlockPos)> = None;
    let (mut selections, mut randoms) = (0, 0);
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "P" => {
                let r = regs.iter().find(|r| r.name == f[1]).unwrap();
                let got = pick_random_entry(registries, r.key, &mut LegacyRandomSource::new(f[2].parse().unwrap())).map_or("-".to_owned(), |i| i.to_string());
                if got != f[3] {
                    failures.push(format!("P {} {}: vanilla {} ours {got}", f[1], f[2], f[3]));
                }
                randoms += 1;
            }
            "C" => {
                let xyz: Vec<i32> = f[3].split(',').map(|v| v.parse().unwrap()).collect();
                let structures = if f[4] == "-" { Vec::new() } else { f[4].split(',').map(|s| Identifier::parse(s).unwrap()).collect() };
                context = Some((f[1].to_owned(), f[2].parse().unwrap(), structures, BlockPos::new(xyz[0], xyz[1], xyz[2])));
            }
            "S" => {
                let (biome, phase, structures, pos) = context.clone().expect("context before selection");
                let stub = StubLevel { registries, moon_phase: phase, structures };
                let ctx = SpawnContext::new(pos, &stub, Some(Identifier::parse(&biome).unwrap()));
                let r = regs.iter().find(|r| r.name == f[1]).unwrap();
                let got = (r.select.unwrap())(&ctx, &mut LegacyRandomSource::new(f[2].parse().unwrap())).map_or("-".to_owned(), |i| i.to_string());
                if got != f[3] {
                    failures.push(format!("S {} seed {} in {biome} moon {phase} at {pos:?}: vanilla {} ours {got}", f[1], f[2], f[3]));
                }
                selections += 1;
            }
            _ => {}
        }
    }
    assert!(selections > 20000 && randoms == 72, "{selections} selections, {randoms} random picks");
    assert!(failures.is_empty(), "{} differ:\n{}", failures.len(), failures.iter().take(15).cloned().collect::<Vec<_>>().join("\n"));
}
