//! `net.minecraft.world.level.entity.Visibility`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Visibility {
    #[default]
    Hidden,
    Tracked,
    Ticking,
}

impl Visibility {
    /// `Visibility.fromFullChunkStatus(status)`.
    pub fn from_full_chunk_status(status: crate::server::level::FullChunkStatus) -> Visibility {
        use crate::server::level::FullChunkStatus as S;
        if status.is_or_after(S::EntityTicking) {
            Visibility::Ticking
        } else if status.is_or_after(S::Full) {
            Visibility::Tracked
        } else {
            Visibility::Hidden
        }
    }

    pub fn is_ticking(self) -> bool {
        self == Visibility::Ticking
    }

    pub fn is_accessible(self) -> bool {
        self != Visibility::Hidden
    }
}
