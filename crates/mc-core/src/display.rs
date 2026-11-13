//! Small value types of display entities: `net.minecraft.util.Brightness`,
//! `net.minecraft.world.entity.Display.BillboardConstraints` and `net.minecraft.world.item.ItemDisplayContext`.

/// `Brightness(block, sky)`: a light override with both levels in `0..=15` (the codec enforces the range).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Brightness {
    pub block: i32,
    pub sky: i32,
}

impl Brightness {
    /// `FULL_BRIGHT`.
    pub const FULL_BRIGHT: Brightness = Brightness { block: 15, sky: 15 };

    /// `pack()`: `LightCoordsUtil.pack(block, sky)`.
    pub const fn pack(self) -> i32 {
        self.block << 4 | self.sky << 20
    }

    /// `unpack(packed)`.
    pub const fn unpack(packed: i32) -> Brightness {
        Brightness {
            block: packed >> 4 & 15,
            sky: packed >> 20 & 15,
        }
    }
}

/// `Display.BillboardConstraints` (ids are the declaration order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BillboardConstraints {
    Fixed,
    Vertical,
    Horizontal,
    Center,
}

impl BillboardConstraints {
    pub const ALL: [BillboardConstraints; 4] =
        [Self::Fixed, Self::Vertical, Self::Horizontal, Self::Center];

    /// `getSerializedName()`.
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
            Self::Center => "center",
        }
    }

    /// `getId()`.
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// `BY_ID` (`ByIdMap.continuous(.., OutOfBoundsStrategy.ZERO)`).
    pub fn by_id(id: i32) -> BillboardConstraints {
        usize::try_from(id)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
            .unwrap_or(Self::Fixed)
    }
}

/// `ItemDisplayContext` (ids are the declaration order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemDisplayContext {
    None,
    ThirdPersonLeftHand,
    ThirdPersonRightHand,
    FirstPersonLeftHand,
    FirstPersonRightHand,
    Head,
    Gui,
    Ground,
    Fixed,
    OnShelf,
}

impl ItemDisplayContext {
    pub const ALL: [ItemDisplayContext; 10] = [
        Self::None,
        Self::ThirdPersonLeftHand,
        Self::ThirdPersonRightHand,
        Self::FirstPersonLeftHand,
        Self::FirstPersonRightHand,
        Self::Head,
        Self::Gui,
        Self::Ground,
        Self::Fixed,
        Self::OnShelf,
    ];

    /// `getSerializedName()`.
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ThirdPersonLeftHand => "thirdperson_lefthand",
            Self::ThirdPersonRightHand => "thirdperson_righthand",
            Self::FirstPersonLeftHand => "firstperson_lefthand",
            Self::FirstPersonRightHand => "firstperson_righthand",
            Self::Head => "head",
            Self::Gui => "gui",
            Self::Ground => "ground",
            Self::Fixed => "fixed",
            Self::OnShelf => "on_shelf",
        }
    }

    /// `getId()`.
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// `BY_ID` (`ByIdMap.continuous(.., OutOfBoundsStrategy.ZERO)`).
    pub fn by_id(id: i32) -> ItemDisplayContext {
        usize::try_from(id)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
            .unwrap_or(Self::None)
    }
}
