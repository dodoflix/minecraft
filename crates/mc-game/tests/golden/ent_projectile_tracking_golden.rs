//! Projectile tracking in the block replay: the power packet of fireballs and wind charges, the per-type update intervals, the
//! owner id of the add-entity packet (owners loaded from a UUID), the rejected attack packet at an arrow and the pickup of a
//! trident whose owner is not in the level (`tests/oracle/scenarios/ent_{proj_tracking,proj_owner_load,arrow_attack_packet,
//! trident_hit_advancement,trident_offline_owner}.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

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
    arrow_attack_packet_disconnects => "ent_arrow_attack_packet",
    projectile_owner_id_from_saved_owner => "ent_proj_owner_load",
    projectile_tracking_packets => "ent_proj_tracking",
    trident_hit_awards_advancements => "ent_trident_hit_advancement",
    trident_with_offline_owner => "ent_trident_offline_owner",
);
