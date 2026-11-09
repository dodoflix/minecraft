//! Guard against masked block subclasses: `behaviour(block)` binds a block to the first registered class of its Java class
//! chain, so a Java subclass without its own registration silently runs its ancestor's behaviour. Every such skipped class
//! that overrides a server-side `BlockBehaviour`/`Block` method (`tests/fixtures/block_class_overrides.txt`, from
//! `scripts/golden-block-behaviour.sh overrides`) must be listed in `MASKED` with the place that implements the overrides.
use mc_data::block::Block;
use mc_game::world::level::block::{BLOCK_CLASSES, BLOCKS};
use std::collections::{BTreeMap, BTreeSet};

/// Overrides with no server-side behaviour: the rendering and client-only methods, and the ones whose result is already part
/// of the extracted per-state data (shapes, light, skylight).
const DATA_OR_CLIENT: [&str; 14] = [
    "animateTick",
    "codec",
    "createBlockStateDefinition",
    "getCollisionShape",
    "getColor",
    "getDustColor",
    "getLightDampening",
    "getRenderShape",
    "getShadeBrightness",
    "getShape",
    "getVisualShape",
    "propagatesSkylightDown",
    "skipRendering",
    "spawnFallingLeavesParticle",
];

/// Unregistered block classes that override server-side methods, and where those are implemented.
const MASKED: [(&str, &str); 17] = [
    (
        "BaseEntityBlock",
        "trial spawner: no menu, and the trial spawner block entity has no block event",
    ),
    (
        "CartographyTableBlock",
        "menus: no `getMenuProvider`/`useWithoutItem` yet (menu hand-off)",
    ),
    (
        "CraftingTableBlock",
        "menus: no `getMenuProvider`/`useWithoutItem` yet (menu hand-off)",
    ),
    (
        "SmithingTableBlock",
        "menus: no `getMenuProvider`/`useWithoutItem` yet (menu hand-off)",
    ),
    (
        "StrawBedBlock",
        "`AbstractBedBlock` handles the straw bed (`is_straw_bed`)",
    ),
    (
        "WeatheringCopperBarsBlock",
        "`weathering_copper::random_tick` (called by `BlockState::random_tick`)",
    ),
    (
        "WeatheringCopperBulbBlock",
        "`weathering_copper::random_tick`",
    ),
    (
        "WeatheringCopperChainBlock",
        "`weathering_copper::random_tick`",
    ),
    (
        "WeatheringCopperChestBlock",
        "`weathering_copper::random_tick`",
    ),
    (
        "WeatheringCopperDoorBlock",
        "`weathering_copper::random_tick`",
    ),
    (
        "WeatheringCopperFullBlock",
        "`weathering_copper::random_tick`",
    ),
    (
        "WeatheringCopperGrateBlock",
        "`weathering_copper::random_tick`",
    ),
    (
        "WeatheringCopperSlabBlock",
        "`weathering_copper::random_tick`",
    ),
    (
        "WeatheringCopperStairBlock",
        "`weathering_copper::random_tick`",
    ),
    (
        "WeatheringCopperTrapDoorBlock",
        "`weathering_copper::random_tick`",
    ),
    ("WeatheringLanternBlock", "`weathering_copper::random_tick`"),
    (
        "WeatheringLightningRodBlock",
        "`weathering_copper::random_tick`",
    ),
];

#[test]
fn unregistered_block_subclasses_override_nothing_server_side() {
    let overrides: BTreeMap<&str, BTreeSet<&str>> =
        include_str!("../fixtures/block_class_overrides.txt")
            .lines()
            .map(|l| {
                let (class, methods) = l.split_once(' ').unwrap_or((l, ""));
                (
                    class,
                    methods.split(',').filter(|m| !m.is_empty()).collect(),
                )
            })
            .collect();
    let mut masked = BTreeSet::new();
    for block in Block::iter() {
        let data = block.data();
        if BLOCKS.iter().any(|r| r.block == data.name) {
            continue;
        }
        for class in data.classes {
            if BLOCK_CLASSES.iter().any(|r| r.class == *class)
                || matches!(*class, "Block" | "BlockBehaviour")
            {
                break;
            }
            let server_side = overrides
                .get(class)
                .is_some_and(|methods| methods.iter().any(|m| !DATA_OR_CLIENT.contains(m)));
            if server_side {
                masked.insert(*class);
            }
        }
    }
    let listed: BTreeSet<&str> = MASKED.iter().map(|(class, _)| *class).collect();
    assert_eq!(
        masked, listed,
        "classes that run an ancestor's behaviour while overriding server-side methods: register them, or list them in MASKED with where their overrides live"
    );
}

/// `TintedGlassBlock` overrides `getLightDampening` (15) and `propagatesSkylightDown` (false): both come from the extracted
/// per-state data, no registered class is needed.
#[test]
fn tinted_glass_blocks_light_and_skylight() {
    let state = Block::TINTED_GLASS.default_state();
    assert!(!state.propagates_skylight_down());
    assert_eq!(state.get_light_block(), 15);
}
