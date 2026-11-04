//! `ContainerInput` (the click type) and `ClickAction`.

/// `net.minecraft.world.inventory.ContainerInput` (wire id = discriminant; unknown ids decode as `Pickup`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContainerInput {
    Pickup = 0,
    QuickMove = 1,
    Swap = 2,
    Clone = 3,
    Throw = 4,
    QuickCraft = 5,
    PickupAll = 6,
}

impl ContainerInput {
    pub fn id(self) -> i32 {
        self as i32
    }

    /// `BY_ID` with `OutOfBoundsStrategy.ZERO`.
    pub fn by_id(id: i32) -> ContainerInput {
        match id {
            1 => Self::QuickMove,
            2 => Self::Swap,
            3 => Self::Clone,
            4 => Self::Throw,
            5 => Self::QuickCraft,
            6 => Self::PickupAll,
            _ => Self::Pickup,
        }
    }
}

/// `ClickAction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClickAction {
    Primary,
    Secondary,
}
