//! `net.minecraft.world.entity.projectile.EvokerFangs`: the spikes of the evoker (`TraceableEntity`).

use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::item::enchantment::actors::Actors;
use linkme::distributed_slice;
use mc_core::phys::AABB;
use mc_data::EntityType;
use mc_nbt::CompoundTag;
use std::any::Any;
use uuid::Uuid;

pub const ATTACK_DURATION: i32 = 20;
pub const LIFE_OFFSET: i32 = 2;
pub const ATTACK_TRIGGER_TICKS: i32 = 14;

/// `EvokerFangs`.
pub struct EvokerFangs {
    base: EntityData,
    warmup_delay_ticks: i32,
    sent_spike_event: bool,
    life_ticks: i32,
    /// `owner` (`EntityReference<LivingEntity>`).
    owner: Option<Uuid>,
}

impl EvokerFangs {
    /// `EvokerFangs(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        Self {
            base: EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {}),
            warmup_delay_ticks: 0,
            sent_spike_event: false,
            life_ticks: 22,
            owner: None,
        }
    }

    /// `EvokerFangs(level, x, y, z, rotationRadians, warmupDelayTicks, owner)`.
    pub fn at(
        level: &ServerLevel,
        x: f64,
        y: f64,
        z: f64,
        rotation_radians: f32,
        warmup_delay_ticks: i32,
        owner: &dyn Entity,
    ) -> Self {
        let mut fangs = Self::new(EntityType::EVOKER_FANGS, level.next_entity_id());
        fangs.warmup_delay_ticks = warmup_delay_ticks;
        fangs.set_owner(Some(owner));
        fangs
            .base
            .set_y_rot(rotation_radians * (180.0f32 / std::f32::consts::PI));
        fangs.set_pos(x, y, z);
        fangs
    }

    /// `setOwner(owner)`.
    pub fn set_owner(&mut self, owner: Option<&dyn Entity>) {
        self.owner = owner.map(|o| o.uuid());
    }

    /// `getOwner()`.
    pub fn get_owner<'l>(&self, level: &'l ServerLevel) -> Option<&'l dyn Entity> {
        level
            .get_entity_by_uuid(self.owner?)
            .filter(|e| e.living().is_some())
    }

    /// `dealDamageTo(entity)`.
    fn deal_damage_to(&mut self, level: &mut ServerLevel, victim: EntityId) {
        let owner_id = self.get_owner(level).map(|o| o.id());
        let Some(target) = level.get_entity(victim) else {
            return;
        };
        if !(target.is_alive() && !target.is_invulnerable() && Some(victim) != owner_id) {
            return;
        }
        match owner_id.and_then(|id| level.get_entity(id)) {
            None => {
                let source = level.damage_sources().magic();
                level.with_entity(victim, |level, t| {
                    level.lend(self.as_dyn_mut(), |level| {
                        t.hurt_server(level, &source, 6.0)
                    })
                });
            }
            Some(owner) => {
                if crate::world::entity::mob::ext::is_allied_to(level, owner, target) {
                    return;
                }
                let source = level
                    .damage_sources()
                    .indirect_magic(self.as_dyn(), Some(owner));
                level.with_entity(victim, |level, t| {
                    if level.lend(self.as_dyn_mut(), |level| {
                        t.hurt_server(level, &source, 6.0)
                    }) {
                        crate::world::item::enchantment_helper::do_post_attack_effects(
                            level,
                            &mut Actors::of(&mut *t),
                            victim,
                            &source,
                        );
                    }
                });
            }
        }
    }
}

impl Entity for EvokerFangs {
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
        self.warmup_delay_ticks -= 1;
        if self.warmup_delay_ticks < 0 {
            if self.warmup_delay_ticks == -8 {
                let bb: AABB = self.get_bounding_box().inflate(0.2, 0.0, 0.2);
                for id in level.get_entities_of_class(&bb, |e| e.living().is_some(), |_| true) {
                    self.deal_damage_to(level, id);
                }
            }
            if !self.sent_spike_event {
                level.broadcast_entity_event(self.base.id, 4);
                self.sent_spike_event = true;
            }
            self.life_ticks -= 1;
            if self.life_ticks < 0 {
                self.discard();
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

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        out.put_int("Warmup", self.warmup_delay_ticks);
        if let Some(owner) = self.owner {
            out.put_int_array(
                "Owner",
                mc_core::uuid_util::uuid_to_int_array(&owner).to_vec(),
            );
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.warmup_delay_ticks = input.get_int_or("Warmup", 0);
        self.owner = entity::read_uuid(input.get("Owner"));
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static EVOKER_FANGS_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::EVOKER_FANGS,
    create: |ty, id, _registries| Box::new(EvokerFangs::new(ty, id)),
};

#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static EVOKER_FANGS_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 30,
    applies: |e| e.as_any().is::<EvokerFangs>(),
    fields: |e, _level, ctx| {
        let f = e.as_any().downcast_ref::<EvokerFangs>().expect("fangs");
        vec![
            ("warmup".into(), f.warmup_delay_ticks.to_string()),
            ("spike".into(), u8::from(f.sent_spike_event).to_string()),
            ("life".into(), f.life_ticks.to_string()),
            ("owner".into(), ctx.name_uuid(f.owner)),
        ]
    },
};
