//! `BrushItem`: only the use animation and duration. Not implemented: `useOn` and `releaseUsing` (brushing suspicious
//! sand and gravel).
use super::{ItemBehaviour, ItemStack};
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
