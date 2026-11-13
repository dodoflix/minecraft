//! Thrown eggs, experience bottles and splash/lingering potions against vanilla: the items, the dispenser entries, chick rolls and variants, the
//! splash effect scaling, dousing, water bottles and lingering clouds (`tests/oracle/scenarios/{itm,blk,ent}_*` of those names;
//! fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

macro_rules! golden {
    ($($test:ident => $scenario:literal),+ $(,)?) => {$(
        #[test]
        fn $test() {
            common::block_scenario::check($scenario);
        }
    )+};
}

golden!(
    dispensed_eggs => "blk_disp_egg",
    dispensed_potions => "blk_disp_potions",
    egg_chick_rolls_and_variants => "ent_egg_chicks",
    water_potion_douses_fire => "ent_potion_dousing",
    splash_potion_effect_scaling => "ent_potion_scale",
    thrown_eggs => "itm_throw_egg",
    thrown_experience_bottles => "itm_throw_xp_bottle",
    thrown_potions => "itm_throw_potion",
);
