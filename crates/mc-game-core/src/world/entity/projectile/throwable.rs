//! `ThrowableProjectile` + `ThrowableItemProjectile`: the shared tick and the synched `Item` stack of snowballs, eggs,
//! ender pearls, experience bottles and thrown potions.
//!
//! A concrete class embeds `base: EntityData` and `projectile: ProjectileData`, invokes `impl_throwable_entity!` inside
//! its `impl Entity` (accessors, `tick`, drag, portal use, the `Projectile` entity overrides) and
//! `impl_throwable_projectile!` inside `impl Projectile` (the data accessors).

use super::{HitType, Projectile, hit_target_or_deflect_self, update_rotation, util};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::entity::inside_block_effects::NoopApplier;
use crate::world::entity::{Entity, EntityData};
use crate::world::item::{ItemStack, load_item_stack};
use crate::world::level::BlockGetter;
use mc_data::block::Block;
use mc_nbt::CompoundTag;

define_accessor!(pub DATA_ITEM_STACK: ItemStack = ThrowableItemProjectile);

/// `Entity` members every throwable shares. `$gravity` is `getDefaultGravity()`. `impl_throwable_entity!(@shared ..)` is
/// the same without `tick`, for a class that overrides `tick()` (it calls [`tick`] for `super.tick()`).
#[macro_export]
macro_rules! impl_throwable_entity {
    ($gravity:expr $(, $arm:tt)?) => {
        $crate::impl_throwable_entity!(@shared $gravity $(, $arm)?);
        fn tick(&mut self, level: &mut $crate::server::level::ServerLevel) {
            $crate::world::entity::projectile::throwable::tick(self, level);
        }
    };
    (@shared $gravity:expr $(, $arm:tt)?) => {
        fn base(&self) -> &$crate::world::entity::EntityData {
            &self.base
        }
        fn base_mut(&mut self) -> &mut $crate::world::entity::EntityData {
            &mut self.base
        }
        fn as_any(&self) -> &dyn ::std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
            self
        }
        fn as_dyn(&self) -> &dyn $crate::world::entity::Entity {
            self
        }
        fn as_dyn_mut(&mut self) -> &mut dyn $crate::world::entity::Entity {
            self
        }
        fn get_default_gravity(&self) -> f64 {
            $gravity
        }
        fn get_air_drag(&self) -> f32 {
            0.99
        }
        fn can_use_portal(&self, _ignore_passenger: bool) -> bool {
            true
        }
        $crate::impl_projectile_entity!($($arm)?);
    };
}

/// `Projectile.projectile_data` accessors for a struct with a `projectile: ProjectileData` field.
#[macro_export]
macro_rules! impl_throwable_projectile {
    () => {
        fn projectile_data(&self) -> &$crate::world::entity::projectile::ProjectileData {
            &self.projectile
        }
        fn projectile_data_mut(
            &mut self,
        ) -> &mut $crate::world::entity::projectile::ProjectileData {
            &mut self.projectile
        }
    };
}

/// `ThrowableProjectile.tick()` (the vanilla `super.tick()` is [`super::tick`]).
pub fn tick(p: &mut dyn Projectile, level: &mut ServerLevel) {
    handle_first_tick_bubble_column(p, level);
    p.apply_gravity();
    apply_inertia(p);
    let result = {
        let this: &dyn Projectile = &*p;
        util::get_hit_result_on_move_vector(level, this.as_dyn(), &|e| {
            this.can_hit_entity(level, e)
        })
    };
    let new_position = if result.get_type() != HitType::Miss {
        result.get_location()
    } else {
        p.position().add(p.base().delta_movement())
    };
    p.set_pos_vec(new_position);
    update_rotation(p);
    p.apply_effects_from_blocks(level);
    super::tick(p, level);
    if result.get_type() != HitType::Miss && p.is_alive() {
        hit_target_or_deflect_self(p, level, &result);
    }
}

/// `applyInertia()`: the bubbles in water are client particles.
fn apply_inertia(p: &mut dyn Projectile) {
    let movement = p.base().delta_movement();
    let inertia = if p.is_in_water() {
        0.8f32
    } else {
        p.get_air_drag()
    };
    p.base_mut()
        .set_delta_movement(movement.scale(f64::from(inertia)));
}

/// `handleFirstTickBubbleColumn()`: bubble columns the projectile spawns in push it right away.
fn handle_first_tick_bubble_column(p: &mut dyn Projectile, level: &mut ServerLevel) {
    if !p.base().first_tick {
        return;
    }
    let bb = p.get_bounding_box();
    let from = mc_core::pos::BlockPos::containing(bb.min_x, bb.min_y, bb.min_z);
    let to = mc_core::pos::BlockPos::containing(bb.max_x, bb.max_y, bb.max_z);
    for x in from.x()..=to.x() {
        for y in from.y()..=to.y() {
            for z in from.z()..=to.z() {
                let pos = mc_core::pos::BlockPos::new(x, y, z);
                let state = level.get_block_state(pos);
                if state.block() == Block::BUBBLE_COLUMN {
                    crate::world::level::block::behaviour(state.block()).entity_inside(
                        state,
                        level,
                        pos,
                        p.as_dyn_mut(),
                        &mut NoopApplier,
                        true,
                    );
                }
            }
        }
    }
}

/// `ThrowableProjectile(type, x, y, z, level)` position + `ThrowableItemProjectile(type, owner, level, item)`:
/// the owner's `getX()`, `getEyeY() - 0.1F`, `getZ()`.
pub fn position_at_owner(p: &mut dyn Entity, owner: &dyn Entity) {
    p.set_pos(
        owner.base().x(),
        owner.base().get_eye_y() - f64::from(0.1f32),
        owner.base().z(),
    );
}

/// `ThrowableItemProjectile.setItem(source)`.
pub fn set_item(base: &mut EntityData, source: &ItemStack) {
    // a new stack object: vanilla's identity comparison always sees a change
    base.entity_data
        .set(DATA_ITEM_STACK, source.copy_with_count(1));
}

/// `ThrowableItemProjectile.getItem()`.
pub fn get_item(base: &EntityData) -> ItemStack {
    base.entity_data.get(DATA_ITEM_STACK)
}

/// `ThrowableItemProjectile.addAdditionalSaveData` (after `Projectile`'s).
pub fn add_additional_save_data(p: &dyn Projectile, out: &mut CompoundTag) {
    super::add_additional_save_data(p, out);
    if let Ok(tag) = get_item(p.base()).to_tag().into_result() {
        out.put("Item", tag);
    }
}

/// `ThrowableItemProjectile.readAdditionalSaveData`: `default_item` is `getDefaultItem()`.
pub fn read_additional_save_data(
    p: &mut dyn Projectile,
    input: &CompoundTag,
    default_item: mc_data::Item,
) {
    super::read_additional_save_data(p, input);
    let item = input
        .get_compound("Item")
        .and_then(load_item_stack)
        .unwrap_or_else(|| ItemStack::new(default_item));
    set_item(p.base_mut(), &item);
}
