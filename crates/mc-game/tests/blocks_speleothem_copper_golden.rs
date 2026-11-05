//! Block-oracle goldens of the speleothem / amethyst / copper weathering slice (BxSpeleothemCopper).
mod common;

#[test]
fn copper_weathering() {
    common::block_scenario::check("blk_copper_weathering");
}

#[test]
fn dripstone_growth() {
    common::block_scenario::check("blk_dripstone_growth");
}

#[test]
fn sulfur_spike_growth() {
    common::block_scenario::check("blk_sulfur_spike_growth");
}

#[test]
fn amethyst_budding() {
    common::block_scenario::check("blk_amethyst_budding");
}
