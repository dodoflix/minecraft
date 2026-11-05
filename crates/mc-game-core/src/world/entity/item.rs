//! `net.minecraft.world.entity.item.ItemEntity`.

pub mod primed_tnt;
use super::entity::{self, Entity, EntityData, MoveSimulationType, MovementEmission};
use super::factory::{ENTITY_FACTORIES, EntityRegistration};
use super::{EntityId, MoverType, movement};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::damagesource::DamageSource;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_data::GameEvent;
use crate::world::level::block::BlockStateExt;
use crate::world::item::{ItemStack, ItemStackExt, load_item_stack};
use linkme::distributed_slice;
use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::block::BlockState;
use mc_nbt::{CompoundTag, Tag};
use std::any::Any;
use uuid::Uuid;

pub const LIFETIME: i32 = 6000;
pub const INFINITE_PICKUP_DELAY: i32 = 32767;
pub const INFINITE_LIFETIME: i32 = -32768;
pub const DEFAULT_HEALTH: i32 = 5;
const FLOAT_HEIGHT: f32 = 0.1;

define_accessor!(pub DATA_ITEM: ItemStack = ItemEntity);

pub struct ItemEntity {
    base: EntityData,
    pub age: i32,
    pub pickup_delay: i32,
    pub health: i32,
    pub thrower: Option<Uuid>,
    pub target: Option<Uuid>,
    pub bob_offs: f32,
    /// `!getItem().canBeHurtBy(damageSources().inFire())`, refreshed at the start of every tick: `Entity.fireImmune()`
    /// has no level to resolve the damage type tags with.
    fire_immune_item: bool,
}

impl ItemEntity {
    /// `ItemEntity(type, level)`: RNG order = UUID (inside `EntityData::new`), `bobOffs`, then the random yaw.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM, ItemStack::empty());
        });
        let bob_offs = base.random.next_float() * std::f32::consts::PI * 2.0;
        let yaw = base.random.next_float() * 360.0;
        base.set_y_rot(yaw);
        Self { base, age: 0, pickup_delay: 0, health: DEFAULT_HEALTH, thrower: None, target: None, bob_offs, fire_immune_item: false }
    }

    /// `ItemEntity(level, x, y, z, itemStack)`: random horizontal motion.
    pub fn with_stack(level: &ServerLevel, x: f64, y: f64, z: f64, stack: ItemStack) -> Self {
        let mut e = Self::new(EntityType::ITEM, level.next_entity_id());
        e.set_pos(x, y, z);
        e.set_item(stack);
        let dx = e.base.random.next_double() * 0.2 - 0.1;
        let dz = e.base.random.next_double() * 0.2 - 0.1;
        e.base.set_delta_movement_xyz(dx, 0.2, dz);
        e
    }

    /// `ItemEntity(level, x, y, z, itemStack, dx, dy, dz)`.
    pub fn with_stack_and_motion(level: &ServerLevel, x: f64, y: f64, z: f64, stack: ItemStack, dx: f64, dy: f64, dz: f64) -> Self {
        let mut e = Self::new(EntityType::ITEM, level.next_entity_id());
        e.set_pos(x, y, z);
        e.set_item(stack);
        e.base.set_delta_movement_xyz(dx, dy, dz);
        e
    }

    pub fn get_item(&self) -> ItemStack {
        self.base.entity_data.get(DATA_ITEM)
    }

    /// `setItem(stack)`: vanilla stores the caller's object, which never equals the previous one, so this always
    /// marks the data dirty.
    pub fn set_item(&mut self, stack: ItemStack) {
        self.base.entity_data.set_force(DATA_ITEM, stack, true);
    }

    /// Vanilla mutates the stored stack object in place (`merge`'s `fromStack.shrink`, `Inventory.add`): the new
    /// value is stored without a dirty flag.
    pub fn set_item_in_place(&mut self, stack: ItemStack) {
        self.base.entity_data.set_untracked(DATA_ITEM, stack);
    }

    /// `getOwner()` (the `thrower` reference).
    pub fn get_owner(&self, level: &ServerLevel) -> Option<EntityId> {
        level.get_entity_by_uuid(self.thrower?).map(|e| e.id())
    }

    pub fn get_age(&self) -> i32 {
        self.age
    }

    pub fn set_target(&mut self, target: Option<Uuid>) {
        self.target = target;
    }
    pub fn set_thrower(&mut self, thrower: Uuid) {
        self.thrower = Some(thrower);
    }
    pub fn set_default_pick_up_delay(&mut self) {
        self.pickup_delay = 10;
    }
    pub fn set_no_pick_up_delay(&mut self) {
        self.pickup_delay = 0;
    }
    pub fn set_never_pick_up(&mut self) {
        self.pickup_delay = INFINITE_PICKUP_DELAY;
    }
    /// `makeFakeItem()`: a visual-only item (`/give` pickup animation).
    pub fn make_fake_item(&mut self) {
        self.set_never_pick_up();
        self.age = 5999;
    }
    pub fn set_pick_up_delay(&mut self, ticks: i32) {
        self.pickup_delay = ticks;
    }
    pub fn has_pick_up_delay(&self) -> bool {
        self.pickup_delay > 0
    }
    pub fn set_unlimited_lifetime(&mut self) {
        self.age = INFINITE_LIFETIME;
    }
    pub fn set_extended_lifetime(&mut self) {
        self.age = -LIFETIME;
    }

    /// `getSpin(ageInTicks, bobOffset)`.
    pub fn get_spin(age_in_ticks: f32, bob_offset: f32) -> f32 {
        age_in_ticks / 20.0 + bob_offset
    }

    fn is_mergable(&self) -> bool {
        let item = self.get_item();
        self.is_alive() && self.pickup_delay != INFINITE_PICKUP_DELAY && self.age != INFINITE_LIFETIME && self.age < LIFETIME && item.count() < item.get_max_stack_size()
    }

    fn set_fluid_movement(&mut self, multiplier: f64) {
        let m = self.base.delta_movement();
        self.base.set_delta_movement_xyz(m.x * multiplier, m.y + if m.y < f64::from(0.06f32) { f64::from(5.0E-4f32) } else { 0.0 }, m.z * multiplier);
    }

    /// `mergeWithNeighbours()`.
    pub fn merge_with_neighbours(&mut self, level: &mut ServerLevel) {
        if !self.is_mergable() {
            return;
        }
        let area = self.base.bounding_box().inflate(0.5, 0.0, 0.5);
        let ids = level.get_entities_of_class(&area, |e| e.as_any().is::<ItemEntity>(), |e| e.as_any().downcast_ref::<ItemEntity>().is_some_and(ItemEntity::is_mergable));
        for id in ids {
            level.with_entity(id, |_, other| {
                if let Some(other) = other.as_any_mut().downcast_mut::<ItemEntity>() {
                    if other.is_mergable() {
                        self.try_to_merge(other);
                    }
                }
            });
            if self.is_removed() {
                break;
            }
        }
    }

    fn try_to_merge(&mut self, other: &mut ItemEntity) {
        let this_stack = self.get_item();
        let other_stack = other.get_item();
        if self.target == other.target && are_mergable(&this_stack, &other_stack) {
            if other_stack.count() < this_stack.count() {
                merge_entities(self, this_stack, other, other_stack);
            } else {
                merge_entities(other, other_stack, self, this_stack);
            }
        }
    }
}

/// `ItemEntity.areMergable`.
pub fn are_mergable(this_stack: &ItemStack, other_stack: &ItemStack) -> bool {
    if other_stack.count() + this_stack.count() > other_stack.get_max_stack_size() {
        false
    } else {
        ItemStack::is_same_item_same_components(this_stack, other_stack)
    }
}

/// `ItemEntity.merge(toStack, fromStack, maxCount)`.
pub fn merge_stacks(to_stack: &ItemStack, from_stack: &mut ItemStack, max_count: i32) -> ItemStack {
    let delta = (to_stack.get_max_stack_size().min(max_count) - to_stack.count()).min(from_stack.count());
    let new_to = to_stack.copy_with_count(to_stack.count() + delta);
    from_stack.shrink(delta);
    new_to
}

/// `ItemEntity.merge(toItem, toStack, fromItem, fromStack)`.
fn merge_entities(to: &mut ItemEntity, to_stack: ItemStack, from: &mut ItemEntity, mut from_stack: ItemStack) {
    let new_to = merge_stacks(&to_stack, &mut from_stack, 64);
    to.set_item(new_to);
    to.pickup_delay = to.pickup_delay.max(from.pickup_delay);
    to.age = to.age.min(from.age);
    let emptied = from_stack.is_empty();
    from.set_item_in_place(from_stack);
    if emptied {
        from.discard();
    }
}

/// `Entity.spawnAtLocation(level, itemStack, offset)`: a default-pickup-delay item entity at the entity's position.
pub fn spawn_at_location(level: &mut ServerLevel, at: &dyn Entity, stack: ItemStack, offset: Vec3) -> Option<EntityId> {
    if stack.is_empty() {
        return None;
    }
    let p = at.base().position();
    let mut entity = ItemEntity::with_stack(level, p.x + offset.x, p.y + offset.y, p.z + offset.z, stack);
    entity.set_default_pick_up_delay();
    let id = entity.base.id;
    level.add_fresh_entity(Box::new(entity));
    Some(id)
}

impl Entity for ItemEntity {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }
    fn tracking_kind(&self) -> super::TrackingKind {
        super::TrackingKind::ItemEntity
    }
    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }
    fn get_default_gravity(&self) -> f64 {
        0.04
    }
    fn is_attackable(&self) -> bool {
        false
    }
    fn block_pos_below_offset(&self) -> f32 {
        0.999999
    }
    fn move_simulation_type(&self) -> MoveSimulationType {
        MoveSimulationType::ServerAndClient
    }

    /// `ignoreExplosion(explosion)`: items only react to explosions that affect blocklike entities.
    fn ignore_explosion(&self, explosion: &dyn crate::world::level::explosion::Explosion) -> bool {
        !explosion.should_affect_blocklike_entities()
    }

    /// `shouldPlayLavaHurtSound()`.
    fn should_play_lava_hurt_sound(&self) -> bool {
        self.health <= 0 || self.base.tick_count % 10 == 0
    }

    /// `fireImmune()`: `!getItem().canBeHurtBy(inFire) || super.fireImmune()`.
    fn fire_immune(&self) -> bool {
        self.fire_immune_item || self.base.entity_type.data().fire_immune
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        let item = self.get_item();
        if item.is_empty() {
            self.discard();
            return;
        }
        self.fire_immune_item = !item.can_be_hurt_by(level, &level.damage_sources().in_fire());
        self.base_tick(level);
        if self.pickup_delay > 0 && self.pickup_delay != INFINITE_PICKUP_DELAY {
            self.pickup_delay -= 1;
        }
        let pos = self.base.position();
        self.base.xo = pos.x;
        self.base.yo = pos.y;
        self.base.zo = pos.z;
        let old_movement = self.base.delta_movement();
        if self.is_in_water() && self.get_fluid_height("water") > f64::from(FLOAT_HEIGHT) {
            self.set_fluid_movement(f64::from(0.99f32));
        } else if self.is_in_lava() && self.get_fluid_height("lava") > f64::from(FLOAT_HEIGHT) {
            self.set_fluid_movement(f64::from(0.95f32));
        } else {
            self.apply_gravity();
        }
        let area = self.base.bounding_box().deflate_scalar(1.0E-7);
        self.base.no_physics = !crate::world::level::collision::CollisionGetter::no_collision(level, Some(self.base.id), Some(&movement::collision_context(self)), &area);
        if self.base.no_physics {
            let bb = self.base.bounding_box();
            move_towards_closest_space(self, level, pos.x, (bb.min_y + bb.max_y) / 2.0, pos.z);
        }
        let hdist = self.base.delta_movement().horizontal_distance_sqr();
        if self.base.on_ground() && !(hdist > f64::from(1.0E-5f32)) && (self.base.tick_count.wrapping_add(self.base.id)) % 4 != 0 {
            self.apply_effects_from_blocks_for_last_movements(level);
        } else {
            let dm = self.base.delta_movement();
            self.r#move(level, MoverType::SelfMover, dm);
            self.apply_effects_from_blocks(level);
            let air_drag = self.get_air_drag();
            let mut ground_friction = air_drag;
            if self.base.on_ground() {
                let below = movement::get_block_pos_below_that_affects_my_movement(self, level);
                ground_friction = air_drag * level.get_block_state(below).block().data().friction;
            }
            let m = self.base.delta_movement();
            self.base.set_delta_movement(m.multiply_xyz(f64::from(ground_friction), f64::from(air_drag), f64::from(ground_friction)));
            if self.base.on_ground() {
                let m = self.base.delta_movement();
                if m.y < 0.0 {
                    self.base.set_delta_movement(m.multiply_xyz(1.0, -0.5, 1.0));
                }
            }
        }
        let p = self.base.position();
        let moved = mth::floor_f64(self.base.xo) != mth::floor_f64(p.x) || mth::floor_f64(self.base.yo) != mth::floor_f64(p.y) || mth::floor_f64(self.base.zo) != mth::floor_f64(p.z);
        let rate = if moved { 2 } else { 40 };
        if self.base.tick_count % rate == 0 && self.is_mergable() {
            self.merge_with_neighbours(level);
        }
        if self.age != INFINITE_LIFETIME {
            self.age += 1;
        }
        let in_fluid = self.update_fluid_interaction(level);
        self.base.needs_sync |= in_fluid;
        let diff = self.base.delta_movement().subtract(old_movement).length_sqr();
        if diff > 0.01 {
            self.base.needs_sync = true;
        }
        if self.age >= LIFETIME {
            self.discard();
        }
    }

    /// `hurtServer(level, source, damage)`: items have `health` 5; `damage_resistant` items are immune.
    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if self.is_invulnerable_to_base(level, source) {
            return false;
        }
        if !level.mob_griefing() && source.causing_info().is_some_and(|c| is_mob(c.entity_type, c.is_living, c.is_player)) {
            return false;
        }
        let mut stack = self.get_item();
        if !stack.can_be_hurt_by(level, source) {
            return false;
        }
        self.base.sync_velocity = true;
        self.health = (self.health as f32 - damage) as i32;
        let position = self.base.position();
        level.game_event_from_entity(source.get_entity(), GameEvent::ENTITY_DAMAGE, position);
        if self.health <= 0 {
            crate::world::item::on_destroyed(&mut stack, level, position);
            self.set_item_in_place(stack);
            self.discard();
        }
        true
    }

    /// `playerTouch(player)`: pickup.
    fn player_touch(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        let mut stack = self.get_item();
        let item = stack.item();
        let org_count = stack.count();
        let infinite = player.player.abilities.instabuild;
        if self.pickup_delay == 0 && self.target.is_none_or(|t| t == player.uuid()) && player.player.inventory.add(&mut stack, infinite) {
            self.set_item_in_place(stack.clone());
            player.take(level, self, org_count);
            if stack.is_empty() {
                self.discard();
                stack.set_count(org_count);
                self.set_item_in_place(stack);
            }
            player.award_stat(level, crate::stats::Stat::PickedUp(item), org_count);
            player.on_item_pickup(level, self);
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        out.put_short("Health", self.health as i16);
        out.put_short("Age", self.age as i16);
        out.put_short("PickupDelay", self.pickup_delay as i16);
        if let Some(t) = self.thrower {
            out.put_int_array("Thrower", mc_core::uuid_util::uuid_to_int_array(&t).to_vec());
        }
        if let Some(t) = self.target {
            out.put_int_array("Owner", mc_core::uuid_util::uuid_to_int_array(&t).to_vec());
        }
        let item = self.get_item();
        if !item.is_empty() {
            let mut t = CompoundTag::new();
            item.save_into(&mut t);
            out.put("Item", Tag::Compound(t));
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.health = i32::from(input.get_short_or("Health", 5));
        self.age = i32::from(input.get_short_or("Age", 0));
        self.pickup_delay = i32::from(input.get_short_or("PickupDelay", 0));
        self.target = entity::read_uuid(input.get("Owner"));
        self.thrower = entity::read_uuid(input.get("Thrower"));
        let item = input.get_compound("Item").and_then(load_item_stack).unwrap_or_else(ItemStack::empty);
        self.set_item(item);
        if self.get_item().is_empty() {
            self.discard();
        }
    }
}

/// `instanceof Mob` from what a damage source remembers about its entity: living, not a player, not an armor stand.
fn is_mob(ty: EntityType, is_living: bool, is_player: bool) -> bool {
    is_living && !is_player && ty != EntityType::ARMOR_STAND && ty != EntityType::MANNEQUIN
}

/// `Entity.moveTowardsClosestSpace(x, y, z)`.
pub fn move_towards_closest_space(entity: &mut dyn Entity, level: &ServerLevel, x: f64, y: f64, z: f64) {
    
use crate::world::level::{BlockGetter, LevelAccessor};
    let pos = BlockPos::containing(x, y, z);
    let delta = Vec3::new(x - f64::from(pos.x()), y - f64::from(pos.y()), z - f64::from(pos.z()));
    let mut closest_direction = Direction::Up;
    let mut closest = f64::MAX;
    for direction in [Direction::North, Direction::South, Direction::West, Direction::East, Direction::Up] {
        let neighbor = pos.relative(direction);
        let state: BlockState = level.get_block_state(neighbor);
        if !state.is_collision_shape_full_block_at(level, neighbor) {
            let d = delta.get(direction.get_axis());
            let oriented = if direction.get_axis_direction() == mc_core::direction::AxisDirection::Positive { 1.0 - d } else { d };
            if oriented < closest {
                closest = oriented;
                closest_direction = direction;
            }
        }
    }
    let speed = entity.base_mut().random.next_float() * 0.2 + 0.1;
    let step = closest_direction.get_axis_direction().get_step() as f32;
    let scaled = entity.base().delta_movement().scale(0.75);
    let (s, _speed) = (f64::from(step * speed), ());
    match closest_direction.get_axis() {
        Axis::X => entity.base_mut().set_delta_movement_xyz(s, scaled.y, scaled.z),
        Axis::Y => entity.base_mut().set_delta_movement_xyz(scaled.x, s, scaled.z),
        Axis::Z => entity.base_mut().set_delta_movement_xyz(scaled.x, scaled.y, s),
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static ITEM_ENTITY_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ITEM,
    create: |ty, id, _registries| Box::new(ItemEntity::new(ty, id)),
};

#[cfg(test)]
mod tests {
    use super::*;
    use mc_data::Item;

    fn level() -> ServerLevel {
        ServerLevel::detached(mc_core::ident::Identifier::parse("minecraft:overworld").unwrap())
    }

    #[test]
    fn item_entity_nbt_round_trip_and_factory() {
        let level = level();
        let mut e = ItemEntity::with_stack(&level, 1.5, 70.0, -2.5, ItemStack::with_count(Item::DIAMOND, 5));
        e.age = 12;
        e.pickup_delay = 7;
        e.target = Some(Uuid::from_u128(9));
        let none = |_| None;
        let tag = entity::save(&e, &none).unwrap();
        assert_eq!(tag.get_string("id"), Some("minecraft:item"));
        let loaded = crate::world::entity::factory::load_entities_recursive(std::slice::from_ref(&tag), &level, mc_data::enums::EntitySpawnReason::Load);
        let back = loaded[0].as_any().downcast_ref::<ItemEntity>().expect("ItemEntity factory");
        assert_eq!((back.age, back.pickup_delay, back.target), (12, 7, Some(Uuid::from_u128(9))));
        assert_eq!(back.get_item().count(), 5);
        assert_eq!(entity::save(back, &none).unwrap(), tag);
    }

    #[test]
    fn merging_moves_items_into_the_larger_stack() {
        let level = level();
        let mut a = ItemEntity::with_stack(&level, 0.0, 70.0, 0.0, ItemStack::with_count(Item::DIAMOND, 10));
        let mut b = ItemEntity::with_stack(&level, 0.0, 70.0, 0.0, ItemStack::with_count(Item::DIAMOND, 3));
        a.age = 5;
        b.age = 2;
        a.try_to_merge(&mut b);
        assert_eq!(a.get_item().count(), 13);
        assert!(b.is_removed());
        assert_eq!(a.age, 2);
    }

    #[test]
    fn pickup_adds_to_inventory_and_discards() {
        let mut level = level();
        let mut player = ServerPlayer::new(1, mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(), mc_protocol::codec::GameProfile { id: Uuid::from_u128(1), name: "P".into(), properties: vec![] }, Default::default());
        let mut e = ItemEntity::with_stack(&level, 0.0, 70.0, 0.0, ItemStack::with_count(Item::DIAMOND, 4));
        e.player_touch(&mut level, &mut player);
        assert!(e.is_removed());
        assert_eq!(player.player.inventory.get_selected_item().count() + (1..9).map(|i| crate::world::container::Container::get_item(&player.player.inventory, i).count()).sum::<i32>(), 4);
    }
}
