//! `net.minecraft.util.datafix.DataFixers`: the global [`DataFixer`].

use crate::fixer::{DataFixer, DataFixerBuilder};
use std::sync::LazyLock;

/// `SharedConstants.getCurrentVersion().dataVersion().version()`. `mc_world::WORLD_VERSION` holds the same number but
/// mc-world will depend on this crate, so it is repeated here.
pub const CURRENT_VERSION: i32 = 5023;

/// `DataFixers.BLENDING_VERSION`.
pub const BLENDING_VERSION: i32 = 4997;

/// `DataFixers.getDataFixer()`: built on first use from [`crate::fixers::add_fixers`].
pub fn data_fixer() -> &'static DataFixer {
    static DATA_FIXER: LazyLock<DataFixer> = LazyLock::new(|| {
        let mut builder = DataFixerBuilder::new(CURRENT_VERSION);
        crate::fixers::add_fixers(&mut builder);
        builder.build()
    });
    &DATA_FIXER
}
