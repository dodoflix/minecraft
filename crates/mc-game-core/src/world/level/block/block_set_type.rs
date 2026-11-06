//! `BlockSetType` and `WoodType` (`world.level.block.state.properties`): the material-specific sounds and interaction
//! rules shared by doors, trapdoors, fence gates, buttons and pressure plates.

use mc_data::SoundEvent as S;
use mc_data::SoundEvent;
use mc_data::block::Block;

/// `BlockSetType.PressurePlateSensitivity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressurePlateSensitivity {
    Everything,
    Mobs,
}

#[derive(Debug)]
pub struct BlockSetType {
    pub name: &'static str,
    pub can_open_by_hand: bool,
    pub can_open_by_wind_charge: bool,
    pub can_button_be_activated_by_arrows: bool,
    pub pressure_plate_sensitivity: PressurePlateSensitivity,
    pub door_close: SoundEvent,
    pub door_open: SoundEvent,
    pub trapdoor_close: SoundEvent,
    pub trapdoor_open: SoundEvent,
    pub pressure_plate_click_off: SoundEvent,
    pub pressure_plate_click_on: SoundEvent,
    pub button_click_off: SoundEvent,
    pub button_click_on: SoundEvent,
}

macro_rules! set_type {
    ($name:literal, $hand:literal, $wind:literal, $arrows:literal, $sens:ident, [$dc:ident, $do_:ident, $tc:ident, $to:ident, $pp_off:ident, $pp_on:ident, $b_off:ident, $b_on:ident]) => {
        BlockSetType {
            name: $name,
            can_open_by_hand: $hand,
            can_open_by_wind_charge: $wind,
            can_button_be_activated_by_arrows: $arrows,
            pressure_plate_sensitivity: PressurePlateSensitivity::$sens,
            door_close: S::$dc,
            door_open: S::$do_,
            trapdoor_close: S::$tc,
            trapdoor_open: S::$to,
            pressure_plate_click_off: S::$pp_off,
            pressure_plate_click_on: S::$pp_on,
            button_click_off: S::$b_off,
            button_click_on: S::$b_on,
        }
    };
}

/// `new BlockSetType(name)`: the default (wooden) sounds.
const fn wood(name: &'static str) -> BlockSetType {
    BlockSetType {
        name,
        can_open_by_hand: true,
        can_open_by_wind_charge: true,
        can_button_be_activated_by_arrows: true,
        pressure_plate_sensitivity: PressurePlateSensitivity::Everything,
        door_close: S::BLOCK_WOODEN_DOOR_CLOSE,
        door_open: S::BLOCK_WOODEN_DOOR_OPEN,
        trapdoor_close: S::BLOCK_WOODEN_TRAPDOOR_CLOSE,
        trapdoor_open: S::BLOCK_WOODEN_TRAPDOOR_OPEN,
        pressure_plate_click_off: S::BLOCK_WOODEN_PRESSURE_PLATE_CLICK_OFF,
        pressure_plate_click_on: S::BLOCK_WOODEN_PRESSURE_PLATE_CLICK_ON,
        button_click_off: S::BLOCK_WOODEN_BUTTON_CLICK_OFF,
        button_click_on: S::BLOCK_WOODEN_BUTTON_CLICK_ON,
    }
}

pub static IRON: BlockSetType = set_type!(
    "iron",
    false,
    false,
    false,
    Everything,
    [
        BLOCK_IRON_DOOR_CLOSE,
        BLOCK_IRON_DOOR_OPEN,
        BLOCK_IRON_TRAPDOOR_CLOSE,
        BLOCK_IRON_TRAPDOOR_OPEN,
        BLOCK_METAL_PRESSURE_PLATE_CLICK_OFF,
        BLOCK_METAL_PRESSURE_PLATE_CLICK_ON,
        BLOCK_STONE_BUTTON_CLICK_OFF,
        BLOCK_STONE_BUTTON_CLICK_ON
    ]
);
pub static COPPER: BlockSetType = set_type!(
    "copper",
    true,
    true,
    false,
    Everything,
    [
        BLOCK_COPPER_DOOR_CLOSE,
        BLOCK_COPPER_DOOR_OPEN,
        BLOCK_COPPER_TRAPDOOR_CLOSE,
        BLOCK_COPPER_TRAPDOOR_OPEN,
        BLOCK_METAL_PRESSURE_PLATE_CLICK_OFF,
        BLOCK_METAL_PRESSURE_PLATE_CLICK_ON,
        BLOCK_STONE_BUTTON_CLICK_OFF,
        BLOCK_STONE_BUTTON_CLICK_ON
    ]
);
pub static GOLD: BlockSetType = set_type!(
    "gold",
    false,
    true,
    false,
    Everything,
    [
        BLOCK_IRON_DOOR_CLOSE,
        BLOCK_IRON_DOOR_OPEN,
        BLOCK_IRON_TRAPDOOR_CLOSE,
        BLOCK_IRON_TRAPDOOR_OPEN,
        BLOCK_METAL_PRESSURE_PLATE_CLICK_OFF,
        BLOCK_METAL_PRESSURE_PLATE_CLICK_ON,
        BLOCK_STONE_BUTTON_CLICK_OFF,
        BLOCK_STONE_BUTTON_CLICK_ON
    ]
);
pub static STONE: BlockSetType = set_type!(
    "stone",
    true,
    true,
    false,
    Mobs,
    [
        BLOCK_IRON_DOOR_CLOSE,
        BLOCK_IRON_DOOR_OPEN,
        BLOCK_IRON_TRAPDOOR_CLOSE,
        BLOCK_IRON_TRAPDOOR_OPEN,
        BLOCK_STONE_PRESSURE_PLATE_CLICK_OFF,
        BLOCK_STONE_PRESSURE_PLATE_CLICK_ON,
        BLOCK_STONE_BUTTON_CLICK_OFF,
        BLOCK_STONE_BUTTON_CLICK_ON
    ]
);
pub static POLISHED_BLACKSTONE: BlockSetType = set_type!(
    "polished_blackstone",
    true,
    true,
    false,
    Mobs,
    [
        BLOCK_IRON_DOOR_CLOSE,
        BLOCK_IRON_DOOR_OPEN,
        BLOCK_IRON_TRAPDOOR_CLOSE,
        BLOCK_IRON_TRAPDOOR_OPEN,
        BLOCK_STONE_PRESSURE_PLATE_CLICK_OFF,
        BLOCK_STONE_PRESSURE_PLATE_CLICK_ON,
        BLOCK_STONE_BUTTON_CLICK_OFF,
        BLOCK_STONE_BUTTON_CLICK_ON
    ]
);
pub static OAK: BlockSetType = wood("oak");
pub static SPRUCE: BlockSetType = wood("spruce");
pub static BIRCH: BlockSetType = wood("birch");
pub static ACACIA: BlockSetType = wood("acacia");
pub static CHERRY: BlockSetType = set_type!(
    "cherry",
    true,
    true,
    true,
    Everything,
    [
        BLOCK_CHERRY_WOOD_DOOR_CLOSE,
        BLOCK_CHERRY_WOOD_DOOR_OPEN,
        BLOCK_CHERRY_WOOD_TRAPDOOR_CLOSE,
        BLOCK_CHERRY_WOOD_TRAPDOOR_OPEN,
        BLOCK_CHERRY_WOOD_PRESSURE_PLATE_CLICK_OFF,
        BLOCK_CHERRY_WOOD_PRESSURE_PLATE_CLICK_ON,
        BLOCK_CHERRY_WOOD_BUTTON_CLICK_OFF,
        BLOCK_CHERRY_WOOD_BUTTON_CLICK_ON
    ]
);
pub static JUNGLE: BlockSetType = wood("jungle");
pub static DARK_OAK: BlockSetType = wood("dark_oak");
pub static PALE_OAK: BlockSetType = wood("pale_oak");
pub static POPLAR: BlockSetType = wood("poplar");
pub static CRIMSON: BlockSetType = set_type!(
    "crimson",
    true,
    true,
    true,
    Everything,
    [
        BLOCK_NETHER_WOOD_DOOR_CLOSE,
        BLOCK_NETHER_WOOD_DOOR_OPEN,
        BLOCK_NETHER_WOOD_TRAPDOOR_CLOSE,
        BLOCK_NETHER_WOOD_TRAPDOOR_OPEN,
        BLOCK_NETHER_WOOD_PRESSURE_PLATE_CLICK_OFF,
        BLOCK_NETHER_WOOD_PRESSURE_PLATE_CLICK_ON,
        BLOCK_NETHER_WOOD_BUTTON_CLICK_OFF,
        BLOCK_NETHER_WOOD_BUTTON_CLICK_ON
    ]
);
pub static WARPED: BlockSetType = set_type!(
    "warped",
    true,
    true,
    true,
    Everything,
    [
        BLOCK_NETHER_WOOD_DOOR_CLOSE,
        BLOCK_NETHER_WOOD_DOOR_OPEN,
        BLOCK_NETHER_WOOD_TRAPDOOR_CLOSE,
        BLOCK_NETHER_WOOD_TRAPDOOR_OPEN,
        BLOCK_NETHER_WOOD_PRESSURE_PLATE_CLICK_OFF,
        BLOCK_NETHER_WOOD_PRESSURE_PLATE_CLICK_ON,
        BLOCK_NETHER_WOOD_BUTTON_CLICK_OFF,
        BLOCK_NETHER_WOOD_BUTTON_CLICK_ON
    ]
);
pub static MANGROVE: BlockSetType = wood("mangrove");
pub static BAMBOO: BlockSetType = set_type!(
    "bamboo",
    true,
    true,
    true,
    Everything,
    [
        BLOCK_BAMBOO_WOOD_DOOR_CLOSE,
        BLOCK_BAMBOO_WOOD_DOOR_OPEN,
        BLOCK_BAMBOO_WOOD_TRAPDOOR_CLOSE,
        BLOCK_BAMBOO_WOOD_TRAPDOOR_OPEN,
        BLOCK_BAMBOO_WOOD_PRESSURE_PLATE_CLICK_OFF,
        BLOCK_BAMBOO_WOOD_PRESSURE_PLATE_CLICK_ON,
        BLOCK_BAMBOO_WOOD_BUTTON_CLICK_OFF,
        BLOCK_BAMBOO_WOOD_BUTTON_CLICK_ON
    ]
);

/// Every registered `BlockSetType` (`BlockSetType.values()`, registration order).
pub static ALL: [&BlockSetType; 18] = [
    &IRON,
    &COPPER,
    &GOLD,
    &STONE,
    &POLISHED_BLACKSTONE,
    &OAK,
    &SPRUCE,
    &BIRCH,
    &ACACIA,
    &CHERRY,
    &JUNGLE,
    &DARK_OAK,
    &PALE_OAK,
    &POPLAR,
    &CRIMSON,
    &WARPED,
    &MANGROVE,
    &BAMBOO,
];

impl BlockSetType {
    fn by_name(name: &str) -> Option<&'static BlockSetType> {
        ALL.iter().copied().find(|t| t.name == name)
    }

    /// The `BlockSetType` passed to the constructor of `block` in `Blocks.java` (doors, trapdoors, buttons, pressure
    /// plates, ...; derived from the registry name, which mirrors the constructor arguments one to one). `None` for
    /// blocks without a set type.
    pub fn for_block(block: Block) -> Option<&'static BlockSetType> {
        let full = block.data().name;
        let name = full.strip_prefix("minecraft:").unwrap_or(full);
        match name {
            "stone_button" | "polished_blackstone_button" | "stone_pressure_plate" => {
                return Some(&STONE);
            }
            "polished_blackstone_pressure_plate" => return Some(&POLISHED_BLACKSTONE),
            "light_weighted_pressure_plate" => return Some(&GOLD),
            "heavy_weighted_pressure_plate" | "iron_door" | "iron_trapdoor" => return Some(&IRON),
            _ => {}
        }
        if name.contains("copper") && (name.ends_with("door") || name.ends_with("trapdoor")) {
            return Some(&COPPER);
        }
        let stem = [
            "_trapdoor",
            "_door",
            "_pressure_plate",
            "_button",
            "_fence_gate",
        ]
        .iter()
        .find_map(|s| name.strip_suffix(s))?;
        Self::by_name(stem)
    }
}

/// `WoodType` as far as block behavior needs it: the set type plus the fence gate sounds.
#[derive(Debug, Clone, Copy)]
pub struct WoodType {
    pub set_type: &'static BlockSetType,
    pub fence_gate_close: SoundEvent,
    pub fence_gate_open: SoundEvent,
}

impl WoodType {
    /// The `WoodType` passed to the `FenceGateBlock` constructor of `block` (`oak_fence_gate` -> `WoodType.OAK`).
    pub fn for_fence_gate(block: Block) -> Option<WoodType> {
        let full = block.data().name;
        let stem = full
            .strip_prefix("minecraft:")
            .unwrap_or(full)
            .strip_suffix("_fence_gate")?;
        let set_type = BlockSetType::by_name(stem)?;
        let (fence_gate_close, fence_gate_open) = match stem {
            "cherry" => (
                S::BLOCK_CHERRY_WOOD_FENCE_GATE_CLOSE,
                S::BLOCK_CHERRY_WOOD_FENCE_GATE_OPEN,
            ),
            "crimson" | "warped" => (
                S::BLOCK_NETHER_WOOD_FENCE_GATE_CLOSE,
                S::BLOCK_NETHER_WOOD_FENCE_GATE_OPEN,
            ),
            "bamboo" => (
                S::BLOCK_BAMBOO_WOOD_FENCE_GATE_CLOSE,
                S::BLOCK_BAMBOO_WOOD_FENCE_GATE_OPEN,
            ),
            _ => (S::BLOCK_FENCE_GATE_CLOSE, S::BLOCK_FENCE_GATE_OPEN),
        };
        Some(WoodType {
            set_type,
            fence_gate_close,
            fence_gate_open,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every block of a class that takes a `BlockSetType`/`WoodType` constructor argument must map to one (the behavior
    /// constructors `expect` it); spot-check the sounds that differ per type.
    #[test]
    fn every_set_type_block_maps() {
        for block in Block::iter() {
            let classes = block.data().classes;
            let needs = [
                "DoorBlock",
                "TrapDoorBlock",
                "ButtonBlock",
                "PressurePlateBlock",
                "WeightedPressurePlateBlock",
            ]
            .iter()
            .any(|c| classes.contains(c));
            if needs {
                assert!(
                    BlockSetType::for_block(block).is_some(),
                    "{} has no BlockSetType",
                    block.data().name
                );
            }
            if classes.contains(&"FenceGateBlock") {
                assert!(
                    WoodType::for_fence_gate(block).is_some(),
                    "{} has no WoodType",
                    block.data().name
                );
            }
        }
        let by = |n: &str| BlockSetType::for_block(Block::by_name(n).unwrap()).unwrap();
        assert_eq!(by("minecraft:iron_door").name, "iron");
        assert_eq!(
            by("minecraft:waxed_weathered_copper_trapdoor").name,
            "copper"
        );
        assert_eq!(by("minecraft:polished_blackstone_button").name, "stone");
        assert_eq!(by("minecraft:light_weighted_pressure_plate").name, "gold");
        assert_eq!(
            by("minecraft:cherry_button").button_click_on,
            SoundEvent::BLOCK_CHERRY_WOOD_BUTTON_CLICK_ON
        );
        assert!(!by("minecraft:iron_door").can_open_by_hand);
    }
}
