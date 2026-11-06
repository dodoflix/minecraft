//! Item classes whose only override so far is the use animation/duration (bow, crossbow, trident, spyglass, brush,
//! bundle). The remaining behavior of these classes (`use`, `releaseUsing`, ...) is not ported yet.
use super::{APPROXIMATELY_INFINITE_USE_DURATION, ItemBehaviour, ItemStack};
use crate::register_item_class;
use mc_protocol::components::ItemUseAnimation;

struct FixedUse {
    duration: i32,
    animation: ItemUseAnimation,
}

impl ItemBehaviour for FixedUse {
    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        self.animation
    }
    fn get_use_duration(&self, _stack: &ItemStack) -> i32 {
        self.duration
    }
}

register_item_class!("BrushItem", |_| Box::new(FixedUse {
    duration: 200,
    animation: ItemUseAnimation::Brush
}));
