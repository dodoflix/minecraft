//! `ProjectileDispenseBehavior` (`DispenserBlock.registerProjectileBehavior`): one instance per item, built from the item's
//! `ProjectileItem` and its `DispenseConfig`. The entity is shot with `Projectile.shoot` (`spawnProjectileUsingShoot`),
//! not with `ProjectileItem.shoot`.
//!
//! Not implemented: the eggs and thrown potions that vanilla registers too, whose item classes are not `ProjectileItem`s
//! yet.

use crate::world::dispenser::{BlockSource, DispenseItemBehavior, DispenseTemplate, Templated};
use crate::world::entity::projectile::spawn::spawn_projectile_using_shoot;
use crate::world::item::{DispenseConfig, ItemStack, ProjectileItem, projectile_item};
use crate::world::level::LevelAccessor;
use mc_data::Item;

struct ProjectileDispense {
    item: &'static dyn ProjectileItem,
    config: DispenseConfig,
}

impl DispenseTemplate for ProjectileDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let direction = source.facing();
        let position = self
            .config
            .position_function
            .get_dispense_position(source.center(), direction);
        let projectile = self
            .item
            .as_projectile(source.level, position, &dispensed, direction);
        spawn_projectile_using_shoot(
            source.level,
            projectile,
            &dispensed,
            f64::from(direction.get_step_x()),
            f64::from(direction.get_step_y()),
            f64::from(direction.get_step_z()),
            self.config.power,
            self.config.uncertainty,
        );
        dispensed.shrink(1);
        dispensed
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        let pos = source.pos;
        source.level.level_event(
            None,
            self.config.override_dispense_event.unwrap_or(1002),
            pos,
            0,
        );
    }
}

/// `new ProjectileDispenseBehavior(item)`: throws `IllegalArgumentException` for an item that is no `ProjectileItem`.
fn create(name: &'static str) -> Box<dyn DispenseItemBehavior> {
    let item = Item::by_name(name).expect("registered projectile item exists");
    let projectile =
        projectile_item(item).unwrap_or_else(|| panic!("{name} not instance of ProjectileItem"));
    Box::new(Templated(ProjectileDispense {
        item: projectile,
        config: projectile.create_dispense_config(),
    }))
}

crate::register_dispense_behavior_per_item!(
    &[
        "minecraft:arrow",
        "minecraft:tipped_arrow",
        "minecraft:spectral_arrow",
        "minecraft:snowball",
        "minecraft:experience_bottle",
        "minecraft:firework_rocket",
        "minecraft:fire_charge",
        "minecraft:wind_charge",
    ],
    create
);
