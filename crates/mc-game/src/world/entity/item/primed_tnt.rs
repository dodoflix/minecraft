//! `net.minecraft.world.entity.item.PrimedTnt`.

use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::{Entity, EntityData, EntityId, MoveSimulationType, MovementEmission, MoverType, entity};
use crate::world::level::BlockGetter;
use crate::world::level::LevelAccessor;
use crate::world::level::explosion::{Explosion, ExplosionDamageCalculator, ExplosionInteraction, default_block_explosion_resistance};
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::FluidState;
use mc_nbt::{CompoundTag, Tag};
use std::any::Any;
use uuid::Uuid;

pub const DEFAULT_FUSE_TIME: i16 = 80;
pub const NO_FUSE: i32 = -1;
const DEFAULT_EXPLOSION_POWER: f32 = 4.0;

define_accessor!(pub DATA_FUSE_ID: Int = PrimedTnt);
define_accessor!(pub DATA_BLOCK_STATE_ID: BlockState = PrimedTnt);

/// `PrimedTnt.USED_PORTAL_DAMAGE_CALCULATOR`.
pub struct UsedPortalDamageCalculator;

impl ExplosionDamageCalculator for UsedPortalDamageCalculator {
    fn should_block_explode(&self, _explosion: &dyn Explosion, _level: &dyn BlockGetter, _pos: BlockPos, state: BlockState, _power: f32) -> bool {
        state.block() != Block::NETHER_PORTAL
    }

    fn get_block_explosion_resistance(&self, _explosion: &dyn Explosion, _level: &dyn BlockGetter, _pos: BlockPos, block: BlockState, fluid: FluidState) -> Option<f32> {
        if block.block() == Block::NETHER_PORTAL { None } else { default_block_explosion_resistance(block, fluid) }
    }
}

pub static USED_PORTAL_DAMAGE_CALCULATOR: UsedPortalDamageCalculator = UsedPortalDamageCalculator;

pub struct PrimedTnt {
    base: EntityData,
    /// `owner` (`EntityReference<LivingEntity>`).
    owner: Option<Uuid>,
    used_portal: bool,
    explosion_power: f32,
}

impl PrimedTnt {
    /// `PrimedTnt(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_FUSE_ID, 80);
            b.define(DATA_BLOCK_STATE_ID, Block::TNT.default_state());
        });
        base.blocks_building = true;
        Self { base, owner: None, used_portal: false, explosion_power: DEFAULT_EXPLOSION_POWER }
    }

    /// `PrimedTnt(level, x, y, z, owner)`: the sideways kick uses the level's random.
    pub fn at(level: &mut ServerLevel, x: f64, y: f64, z: f64, owner: Option<&dyn Entity>) -> Self {
        Self::at_owner_uuid(level, x, y, z, owner.map(|o| o.uuid()))
    }

    /// [`PrimedTnt::at`] with the owner given by uuid (for callers that cannot borrow the owner while the level is borrowed).
    pub fn at_owner_uuid(level: &mut ServerLevel, x: f64, y: f64, z: f64, owner: Option<Uuid>) -> Self {
        let mut tnt = Self::new(EntityType::TNT, level.next_entity_id());
        tnt.set_pos(x, y, z);
        let rot = LevelAccessor::random(level).next_double() * f64::from((std::f64::consts::PI * 2.0) as f32);
        tnt.base.set_delta_movement_xyz(-rot.sin() * 0.02, f64::from(0.2f32), -rot.cos() * 0.02);
        tnt.set_fuse(80);
        tnt.base.xo = x;
        tnt.base.yo = y;
        tnt.base.zo = z;
        tnt.owner = owner;
        tnt
    }

    /// `getOwner()`: the owner if it is a living entity in the level.
    pub fn get_owner<'l>(&self, level: &'l ServerLevel) -> Option<&'l dyn Entity> {
        level.get_entity_by_uuid(self.owner?).filter(|e| e.living().is_some())
    }

    pub fn set_fuse(&mut self, time: i32) {
        self.base.entity_data.set(DATA_FUSE_ID, time);
    }

    pub fn get_fuse(&self) -> i32 {
        self.base.entity_data.get(DATA_FUSE_ID)
    }

    /// `getRandomShortFuse(fuse, random)`.
    pub fn get_random_short_fuse(fuse: i32, random: &mut dyn RandomSource) -> i32 {
        random.next_int_bounded(1.max(fuse / 4)) + fuse / 8
    }

    pub fn set_block_state(&mut self, state: BlockState) {
        self.base.entity_data.set(DATA_BLOCK_STATE_ID, state);
    }

    pub fn get_block_state(&self) -> BlockState {
        self.base.entity_data.get(DATA_BLOCK_STATE_ID)
    }

    /// `setUsedPortal(true)` (called by `teleport` for the new entity).
    pub fn set_used_portal(&mut self, used_portal: bool) {
        self.used_portal = used_portal;
    }

    /// `explode()`.
    fn explode(&self, level: &mut ServerLevel) {
        if level.tnt_explodes() {
            let calculator: Option<&dyn ExplosionDamageCalculator> = self.used_portal.then_some(&USED_PORTAL_DAMAGE_CALCULATOR as &dyn ExplosionDamageCalculator);
            let y = self.base.get_y_progress(0.0625);
            let params = Explode::new(Some(self), self.base.x(), y, self.base.z(), self.explosion_power, ExplosionInteraction::Tnt).with_damage_source(None, calculator);
            level.explode(params, &mut []);
        }
    }
}

impl Entity for PrimedTnt {
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

    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }

    fn is_pickable(&self) -> bool {
        !self.is_removed()
    }

    fn get_default_gravity(&self) -> f64 {
        0.04
    }

    fn move_simulation_type(&self) -> MoveSimulationType {
        MoveSimulationType::ServerAndClient
    }

    fn explosion_indirect_source(&self, level: &ServerLevel) -> Option<EntityId> {
        self.get_owner(level).map(|o| o.id())
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.handle_portal(level);
        self.apply_gravity();
        let movement = self.base.delta_movement();
        let _ = self.r#move(level, MoverType::SelfMover, movement);
        self.apply_effects_from_blocks(level);
        let drag = f64::from(self.get_air_drag());
        self.base.set_delta_movement(self.base.delta_movement().scale(drag));
        if self.base.on_ground() {
            self.base.set_delta_movement(self.base.delta_movement().multiply_xyz(0.7, -0.5, 0.7));
        }
        let fuse = self.get_fuse() - 1;
        self.set_fuse(fuse);
        if fuse <= 0 {
            self.discard();
            self.explode(level);
        } else {
            self.update_fluid_interaction(level);
        }
        let precise = self.base.horizontal_collision || self.base.vertical_collision;
        self.set_requires_precise_position(precise);
    }

    fn hurt_server(&mut self, _level: &mut ServerLevel, _source: &crate::world::damagesource::DamageSource, _amount: f32) -> bool {
        false
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        out.put_short("fuse", self.get_fuse() as i16);
        out.put("block_state", Tag::Compound(crate::world::level::levelgen::structure::templatesystem::structure_template::write_block_state(self.get_block_state())));
        if self.explosion_power != DEFAULT_EXPLOSION_POWER {
            out.put_float("explosion_power", self.explosion_power);
        }
        if let Some(owner) = self.owner {
            out.put_int_array("owner", mc_core::uuid_util::uuid_to_int_array(&owner).to_vec());
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.set_fuse(i32::from(input.get_short_or("fuse", DEFAULT_FUSE_TIME)));
        let state = input
            .get_compound("block_state")
            .filter(|t| t.get_string("id").and_then(|s| mc_core::ident::Identifier::read(s).ok()).and_then(|id| Block::by_name(&id.to_string())).is_some())
            .map(crate::world::level::levelgen::structure::templatesystem::structure_template::read_block_state)
            .unwrap_or_else(|| Block::TNT.default_state());
        self.set_block_state(state);
        self.explosion_power = mth::clamp_f32(input.get_float_or("explosion_power", DEFAULT_EXPLOSION_POWER), 0.0, 128.0);
        self.owner = entity::read_uuid(input.get("owner"));
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static PRIMED_TNT_FACTORY: EntityRegistration = EntityRegistration { entity_type: EntityType::TNT, create: |ty, level| Box::new(PrimedTnt::new(ty, level.next_entity_id())) };

#[allow(dead_code)]
fn _unused(_: Vec3) {}
