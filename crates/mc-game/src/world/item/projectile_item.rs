//! `net.minecraft.world.item.ProjectileItem` and its `DispenseConfig`.
//!
//! An item class implements [`ProjectileItem`] next to `ItemBehaviour` and returns itself from
//! `ItemBehaviour::as_projectile_item` (`item instanceof ProjectileItem`). The dispenser's `ProjectileDispenseBehavior`
//! and `OminousItemSpawner` go through [`projectile_item`].

use super::{ItemStack, behaviour};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::projectile::Projectile;
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_data::Item;

/// `ProjectileItem.PositionFunction` (every vanilla function only reads the dispenser centre and its facing).
#[derive(Clone, Copy, Debug)]
pub enum PositionFunction {
    /// `DispenserBlock.getDispensePosition(source, scale, offset)`.
    Dispenser { scale: f64, offset: Vec3 },
    /// `FireworkRocketItem.getEntityJustOutsideOfBlockPos`.
    JustOutsideOfBlock,
}

impl PositionFunction {
    /// `getDispensePosition(source, direction)` with `center = source.center()`.
    pub fn get_dispense_position(&self, center: Vec3, direction: Direction) -> Vec3 {
        let (sx, sy, sz) = (f64::from(direction.get_step_x()), f64::from(direction.get_step_y()), f64::from(direction.get_step_z()));
        match *self {
            PositionFunction::Dispenser { scale, offset } => Vec3::new(center.x + (scale * sx + offset.x), center.y + (scale * sy + offset.y), center.z + (scale * sz + offset.z)),
            PositionFunction::JustOutsideOfBlock => center.add_xyz(sx * 0.5000099999997474, sy * 0.5000099999997474, sz * 0.5000099999997474),
        }
    }
}

/// `ProjectileItem.DispenseConfig`.
#[derive(Clone, Copy, Debug)]
pub struct DispenseConfig {
    pub position_function: PositionFunction,
    pub uncertainty: f32,
    pub power: f32,
    pub override_dispense_event: Option<i32>,
}

impl DispenseConfig {
    /// `DispenseConfig.DEFAULT` (`Builder` defaults).
    pub const DEFAULT: DispenseConfig = DispenseConfig {
        position_function: PositionFunction::Dispenser { scale: 0.7, offset: Vec3 { x: 0.0, y: 0.1, z: 0.0 } },
        uncertainty: 6.0,
        power: 1.1,
        override_dispense_event: None,
    };
}

/// `ProjectileItem`.
pub trait ProjectileItem: Send + Sync {
    /// `asProjectile(level, position, itemStack, direction)`.
    fn as_projectile(&self, level: &mut ServerLevel, position: Vec3, item_stack: &ItemStack, direction: Direction) -> Box<dyn Entity>;

    /// `createDispenseConfig()`.
    fn create_dispense_config(&self) -> DispenseConfig {
        DispenseConfig::DEFAULT
    }

    /// `shoot(projectile, xd, yd, zd, pow, uncertainty)`.
    fn shoot(&self, projectile: &mut dyn Projectile, xd: f64, yd: f64, zd: f64, pow: f32, uncertainty: f32) {
        projectile.shoot(xd, yd, zd, pow, uncertainty);
    }
}

/// `item instanceof ProjectileItem`.
pub fn projectile_item(item: Item) -> Option<&'static dyn ProjectileItem> {
    behaviour(item).as_projectile_item()
}
