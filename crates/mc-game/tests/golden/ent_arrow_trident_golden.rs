//! Arrows and tridents against vanilla: hits on mobs and players, shields, pickup, despawn, the world border, loyalty,
//! channeling, riptide and projectiles against minecarts and boats (`tests/oracle/scenarios/ent_{arrow,trident}_*.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`).

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
    arrow_deflected_by_an_enderman => "ent_arrow_enderman",
    arrow_expiry_and_falling => "ent_arrow_expire",
    arrow_in_fluids_and_fire => "ent_arrow_fluids",
    arrow_hits_on_mobs_and_players => "ent_arrow_mobs",
    arrow_knockback_and_boats => "ent_arrow_punch",
    arrow_pickup_rules => "ent_arrow_pickup",
    arrow_against_shields_and_armor => "ent_arrow_shield",
    arrow_stuck_in_blocks => "ent_arrow_stuck",
    arrow_and_teams => "ent_arrow_teams",
    arrow_against_other_entities => "ent_arrow_targets",
    projectiles_against_minecarts_and_boats => "ent_arrow_minecart",
    arrow_at_the_world_border => "ent_arrow_world_border",
    tipped_and_spectral_arrows => "ent_arrow_tipped",
    trident_channeling_in_thunder => "ent_trident_channeling",
    trident_hits => "ent_trident_hit",
    trident_loyalty_return => "ent_trident_loyalty",
    trident_owner_gone => "ent_trident_owner_gone",
    trident_riptide_hits => "ent_trident_riptide_hit",
);
