//! `net.minecraft.world.level.pathfinder.PathType` (+ the vanilla default malus of every type).

macro_rules! path_types {
    ($($variant:ident = $name:literal, $malus:expr;)*) => {
        /// `PathType`: the classification of a block position for pathfinding; `get_malus()` is the default cost (negative
        /// = impassable) a mob uses unless it overrides it with `setPathfindingMalus`.
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum PathType {
            $($variant,)*
        }

        impl PathType {
            /// `values()` in ordinal order (also the iteration order of vanilla's `EnumSet<PathType>`).
            pub const ALL: [PathType; { [$(PathType::$variant),*].len() }] = [$(PathType::$variant),*];
            /// `values().length`.
            pub const COUNT: usize = Self::ALL.len();

            /// `getMalus()`.
            pub const fn get_malus(self) -> f32 {
                match self {
                    $(PathType::$variant => $malus,)*
                }
            }

            /// The Java constant name (`WALKABLE_DOOR`, ...).
            pub const fn name(self) -> &'static str {
                match self {
                    $(PathType::$variant => $name,)*
                }
            }
        }
    };
}

path_types! {
    Blocked = "BLOCKED", -1.0;
    Open = "OPEN", 0.0;
    Walkable = "WALKABLE", 0.0;
    WalkableDoor = "WALKABLE_DOOR", 0.0;
    Trapdoor = "TRAPDOOR", 0.0;
    PowderSnow = "POWDER_SNOW", -1.0;
    OnTopOfPowderSnow = "ON_TOP_OF_POWDER_SNOW", 0.0;
    Fence = "FENCE", -1.0;
    Lava = "LAVA", -1.0;
    Water = "WATER", 8.0;
    WaterBorder = "WATER_BORDER", 8.0;
    Rail = "RAIL", 0.0;
    UnpassableRail = "UNPASSABLE_RAIL", -1.0;
    FireInNeighbor = "FIRE_IN_NEIGHBOR", 8.0;
    Fire = "FIRE", 16.0;
    DamagingInNeighbor = "DAMAGING_IN_NEIGHBOR", 8.0;
    Damaging = "DAMAGING", -1.0;
    DoorOpen = "DOOR_OPEN", 0.0;
    DoorWoodClosed = "DOOR_WOOD_CLOSED", -1.0;
    DoorIronClosed = "DOOR_IRON_CLOSED", -1.0;
    Breach = "BREACH", 4.0;
    Leaves = "LEAVES", -1.0;
    StickyHoney = "STICKY_HONEY", 8.0;
    Cocoa = "COCOA", 0.0;
    DamageCautious = "DAMAGE_CAUTIOUS", 0.0;
    OnTopOfTrapdoor = "ON_TOP_OF_TRAPDOOR", 0.0;
    BigMobsCloseToDanger = "BIG_MOBS_CLOSE_TO_DANGER", 4.0;
}

impl PathType {
    /// `ordinal()` (= the vanilla id).
    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// `BY_ID` (`OutOfBoundsStrategy.ZERO`: unknown ids map to `BLOCKED`).
    pub fn by_id(id: i32) -> PathType {
        usize::try_from(id).ok().and_then(|i| Self::ALL.get(i).copied()).unwrap_or(PathType::Blocked)
    }
}

/// `PathType`-keyed `EnumMap<PathType, Float>` (`Mob.pathfindingMalus`).
#[derive(Clone, Debug, Default)]
pub struct PathfindingMalus([Option<f32>; PathType::COUNT]);

impl PathfindingMalus {
    /// `pathfindingMalus.get(type)`.
    pub fn get(&self, ty: PathType) -> Option<f32> {
        self.0[ty.ordinal()]
    }

    /// `Mob.getPathfindingMalus` without the vehicle redirect: the override or the type's default.
    pub fn get_or_default(&self, ty: PathType) -> f32 {
        self.0[ty.ordinal()].unwrap_or_else(|| ty.get_malus())
    }

    /// `pathfindingMalus.put(type, cost)`.
    pub fn set(&mut self, ty: PathType, cost: f32) {
        self.0[ty.ordinal()] = Some(cost);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_default_maluses() {
        assert_eq!(PathType::COUNT, 27);
        assert_eq!(PathType::BigMobsCloseToDanger.ordinal(), 26);
        assert_eq!(PathType::by_id(99), PathType::Blocked);
        assert_eq!(PathType::Fire.get_malus(), 16.0);
        assert_eq!(PathType::WaterBorder.get_malus(), 8.0);
        assert_eq!(PathType::Breach.get_malus(), 4.0);
        assert_eq!(PathType::DoorWoodClosed.get_malus(), -1.0);
        let mut m = PathfindingMalus::default();
        assert_eq!(m.get_or_default(PathType::Water), 8.0);
        m.set(PathType::Water, 0.0);
        assert_eq!(m.get_or_default(PathType::Water), 0.0);
    }
}
