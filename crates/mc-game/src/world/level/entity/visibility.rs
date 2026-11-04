//! `net.minecraft.world.level.entity.Visibility`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Visibility {
    #[default]
    Hidden,
    Tracked,
    Ticking,
}

impl Visibility {
    pub fn is_ticking(self) -> bool {
        self == Visibility::Ticking
    }

    pub fn is_accessible(self) -> bool {
        self != Visibility::Hidden
    }
}
