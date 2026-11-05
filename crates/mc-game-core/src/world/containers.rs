//! `net.minecraft.world.Containers`: dropping container contents as item entities.

use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::entity::Entity;
use crate::world::entity::item::ItemEntity;
use crate::world::item::ItemStack;
use crate::world::level::LevelAccessor;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;

/// `dropContents(level, pos, container)`.
pub fn drop_contents_at(level: &mut ServerLevel, pos: BlockPos, container: &dyn Container) {
    drop_contents_xyz(level, f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()), container);
}

/// `dropContents(level, entity, container)`.
pub fn drop_contents_of_entity(level: &mut ServerLevel, entity: &dyn Entity, container: &dyn Container) {
    let p = entity.base().position();
    drop_contents_xyz(level, p.x, p.y, p.z, container);
}

fn drop_contents_xyz(level: &mut ServerLevel, x: f64, y: f64, z: f64, container: &dyn Container) {
    for i in 0..container.get_container_size() {
        drop_item_stack(level, x, y, z, container.get_item(i).clone());
    }
}

/// `dropContents(level, pos, list)`.
pub fn drop_contents_list(level: &mut ServerLevel, pos: BlockPos, list: &[ItemStack]) {
    for stack in list {
        drop_item_stack(level, f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()), stack.clone());
    }
}

/// `dropItemStack(level, x, y, z, itemStack)`: splits `stack` into entities of 10..=30 items each.
pub fn drop_item_stack(level: &mut ServerLevel, x: f64, y: f64, z: f64, mut stack: ItemStack) {
    let size = f64::from(EntityType::ITEM.data().width);
    let center_range = 1.0 - size;
    let half_size = size / 2.0;
    let random = level.random();
    let xo = x.floor() + random.next_double() * center_range + half_size;
    let yo = y.floor() + random.next_double() * center_range;
    let zo = z.floor() + random.next_double() * center_range + half_size;
    while !stack.is_empty() {
        let amount = random_amount(level);
        let mut entity = ItemEntity::with_stack(level, xo, yo, zo, stack.split(amount));
        let random = level.random();
        let (dx, dy, dz) = (random.triangle(0.0, 0.11485000171139836), random.triangle(0.2, 0.11485000171139836), random.triangle(0.0, 0.11485000171139836));
        entity.base_mut().set_delta_movement_xyz(dx, dy, dz);
        level.add_fresh_entity(Box::new(entity));
    }
}

fn random_amount(level: &mut ServerLevel) -> i32 {
    level.random().next_int_bounded(21) + 10
}
