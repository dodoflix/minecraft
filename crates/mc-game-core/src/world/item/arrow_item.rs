//! `net.minecraft.world.item.{ArrowItem, SpectralArrowItem, TippedArrowItem}`: the dispenser side (`ProjectileItem.asProjectile`);
//! the shooter side (`createArrow`) is `projectile_weapon_item::create_arrow`.

use super::{ItemBehaviour, ItemStack, ProjectileItem};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::projectile::arrow::{AbstractArrow, Arrow, Pickup, SpectralArrow};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;

pub struct ArrowItem {
    spectral: bool,
}

/// `ArrowItem` as a `ProjectileItem` (what `TippedArrowItem` shares).
pub static ARROW_PROJECTILE: ArrowItem = ArrowItem { spectral: false };

impl ProjectileItem for ArrowItem {
    fn as_projectile(
        &self,
        level: &mut ServerLevel,
        position: Vec3,
        item_stack: &ItemStack,
        _direction: Direction,
    ) -> Box<dyn Entity> {
        let stack = item_stack.copy_with_count(1);
        if self.spectral {
            let mut arrow =
                SpectralArrow::at(level, position.x, position.y, position.z, stack, None);
            arrow.arrow_data_mut().pickup = Pickup::Allowed;
            Box::new(arrow)
        } else {
            let mut arrow = Arrow::at(level, position.x, position.y, position.z, stack, None);
            arrow.arrow_data_mut().pickup = Pickup::Allowed;
            Box::new(arrow)
        }
    }
}

impl ItemBehaviour for ArrowItem {
    fn as_projectile_item(&self) -> Option<&dyn ProjectileItem> {
        Some(self)
    }
}

register_item_class!("ArrowItem", |_| Box::new(ArrowItem { spectral: false }));
register_item_class!("SpectralArrowItem", |_| Box::new(ArrowItem {
    spectral: true
}));
