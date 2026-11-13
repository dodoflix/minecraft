//! `net.minecraft.world.entity.projectile.LlamaSpit`.

use super::{EntityHitResult, Projectile, ProjectileData};
use crate::server::level::ServerLevel;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::item::enchantment::actors::Actors;
use crate::world::level::BlockGetter;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::BlockHitResult;
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_nbt::CompoundTag;
use std::any::Any;

/// `LlamaSpit`.
pub struct LlamaSpit {
    base: EntityData,
    projectile: ProjectileData,
}

impl LlamaSpit {
    /// `LlamaSpit(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        Self {
            base: EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {}),
            projectile: ProjectileData::default(),
        }
    }

    /// `LlamaSpit(level, owner)` (`owner` is the `Llama`).
    pub fn from_llama(level: &ServerLevel, owner: &dyn Entity) -> Self {
        let mut spit = Self::new(EntityType::LLAMA_SPIT, level.next_entity_id());
        spit.set_owner(Some(owner));
        let width = owner.base().get_bb_width();
        let y_body_rot = owner.living().map_or(0.0, |l| l.y_body_rot);
        let deg_to_rad = (std::f64::consts::PI / 180.0) as f32;
        let b = owner.base();
        spit.set_pos(
            b.x()
                - f64::from(width + 1.0)
                    * 0.5
                    * f64::from(mth::sin(f64::from(y_body_rot * deg_to_rad))),
            b.get_eye_y() - f64::from(0.1f32),
            b.z()
                + f64::from(width + 1.0)
                    * 0.5
                    * f64::from(mth::cos(f64::from(y_body_rot * deg_to_rad))),
        );
        spit
    }
}

impl Projectile for LlamaSpit {
    fn projectile_data(&self) -> &ProjectileData {
        &self.projectile
    }

    fn projectile_data_mut(&mut self) -> &mut ProjectileData {
        &mut self.projectile
    }

    fn on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
        let victim = hit.entity;
        let Some(owner_id) = self
            .get_owner(level)
            .filter(|o| o.living().is_some())
            .map(|o| o.id())
        else {
            return;
        };
        let source = level
            .damage_sources()
            .spit(self.as_dyn(), level.get_entity(owner_id));
        level.with_entity(victim, |level, target| {
            if level.lend(self.as_dyn_mut(), |level| {
                target.hurt_server(level, &source, 1.0)
            }) {
                crate::world::item::enchantment_helper::do_post_attack_effects(
                    level,
                    &mut Actors::of(&mut *target),
                    victim,
                    &source,
                );
            }
        });
    }

    fn on_hit_block(&mut self, level: &mut ServerLevel, hit: &BlockHitResult) {
        super::on_hit_block_base(self, level, hit);
        self.discard();
    }
}

impl Entity for LlamaSpit {
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
    crate::impl_projectile_entity!();

    fn get_default_gravity(&self) -> f64 {
        0.06
    }

    fn get_air_drag(&self) -> f32 {
        0.99
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        super::tick(self, level);
        let movement = self.base.delta_movement();
        let hit_result = super::util::get_hit_result_on_move_vector(level, self, &|e| {
            self.can_hit_entity(level, e)
        });
        super::hit_target_or_deflect_self(self, level, &hit_result);
        let x = self.base.x() + movement.x;
        let y = self.base.y() + movement.y;
        let z = self.base.z() + movement.z;
        super::update_rotation(self);
        let touches_no_air = !BlockPos::between_closed_aabb(&self.get_bounding_box())
            .any(|p| level.get_block_state(p).is_air());
        if touches_no_air || self.is_in_water() {
            self.discard();
        } else {
            let drag = f64::from(self.get_air_drag());
            self.base.set_delta_movement(movement.scale(drag));
            self.apply_gravity();
            self.set_pos(x, y, z);
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        super::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        super::read_additional_save_data(self, input);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static LLAMA_SPIT_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::LLAMA_SPIT,
    create: |ty, id, _registries| Box::new(LlamaSpit::new(ty, id)),
};
