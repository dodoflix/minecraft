//! `net.minecraft.world.entity.vehicle.minecart.MinecartFurnace`: a cart that pushes itself along while it has fuel.

use super::abstract_minecart::{self, AbstractMinecart, MinecartData};
use crate::define_accessor;
use crate::minecart_entity_methods;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::mob;
use crate::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use crate::world::entity::vehicle::vehicle_entity::{self, VehicleEntity};
use crate::world::entity::{Entity, EntityData, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt as _};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::math::java::java_double_to_string;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::{EntityType, Item, props};
use mc_nbt::CompoundTag;

define_accessor!(pub DATA_ID_FUEL: Boolean = MinecartFurnace);

const FUEL_TICKS_PER_ITEM: i32 = 3600;
const MAX_FUEL_TICKS: i32 = 32000;

/// The scenario dump fields of a furnace cart: `fuel push`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static FURNACE_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 31,
    applies: |e| e.get_type() == EntityType::FURNACE_MINECART,
    fields: |e, _level, _ctx| {
        let m = e
            .as_any()
            .downcast_ref::<MinecartFurnace>()
            .expect("applies");
        vec![
            ("fuel".into(), m.fuel.to_string()),
            (
                "push".into(),
                format!(
                    "{}/{}",
                    java_double_to_string(m.push.x),
                    java_double_to_string(m.push.z)
                ),
            ),
        ]
    },
};

pub struct MinecartFurnace {
    base: EntityData,
    minecart: MinecartData,
    fuel: i32,
    push: Vec3,
}

impl MinecartFurnace {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let (base, minecart) = abstract_minecart::new_base(ty, id, 6, |b| {
            b.define(DATA_ID_FUEL, false);
        });
        Self {
            base,
            minecart,
            fuel: 0,
            push: Vec3::ZERO,
        }
    }

    fn has_fuel(&self) -> bool {
        self.base.entity_data.get(DATA_ID_FUEL)
    }

    fn set_has_fuel(&mut self, fuel: bool) {
        self.base.entity_data.set(DATA_ID_FUEL, fuel);
    }

    /// `calculateNewPushAlong(deltaMovement)`.
    fn calculate_new_push_along(&self, delta_movement: Vec3) -> Vec3 {
        if self.push.horizontal_distance_sqr() > 1.0E-4
            && delta_movement.horizontal_distance_sqr() > 0.001
        {
            self.push
                .projected_on(delta_movement)
                .normalize()
                .scale(self.push.length())
        } else {
            self.push
        }
    }

    /// `addFuel(interactingPos, itemStack)`.
    pub fn add_fuel(&mut self, interacting_pos: Vec3, item_stack: &ItemStack) -> bool {
        if mc_data::tags::get::<Item>("furnace_minecart_fuel")
            .is_some_and(|tag| tag.contains(item_stack.item()))
            && self.fuel + FUEL_TICKS_PER_ITEM <= MAX_FUEL_TICKS
        {
            self.fuel += FUEL_TICKS_PER_ITEM;
            if self.fuel > 0 {
                self.push = self.base.position().subtract(interacting_pos).horizontal();
            }
            true
        } else {
            false
        }
    }
}

impl AbstractMinecart for MinecartFurnace {
    fn minecart(&self) -> &MinecartData {
        &self.minecart
    }
    fn minecart_mut(&mut self) -> &mut MinecartData {
        &mut self.minecart
    }
    fn is_furnace(&self) -> bool {
        true
    }
    fn get_max_speed(&self, level: &ServerLevel) -> f64 {
        let speed = abstract_minecart::behavior_max_speed(self, level);
        if self.is_in_water() {
            speed * 0.75
        } else {
            speed * 0.5
        }
    }
    fn get_default_display_block_state(&self) -> BlockState {
        Block::FURNACE
            .default_state()
            .set(props::HORIZONTAL_FACING, Direction::North)
            .set(props::LIT, self.has_fuel())
    }
    fn apply_natural_slowdown(&mut self, movement: Vec3) -> Vec3 {
        let new_movement = if self.push.length_sqr() > 1.0E-7 {
            self.push = self.calculate_new_push_along(movement);
            let moved = movement.multiply_xyz(0.8, 0.0, 0.8).add(self.push);
            if self.is_in_water() {
                moved.scale(0.1)
            } else {
                moved
            }
        } else {
            movement.multiply_xyz(0.98, 0.0, 0.98)
        };
        abstract_minecart::apply_natural_slowdown(self, new_movement)
    }
}

impl VehicleEntity for MinecartFurnace {
    fn get_drop_item(&self) -> Item {
        Item::FURNACE_MINECART
    }
}

impl Entity for MinecartFurnace {
    minecart_entity_methods!();

    fn tick(&mut self, level: &mut ServerLevel) {
        abstract_minecart::tick(self, level);
        if self.fuel > 0 {
            self.fuel -= 1;
        }
        if self.fuel <= 0 {
            self.push = Vec3::ZERO;
        }
        let has_fuel = self.fuel > 0;
        self.set_has_fuel(has_fuel);
        // the smoke particle is client side; the server still draws the random of its roll
        if self.has_fuel() {
            self.base.random.next_int_bounded(4);
        }
    }

    fn interact(
        &mut self,
        _level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _location: Vec3,
    ) -> InteractionResult {
        let stack = player.get_item_in_hand(hand);
        if self.add_fuel(player.position(), &stack) {
            mob::interact::with_hand_stack(player, hand, |s, p| {
                s.consume(1, p.has_infinite_materials());
            });
        }
        InteractionResult::SUCCESS
    }

    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::FURNACE_MINECART))
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        vehicle_entity::hurt_server(self, level, source, amount)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_minecart::add_additional_save_data(self, out);
        out.put_double("PushX", self.push.x);
        out.put_double("PushZ", self.push.z);
        out.put_short("Fuel", self.fuel as i16);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_minecart::read_additional_save_data(self, input);
        let x_push = input.get_double_or("PushX", 0.0);
        let z_push = input.get_double_or("PushZ", 0.0);
        self.push = Vec3::new(x_push, 0.0, z_push);
        self.fuel = i32::from(input.get_short_or("Fuel", 0));
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static MINECART_FURNACE_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::FURNACE_MINECART,
    create: |ty, id, _registries| Box::new(MinecartFurnace::new(ty, id)),
};
