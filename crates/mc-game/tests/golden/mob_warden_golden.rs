//! The warden against the vanilla oracle (scenarios in `oracle/scenarios`): sniffing and roaring, anger from attacks and
//! vibrations, melee and sonic boom, digging and emerging, and the shrieker summon.
use crate::common::block_scenario;
use crate::common::entity_oracle::Options;

#[test]
fn warden_sniffs_investigates_and_roars() {
    block_scenario::check("mob_warden_sniff_roar");
}

#[test]
fn warden_anger_suspects_and_saved_list() {
    block_scenario::check("mob_warden_anger");
}

/// Players get armor and shield through the driver's `give`, which plays the equip sounds in vanilla and stays silent in the replay.
#[test]
fn warden_claws_and_sonic_booms() {
    block_scenario::check_with(
        "mob_warden_sonic_boom",
        &Options {
            ignore_containing: vec!["event=minecraft:item.armor.equip_"],
            ..Default::default()
        },
    );
}

#[test]
fn warden_digs_and_emerges() {
    block_scenario::check("mob_warden_dig");
}

#[test]
fn warden_listens_to_vibrations() {
    block_scenario::check("mob_warden_vibration");
}

#[test]
fn shrieker_summons_a_warden() {
    block_scenario::check("mob_shrieker_summon");
}
