//! `net.minecraft.world.item.ProjectileWeaponItem` (the shared bow/crossbow logic) and `ArrowItem`'s projectile creation.
//!
//! A weapon class implements [`ProjectileWeapon`] (the abstract/overridable methods) and calls [`shoot`], [`draw`] and
//! [`get_projectile`]. The shooter is the entity that is currently running (a player's release packet, a mob's tick):
//! it is taken out of the level and handed in as `&mut dyn Entity`; the weapon stack is the caller's `&mut ItemStack`
//! (written back to its slot by the caller).

use super::{ItemStack, ItemStackExt as _, InteractionHand, hurt_and_break_stack};
use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::player::ServerPlayer;
use crate::world::entity::projectile::arrow::{AbstractArrow, Arrow, SpectralArrow};
use crate::world::entity::projectile::spawn::spawn_projectile_plain;
use crate::world::entity::projectile::Projectile;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::enchantment_helper as helper;
use mc_data::Item;
use mc_protocol::components::INTANGIBLE_PROJECTILE;

/// `ProjectileWeaponItem.ARROW_ONLY`: `itemStack.is(ItemTags.ARROWS)`.
pub fn is_arrow(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("arrows").is_some_and(|t| t.contains(stack.item()))
}

/// `ProjectileWeaponItem.ARROW_OR_FIREWORK`.
pub fn is_arrow_or_firework(stack: &ItemStack) -> bool {
    is_arrow(stack) || stack.item() == Item::FIREWORK_ROCKET
}

/// The overridable part of `ProjectileWeaponItem`.
pub trait ProjectileWeapon {
    /// `getSupportedHeldProjectiles()`.
    fn supported_held_projectiles(&self, stack: &ItemStack) -> bool;

    /// `getAllSupportedProjectiles()`.
    fn all_supported_projectiles(&self, stack: &ItemStack) -> bool {
        is_arrow(stack)
    }

    /// `getDurabilityUse(projectile)`.
    fn get_durability_use(&self, _projectile: &ItemStack) -> i32 {
        1
    }

    /// `shootProjectile(shooter, projectile, index, power, uncertainty, angle, targetOverride)`.
    #[allow(clippy::too_many_arguments)]
    fn shoot_projectile(&self, level: &mut ServerLevel, shooter: &mut dyn Entity, projectile: &mut dyn Projectile, index: i32, power: f32, uncertainty: f32, angle: f32, target_override: Option<EntityId>);

    /// `createProjectile(level, shooter, weapon, projectile, isCrit)`.
    fn create_projectile(&self, level: &mut ServerLevel, shooter: &dyn Entity, weapon: &ItemStack, projectile: &ItemStack, is_crit: bool) -> Box<dyn Entity> {
        create_arrow_projectile(level, shooter, weapon, projectile, is_crit)
    }
}

/// `ArrowItem` / `SpectralArrowItem` `.createArrow(level, itemStack, owner, firedFromWeapon)`: `item` is an arrow item.
pub fn create_arrow(level: &mut ServerLevel, item: Item, stack: &ItemStack, owner: &dyn Entity, fired_from_weapon: Option<&ItemStack>) -> Box<dyn AbstractArrowBox> {
    let single = stack.copy_with_count(1);
    if item_is_class(item, "SpectralArrowItem") {
        Box::new(SpectralArrow::from_owner(level, owner, single, fired_from_weapon))
    } else {
        Box::new(Arrow::from_owner(level, owner, single, fired_from_weapon))
    }
}

/// An `AbstractArrow` that is also an entity (what `ArrowItem.createArrow` returns).
pub trait AbstractArrowBox: AbstractArrow {
    fn into_entity(self: Box<Self>) -> Box<dyn Entity>;
}

impl<T: AbstractArrow + 'static> AbstractArrowBox for T {
    fn into_entity(self: Box<Self>) -> Box<dyn Entity> {
        self
    }
}

/// `item instanceof <class>` through the Java class chain of the extracted item data.
fn item_is_class(item: Item, class: &str) -> bool {
    item.data().classes.contains(&class)
}

/// `item instanceof ArrowItem`.
pub fn is_arrow_item(item: Item) -> bool {
    item_is_class(item, "ArrowItem")
}

/// The arrow item that fires `projectile`: itself when it is an `ArrowItem`, else `Items.ARROW`.
pub fn arrow_item_for(projectile: &ItemStack) -> Item {
    if is_arrow_item(projectile.item()) { projectile.item() } else { Item::ARROW }
}

/// `ProjectileWeaponItem.createProjectile` default: an arrow, crit-flagged when `is_crit`.
pub fn create_arrow_projectile(level: &mut ServerLevel, shooter: &dyn Entity, weapon: &ItemStack, projectile: &ItemStack, is_crit: bool) -> Box<dyn Entity> {
    let mut arrow = create_arrow(level, arrow_item_for(projectile), projectile, shooter, Some(weapon));
    if is_crit {
        arrow.set_crit_arrow(true);
    }
    arrow.into_entity()
}

/// `ProjectileUtil.getMobArrow(mob, projectile, power, firedFromWeapon)`.
pub fn get_mob_arrow(level: &mut ServerLevel, mob: &dyn Entity, projectile: &ItemStack, power: f32, fired_from_weapon: Option<&ItemStack>) -> Box<dyn Entity> {
    let mut arrow = create_arrow(level, arrow_item_for(projectile), projectile, mob, fired_from_weapon);
    arrow.set_base_damage_from_mob(level, power);
    arrow.into_entity()
}

/// `ProjectileUtil.getWeaponHoldingHand(mob, weaponItem)`.
pub fn get_weapon_holding_hand(mob: &dyn Entity, weapon_item: Item) -> InteractionHand {
    if mob.get_item_by_slot(EquipmentSlot::MainHand).item() == weapon_item { InteractionHand::MainHand } else { InteractionHand::OffHand }
}

/// Where a projectile found by [`get_projectile`] lives.
pub enum AmmoSlot {
    /// A slot of the player's inventory.
    Inventory(usize),
    /// An equipment slot of a mob.
    Equipment(EquipmentSlot),
}

/// A projectile stack found for a weapon (`LivingEntity.getProjectile`).
pub struct ProjectileSource {
    /// A copy of the stack (`ItemStack.EMPTY` when there is none).
    pub stack: ItemStack,
    /// Where it came from; `None` for the virtual arrow (infinite materials / a mob's default arrow).
    pub slot: Option<AmmoSlot>,
}

impl ProjectileSource {
    /// Writes the (drawn-from) copy back to its slot (vanilla mutates the live stack in place).
    pub fn write_back(self, holder: &mut dyn Entity) {
        match self.slot {
            Some(AmmoSlot::Inventory(slot)) => {
                if let Some(player) = holder.as_any_mut().downcast_mut::<ServerPlayer>() {
                    player.player.inventory.set_item(slot, self.stack);
                }
            }
            Some(AmmoSlot::Equipment(slot)) => {
                if let Some(stack) = holder.item_by_slot_mut(slot) {
                    *stack = self.stack;
                }
            }
            None => {}
        }
    }
}

/// `LivingEntity.getProjectile(heldWeapon)`: `Player.getProjectile` for players, `Mob.getProjectile` for mobs.
pub fn get_projectile(holder: &dyn Entity, weapon: &dyn ProjectileWeapon) -> ProjectileSource {
    if let Some(player) = holder.as_any().downcast_ref::<ServerPlayer>() {
        return get_player_projectile(player, weapon);
    }
    // `Mob.getProjectile`: `ProjectileWeaponItem.getHeldProjectile`, else a plain arrow
    for slot in [EquipmentSlot::OffHand, EquipmentSlot::MainHand] {
        let held = holder.get_item_by_slot(slot);
        if weapon.supported_held_projectiles(&held) {
            return ProjectileSource { stack: held, slot: Some(AmmoSlot::Equipment(slot)) };
        }
    }
    ProjectileSource { stack: ItemStack::new(Item::ARROW), slot: None }
}

/// `Player.getProjectile(heldWeapon)`.
fn get_player_projectile(player: &ServerPlayer, weapon: &dyn ProjectileWeapon) -> ProjectileSource {
    let inv = &player.player.inventory;
    let offhand_slot = (36..inv.get_container_size()).find(|&i| crate::world::entity::player::inventory::equipment_slot_for(i) == Some(EquipmentSlot::OffHand));
    // `ProjectileWeaponItem.getHeldProjectile(this, supportedHeldProjectiles)`: off hand first
    if let Some(slot) = offhand_slot
        && weapon.supported_held_projectiles(inv.get_item(slot))
    {
        return ProjectileSource { stack: inv.get_item(slot).clone(), slot: Some(AmmoSlot::Inventory(slot)) };
    }
    let main = inv.get_selected_slot() as usize;
    if weapon.supported_held_projectiles(inv.get_item(main)) {
        return ProjectileSource { stack: inv.get_item(main).clone(), slot: Some(AmmoSlot::Inventory(main)) };
    }
    for i in 0..inv.get_container_size() {
        if weapon.all_supported_projectiles(inv.get_item(i)) {
            return ProjectileSource { stack: inv.get_item(i).clone(), slot: Some(AmmoSlot::Inventory(i)) };
        }
    }
    let stack = if player.has_infinite_materials() { ItemStack::new(Item::ARROW) } else { ItemStack::empty() };
    ProjectileSource { stack, slot: None }
}

/// `ProjectileWeaponItem.useAmmo(weapon, projectile, holder, forceInfinite)`; `projectile` is the (copy of the)
/// stack being drawn from.
fn use_ammo(level: &mut ServerLevel, weapon: &ItemStack, projectile: &mut ItemStack, holder: &dyn Entity, force_infinite: bool) -> ItemStack {
    let infinite = holder.player().is_some_and(|p| p.abilities.instabuild);
    let ammo_to_use = if !force_infinite && !infinite { helper::process_ammo_use(level, weapon, projectile, 1) } else { 0 };
    if ammo_to_use > projectile.count() {
        ItemStack::empty()
    } else if ammo_to_use == 0 {
        let mut copy = projectile.copy_with_count(1);
        copy.set(INTANGIBLE_PROJECTILE, ());
        copy
    } else {
        // (`inventory.removeItem(projectile)` of an emptied stack: the caller writes the empty stack back)
        projectile.split(ammo_to_use)
    }
}

/// `ProjectileWeaponItem.draw(weapon, projectile, shooter)`: `projectile` is a copy of the ammo stack that the caller
/// writes back (see [`ProjectileSource::write_back`]).
pub fn draw(level: &mut ServerLevel, weapon: &ItemStack, projectile: &mut ItemStack, shooter: &mut dyn Entity) -> Vec<ItemStack> {
    if projectile.is_empty() {
        return Vec::new();
    }
    let id = shooter.id();
    let num_projectiles = helper::process_projectile_count(level, &mut Actors::of(&mut *shooter), weapon, id, 1);
    let mut drawn = Vec::with_capacity(num_projectiles.max(0) as usize);
    let mut projectile_copy = projectile.copy();
    for i in 0..num_projectiles {
        let drawn_stack = use_ammo(level, weapon, if i == 0 { &mut *projectile } else { &mut projectile_copy }, &*shooter, i > 0);
        if !drawn_stack.is_empty() {
            drawn.push(drawn_stack);
        }
    }
    drawn
}

/// `ProjectileWeaponItem.shoot(level, shooter, hand, weapon, projectiles, power, uncertainty, isCrit, targetOverride)`.
#[allow(clippy::too_many_arguments)]
pub fn shoot(
    weapon_impl: &dyn ProjectileWeapon,
    level: &mut ServerLevel,
    shooter: &mut dyn Entity,
    hand: InteractionHand,
    weapon: &mut ItemStack,
    projectiles: &[ItemStack],
    power: f32,
    uncertainty: f32,
    is_crit: bool,
    target_override: Option<EntityId>,
) {
    let shooter_id = shooter.id();
    let max_angle = helper::process_projectile_spread(level, &mut Actors::of(&mut *shooter), weapon, shooter_id, 0.0);
    let n = projectiles.len() as i32;
    let angle_step = if n == 1 { 0.0 } else { 2.0 * max_angle / (n - 1) as f32 };
    let angle_offset = ((n - 1) % 2) as f32 * angle_step / 2.0;
    let mut direction = 1.0f32;
    for (i, projectile) in projectiles.iter().enumerate() {
        if projectile.is_empty() {
            continue;
        }
        let angle = angle_offset + direction * ((i as i32 + 1) / 2) as f32 * angle_step;
        direction = -direction;
        let mut entity = weapon_impl.create_projectile(level, &*shooter, weapon, projectile, is_crit);
        if let Some(p) = entity.as_projectile_mut() {
            weapon_impl.shoot_projectile(level, &mut *shooter, p, i as i32, power, uncertainty, angle, target_override);
        }
        spawn_projectile_plain(level, entity, projectile);
        // `weapon.hurtAndBreak(getDurabilityUse(projectile), shooter, hand.asEquipmentSlot())`
        let infinite = shooter.player().is_some_and(|p| p.abilities.instabuild);
        if let Some(broken) = hurt_and_break_stack(weapon, weapon_impl.get_durability_use(projectile), level, infinite) {
            shooter.on_equipped_item_broken(level, &broken, hand.as_equipment_slot());
        }
        if weapon.is_empty() {
            break;
        }
    }
}
