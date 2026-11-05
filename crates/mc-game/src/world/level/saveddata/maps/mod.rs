//! `net.minecraft.world.level.saveddata.maps`: map saved data, markers, decorations and the server-wide map storage.
mod map_decoration;
mod map_item_saved_data;
mod map_markers;
mod map_storage;

pub use map_decoration::{MapDecoration, MapDecorationType, banner_decoration, by_name as decoration_type, track_count};
pub use map_item_saved_data::{FrameInfo, HoldingPlayer, MAX_SCALE, MapItemSavedData, MapTickEnv, PlayerView, TRACKED_DECORATION_LIMIT};
pub use map_markers::{MapBanner, MapFrame, MapIndex};
pub use map_storage::MapStorage;
