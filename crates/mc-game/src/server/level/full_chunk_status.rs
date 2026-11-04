//! `FullChunkStatus`: what a fully generated chunk may do (ordered).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum FullChunkStatus {
    Inaccessible,
    Full,
    BlockTicking,
    EntityTicking,
}

impl FullChunkStatus {
    pub fn is_or_after(self, step: FullChunkStatus) -> bool {
        self >= step
    }
}
