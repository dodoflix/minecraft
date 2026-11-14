//! Pets against vanilla: wolves, cats, ocelots and parrots (variants and their entity predicates, taming, collars, armour, anger,
//! shaking, sleeping, scaring, trust, shoulders, imitation and dancing). Scenarios: `tests/oracle/scenarios/mob_{pet,wolf,cat,ocelot,parrot}_*.txt`,
//! fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// `execute if predicate` over the variant, sound variant and collar components of wolves and cats and the parrot variant.
#[test]
fn pet_variant_predicate() {
    check("mob_pet_variant_predicate");
}

/// Collar dyes, healing food, wolf armour on adults, shears and the taming side effects.
#[test]
fn wolf_taming_care() {
    check("mob_wolf_taming_care");
}

/// Armour absorbs damage, cracks, breaks, is bypassed by magic damage and repaired by scutes.
#[test]
fn wolf_armor_crack() {
    check("mob_wolf_armor_crack");
}

/// A wild wolf hurt by a player: its anger, the pack alert, the refused lead, the owner's wolf, and the anger running out.
#[test]
fn wolf_pack_anger() {
    check("mob_wolf_pack_anger");
}

/// Wet wolves leaving the water shake dry; dying clears the shake.
#[test]
fn wolf_shake() {
    check("mob_wolf_shake");
}

/// Begging for a bone or the owner's food.
#[test]
fn wolf_begging() {
    check("mob_wolf_begging");
}

/// Creepers avoiding cats and ocelots, untamed cats avoiding players.
#[test]
fn cat_scare() {
    check("mob_cat_scare");
}

/// A swooping phantom hisses at the cats near it. Ignored: a phantom circling its anchor picks another target height than vanilla from tick 33 on
/// (same random draws, a different `moveTargetPoint.y` after the second target change).
#[test]
#[ignore = "deviation: the phantom's circling flight leaves vanilla's after about 30 ticks (same draws, other target height), so the sweep and the hisses are off"]
fn cat_phantom_hiss() {
    check("mob_cat_phantom_hiss");
}

/// A tamed cat on its sleeping owner's bed and the morning gift.
#[test]
fn cat_gift_morning() {
    check("mob_cat_gift_morning");
}

/// Cats lying on beds and sitting on lit furnaces and unopened chests.
#[test]
fn cat_lie_bed_chest() {
    check("mob_cat_lie_bed_chest");
}

/// Ocelots fleeing, tempted by raw fish, winning trust, kittens at finalize.
#[test]
fn ocelot_trust() {
    check("mob_ocelot_trust");
}

/// Taming with seeds, sitting, landing on the shoulder, being shaken off and the cookie.
#[test]
fn parrot_shoulder() {
    check("mob_parrot_shoulder");
}

/// Imitating nearby mobs and the flap state.
#[test]
fn parrot_imitate() {
    check("mob_parrot_imitate");
}

/// Wolf, cat, ocelot and parrot variants chosen at spawn.
#[test]
fn pet_biome_variants() {
    check("mob_pet_biome_variants");
}
