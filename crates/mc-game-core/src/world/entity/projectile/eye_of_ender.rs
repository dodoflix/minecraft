//! `net.minecraft.world.entity.projectile.EyeOfEnder`: the eye thrown towards the nearest stronghold. It flies to a
//! target at most 12 blocks away horizontally (8 blocks up when the structure is farther), lives 80 ticks and then drops
//! as an item or breaks.

use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::item::ItemEntity;
use crate::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use crate::world::entity::{Entity, EntityData, EntityId, RemovalReason, entity};
use crate::world::item::{ItemStack, load_item_stack};
use crate::world::level::LevelAccessor;
use linkme::distributed_slice;
use mc_core::math::java::java_double_to_string;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use std::any::Any;

define_accessor!(pub DATA_ITEM_STACK: ItemStack = EyeOfEnder);

/// `TOO_FAR_DISTANCE`.
const TOO_FAR_DISTANCE: f64 = 12.0;
/// `TOO_FAR_SIGNAL_HEIGHT`.
const TOO_FAR_SIGNAL_HEIGHT: f64 = 8.0;

/// `EyeOfEnder`.
pub struct EyeOfEnder {
    base: EntityData,
    target: Option<Vec3>,
    life: i32,
    survive_after_death: bool,
}

impl EyeOfEnder {
    /// `EyeOfEnder(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ITEM_STACK, ItemStack::new(Item::ENDER_EYE));
        });
        Self {
            base,
            target: None,
            life: 0,
            survive_after_death: false,
        }
    }

    /// `EyeOfEnder(level, x, y, z)`.
    pub fn at(level: &ServerLevel, x: f64, y: f64, z: f64) -> Self {
        let mut eye = Self::new(EntityType::EYE_OF_ENDER, level.next_entity_id());
        eye.set_pos(x, y, z);
        eye
    }

    /// `setItem(source)`.
    pub fn set_item(&mut self, source: &ItemStack) {
        let item = if source.is_empty() {
            ItemStack::new(Item::ENDER_EYE)
        } else {
            source.copy_with_count(1)
        };
        self.base.entity_data.set(DATA_ITEM_STACK, item);
    }

    /// `getItem()`.
    pub fn get_item(&self) -> ItemStack {
        self.base.entity_data.get(DATA_ITEM_STACK)
    }

    /// `signalTo(target)`.
    pub fn signal_to(&mut self, target: Vec3) {
        let delta = target.subtract(self.position());
        let horizontal_distance = delta.horizontal_distance();
        self.target = Some(if horizontal_distance > TOO_FAR_DISTANCE {
            self.position().add_xyz(
                delta.x / horizontal_distance * TOO_FAR_DISTANCE,
                TOO_FAR_SIGNAL_HEIGHT,
                delta.z / horizontal_distance * TOO_FAR_DISTANCE,
            )
        } else {
            target
        });
        self.life = 0;
        self.survive_after_death = self.base.random.next_int_bounded(5) > 0;
    }
}

/// `updateDeltaMovement(oldMovement, position, target)`.
fn update_delta_movement(old_movement: Vec3, position: Vec3, target: Vec3) -> Vec3 {
    let horizontal_delta = Vec3::new(target.x - position.x, 0.0, target.z - position.z);
    let horizontal_length = horizontal_delta.length();
    let mut wanted_speed = mth::lerp_f64(
        0.0025,
        old_movement.horizontal_distance(),
        horizontal_length,
    );
    let mut movement_y = old_movement.y;
    if horizontal_length < 1.0 {
        wanted_speed *= 0.8;
        movement_y *= 0.8;
    }
    let wanted_movement_y = if position.y - old_movement.y < target.y {
        1.0
    } else {
        -1.0
    };
    horizontal_delta
        .scale(wanted_speed / horizontal_length)
        .add_xyz(
            0.0,
            movement_y + (wanted_movement_y - movement_y) * 0.015,
            0.0,
        )
}

impl Entity for EyeOfEnder {
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

    fn tick(&mut self, level: &mut ServerLevel) {
        self.base_tick(level);
        let new_position = self.position().add(self.base.delta_movement());
        if let Some(target) = self.target {
            let movement = update_delta_movement(self.base.delta_movement(), new_position, target);
            self.base.set_delta_movement(movement);
        }
        self.set_pos_vec(new_position);
        self.life += 1;
        if self.life > 80 {
            self.play_sound(level, SoundEvent::ENTITY_ENDER_EYE_DEATH, 1.0, 1.0);
            level.remove_held_entity(self.as_dyn_mut(), RemovalReason::Discarded);
            if self.survive_after_death {
                let p = self.position();
                let drop = ItemEntity::with_stack(level, p.x, p.y, p.z, self.get_item());
                level.add_fresh_entity(Box::new(drop));
            } else {
                let pos = self.block_position();
                level.level_event(None, 2003, pos, 0);
            }
        }
    }

    fn hurt_server(
        &mut self,
        _level: &mut ServerLevel,
        _source: &DamageSource,
        _amount: f32,
    ) -> bool {
        false
    }

    fn is_attackable(&self) -> bool {
        false
    }

    fn get_light_level_dependent_magic_value(&self, _level: &ServerLevel) -> f32 {
        1.0
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        if let Ok(tag) = self.get_item().to_tag().into_result() {
            out.put("Item", tag);
        }
    }

    fn read_additional_save_data(&mut self, _level: Option<&ServerLevel>, input: &CompoundTag) {
        let item = input
            .get_compound("Item")
            .and_then(load_item_stack)
            .unwrap_or_else(|| ItemStack::new(Item::ENDER_EYE));
        self.set_item(&item);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static EYE_OF_ENDER_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::EYE_OF_ENDER,
    create: |ty, id, _registries| Box::new(EyeOfEnder::new(ty, id)),
};

#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static EYE_OF_ENDER_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 30,
    applies: |e| e.as_any().is::<EyeOfEnder>(),
    fields: |e, _level, _ctx| {
        let eye = e.as_any().downcast_ref::<EyeOfEnder>().expect("eye");
        let target = eye.target.map_or_else(
            || "-".to_string(),
            |t| [t.x, t.y, t.z].map(java_double_to_string).join(","),
        );
        vec![
            ("target".into(), target),
            ("life".into(), eye.life.to_string()),
            (
                "survive".into(),
                u8::from(eye.survive_after_death).to_string(),
            ),
        ]
    },
};
