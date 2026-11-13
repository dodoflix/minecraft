//! Small value types of display entities: `net.minecraft.util.Brightness` and
//! `net.minecraft.world.entity.Display.BillboardConstraints`.

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
