//! `Markings` of horses.

/// `Markings` (ids `0..=4`, `ByIdMap.continuous(.., WRAP)`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Markings {
    None,
    White,
    WhiteField,
    WhiteDots,
    BlackDots,
}

impl Markings {
    pub const ALL: [Markings; 5] = [Markings::None, Markings::White, Markings::WhiteField, Markings::WhiteDots, Markings::BlackDots];

    /// `getId()`.
    pub fn id(self) -> i32 {
        self as i32
    }

    /// `byId(id)`.
    pub fn by_id(id: i32) -> Markings {
        Self::ALL[id.rem_euclid(Self::ALL.len() as i32) as usize]
    }
}
