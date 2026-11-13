//! Rails, pistons, pressure plates and hoppers against real minecarts and boats (`tests/oracle/scenarios/blk_*cart*.txt`,
//! `blk_plates_vehicles.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;
use crate::common::block_scenario::check;

// `og` of a player that only sends packets is left out: vanilla keeps the flag the ride set, the replay derives it again on the first
// tick after the dismount (the position, pose and packets are compared)
fn ignore_on_ground() -> common::entity_oracle::Options {
    common::entity_oracle::Options {
        ignore_fields: vec!["og"],
        ..Default::default()
    }
}

#[test]
fn powered_rails_boost_brake_and_start_carts() {
    check("blk_rails_cart_boost");
}

#[test]
fn detector_rails_with_moving_standing_and_container_carts() {
    check("blk_rails_cart_detector");
}

#[test]
fn activator_rails_shake_eject_and_prime_carts() {
    common::block_scenario::check_with("blk_rails_cart_activator", &ignore_on_ground());
}

#[test]
fn pistons_push_and_carry_carts_and_boats() {
    check("blk_pistons_cart");
}

#[test]
fn pressure_plates_and_tripwires_react_to_vehicles() {
    check("blk_plates_vehicles");
}

#[test]
fn hoppers_exchange_items_with_container_carts() {
    check("blk_hopper_minecart_chain");
}

#[test]
fn lily_pads_slime_honey_and_soul_sand_act_on_vehicles() {
    check("blk_vehicle_block_effects");
}

#[test]
fn rail_shape_and_power_changes_under_carts() {
    check("blk_rails_cart_shapes");
}

mod improved_minecarts {
    use crate::common;
    use crate::common::block_scenario::check;

    #[test]
    fn powered_rails_boost_brake_and_start_carts() {
        check("blk_rails_cart_boost_new");
    }

    #[test]
    fn detector_rails_with_moving_standing_and_container_carts() {
        check("blk_rails_cart_detector_new");
    }

    #[test]
    fn activator_rails_shake_eject_and_prime_carts() {
        common::block_scenario::check_with(
            "blk_rails_cart_activator_new",
            &super::ignore_on_ground(),
        );
    }

    #[test]
    fn rail_shape_and_power_changes_under_carts() {
        check("blk_rails_cart_shapes_new");
    }

    #[test]
    fn pistons_push_and_carry_carts_and_boats() {
        check("blk_pistons_cart_new");
    }

    #[test]
    fn pressure_plates_and_tripwires_react_to_vehicles() {
        check("blk_plates_vehicles_new");
    }

    #[test]
    fn hoppers_exchange_items_with_container_carts() {
        check("blk_hopper_minecart_chain_new");
    }

    #[test]
    fn lily_pads_slime_honey_and_soul_sand_act_on_vehicles() {
        check("blk_vehicle_block_effects_new");
    }
}
