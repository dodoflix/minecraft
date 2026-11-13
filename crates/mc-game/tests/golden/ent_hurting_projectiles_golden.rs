//! Fireballs, wither skulls, dragon fireballs and wind charges replayed against the real vanilla level
//! (`scripts/block-oracle.sh NAME`): the packets that pair and move them, what they hurt, the explosions, fire and clouds they
//! leave, how an attack or another charge deflects them and how the world border ends them.

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
    large_fireball_hits_and_explodes => "ent_fireball_hits",
    small_fireball_ignites_and_places_fire => "ent_small_fireball",
    wither_skull_damage_effect_and_explosion => "ent_wither_skull",
    dragon_fireball_leaves_breath_cloud => "ent_dragon_fireball",
    breeze_wind_charge_explodes_and_knocks_back => "ent_breeze_charge",
    wind_charge_hits_mobs_and_entities => "ent_wind_charge_mobs",
    hurting_projectiles_deflect_by_attack_and_charge => "ent_hurting_deflect",
    hurting_projectiles_end_at_world_border => "ent_hurting_world_border",
);
