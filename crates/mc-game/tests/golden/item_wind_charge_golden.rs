//! `WindChargeItem.use` against vanilla (`tests/oracle/scenarios/itm_wind_charge.txt`; fixture regenerated with
//! `scripts/block-oracle.sh itm_wind_charge`).

use crate::common;

#[test]
fn wind_charge_use() {
    common::block_scenario::check("itm_wind_charge");
}
