//! `net.minecraft.world.entity.animal.fish.Pufferfish`.

use super::abstract_fish::{self, AbstractFish, abstract_fish_bucketable, abstract_fish_entity};
use linkme::distributed_slice;
use mc_data::enums::Pose;
use mc_data::{EntityType, Item, MobEffect, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::entity::DATA_POSE;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::damage::make_sound;
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::ItemStack;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::ClientboundGameEventPacket;
use std::sync::{Arc, LazyLock};

define_accessor!(pub PUFF_STATE: Int = Pufferfish);

pub const STATE_SMALL: i32 = 0;
pub const STATE_MID: i32 = 1;
pub const STATE_FULL: i32 = 2;

/// `TARGETING_CONDITIONS`: `forNonCombat().ignoreInvisibilityTesting().ignoreLineOfSight().selector(SCARY_MOB)`.
static TARGETING_CONDITIONS: LazyLock<TargetingConditions> = LazyLock::new(|| {
    TargetingConditions::for_non_combat()
        .ignore_invisibility_testing()
        .ignore_line_of_sight()
        .selector(Some(Arc::new(
            |target: &dyn Entity, _level: &ServerLevel| {
                // `SCARY_MOB`: creative players are not scary, else `!target.is(EntityTypeTags.NOT_SCARY_FOR_PUFFERFISH)`
                if mob::ext::is_creative_player(target) {
                    return false;
                }
                !mc_data::tags::get::<EntityType>("not_scary_for_pufferfish")
                    .is_some_and(|t| t.contains(target.get_type()))
            },
        )))
});

pub struct Pufferfish {
    pub mob: MobData,
    inflate_counter: i32,
    deflate_timer: i32,
}

impl Pufferfish {
    /// `new Pufferfish(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::PathfinderMob, |b| {
            abstract_fish::define_synched_data(b);
            b.define(PUFF_STATE, 0);
        });
        let mut fish = Pufferfish {
            mob,
            inflate_counter: 0,
            deflate_timer: 0,
        };
        let navigation = StandardPathNavigation::water_bound(&fish);
        abstract_fish::init_controls(&mut fish.mob, navigation);
        abstract_fish::register_goals(&mut fish);
        fish.mob
            .goal_selector
            .add_goal(1, Box::new(PufferfishPuffGoal));
        fish.refresh_dimensions();
        fish
    }

    /// `getPuffState()`.
    pub fn get_puff_state(&self) -> i32 {
        self.mob.living.entity.entity_data.get(PUFF_STATE)
    }

    /// `setPuffState(state)`.
    pub fn set_puff_state(&mut self, state: i32) {
        self.mob.living.entity.entity_data.set(PUFF_STATE, state);
        self.on_synced_data_updated(PUFF_STATE.id());
    }

    /// `touch(level, mob)`.
    fn touch(&mut self, level: &mut ServerLevel, target: EntityId) {
        let allowed = level.get_entity(target).is_some_and(|m| {
            mc_game_core::world::entity::team::do_teams_allow_damage(level, self, m)
        });
        if !allowed {
            return;
        }
        let puff_state = self.get_puff_state();
        let source = level.damage_sources().mob_attack(self);
        let me = self.id();
        let stung = level
            .with_entity(target, |level, m| {
                if level.lend(&mut *self, |level| {
                    m.hurt_server(level, &source, (1 + puff_state) as f32)
                }) {
                    m.add_effect(
                        level,
                        MobEffectInstance::of(MobEffect::POISON, 60 * puff_state, 0),
                        Some(me),
                    );
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        if stung {
            self.play_sound(level, SoundEvent::ENTITY_PUFFER_FISH_STING, 1.0, 1.0);
        }
    }

    /// `getScale(state)`.
    fn get_scale(state: i32) -> f32 {
        match state {
            0 => 0.5,
            1 => 0.7,
            _ => 1.0,
        }
    }
}

impl AbstractFish for Pufferfish {
    fn after_ai_step(&mut self, level: &mut ServerLevel) {
        if self.is_alive() && self.get_puff_state() > 0 {
            let bb = self.base().bounding_box().inflate(0.3, 0.3, 0.3);
            let targets = level.get_entities_of_class(
                &bb,
                |e| e.mob().is_some(),
                |e| TARGETING_CONDITIONS.test(level, Some(&*self), e),
            );
            for target in targets {
                if level.get_entity(target).is_some_and(|m| m.is_alive()) {
                    self.touch(level, target);
                }
            }
        }
    }

    fn get_flop_sound(&self) -> SoundEvent {
        SoundEvent::ENTITY_PUFFER_FISH_FLOP
    }
    fn get_bucket_item_stack(&self) -> ItemStack {
        ItemStack::new(Item::PUFFERFISH_BUCKET)
    }
}

abstract_fish_bucketable!(Pufferfish);

impl Entity for Pufferfish {
    abstract_fish_entity!("minecraft:pufferfish");

    fn get_max_spawn_cluster_size(&self) -> i32 {
        8
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        if self.is_alive() && self.is_effective_ai() {
            if self.inflate_counter > 0 {
                if self.get_puff_state() == 0 {
                    make_sound(self, level, SoundEvent::ENTITY_PUFFER_FISH_BLOW_UP);
                    self.set_puff_state(1);
                } else if self.inflate_counter > 40 && self.get_puff_state() == 1 {
                    make_sound(self, level, SoundEvent::ENTITY_PUFFER_FISH_BLOW_UP);
                    self.set_puff_state(2);
                }
                self.inflate_counter += 1;
            } else if self.get_puff_state() != 0 {
                if self.deflate_timer > 60 && self.get_puff_state() == 2 {
                    make_sound(self, level, SoundEvent::ENTITY_PUFFER_FISH_BLOW_OUT);
                    self.set_puff_state(1);
                } else if self.deflate_timer > 100 && self.get_puff_state() == 1 {
                    make_sound(self, level, SoundEvent::ENTITY_PUFFER_FISH_BLOW_OUT);
                    self.set_puff_state(0);
                }
                self.deflate_timer += 1;
            }
        }
        mob::tick::tick(self, level);
    }

    fn player_touch(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        if mc_game_core::world::entity::team::do_teams_allow_damage(level, self, player) {
            let puff_state = self.get_puff_state();
            let source = level.damage_sources().mob_attack(self);
            if puff_state > 0
                && level.lend(&mut *self, |level| {
                    player.hurt_server(level, &source, (1 + puff_state) as f32)
                })
            {
                if !self.base().is_silent() {
                    player.send(&ClientboundGameEventPacket {
                        event: ClientboundGameEventPacket::PUFFER_FISH_STING,
                        param: 0.0,
                    });
                }
                let me = self.id();
                player.add_effect(
                    level,
                    MobEffectInstance::of(MobEffect::POISON, 60 * puff_state, 0),
                    Some(me),
                );
            }
        }
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == PUFF_STATE.id() || accessor_id == DATA_POSE.id() {
            self.refresh_dimensions();
        }
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        EntityDimensions::of_type(self.get_type()).scale(Self::get_scale(self.get_puff_state()))
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PUFFER_FISH_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PUFFER_FISH_HURT)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_fish::add_additional_save_data(self, out);
        out.put_int("PuffState", self.get_puff_state());
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        abstract_fish::read_additional_save_data(self, level, input);
        self.set_puff_state(input.get_int_or("PuffState", 0).min(2));
    }
}

/// `Pufferfish.PufferfishPuffGoal`.
struct PufferfishPuffGoal;

impl Goal for PufferfishPuffGoal {
    fn name(&self) -> &'static str {
        "Pufferfish$PufferfishPuffGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let bb = mob.base().bounding_box().inflate(2.0, 2.0, 2.0);
        !level
            .get_entities_of_class(
                &bb,
                |e| e.living().is_some(),
                |e| TARGETING_CONDITIONS.test(level, Some(&*mob), e),
            )
            .is_empty()
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        if let Some(fish) = mob.as_any_mut().downcast_mut::<Pufferfish>() {
            fish.inflate_counter = 1;
            fish.deflate_timer = 0;
        }
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        if let Some(fish) = mob.as_any_mut().downcast_mut::<Pufferfish>() {
            fish.inflate_counter = 0;
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static PUFFERFISH: EntityRegistration = EntityRegistration {
    entity_type: EntityType::PUFFERFISH,
    create: |ty, id, _registries| Box::new(Pufferfish::new(ty, id)),
};

/// The scenario dump fields of a pufferfish: `puff inflate deflate`.
#[distributed_slice(mc_game_core::world::entity::scenario_dump::SCENARIO_DUMP_EXTRA)]
static PUFFERFISH_DUMP: mc_game_core::world::entity::scenario_dump::ScenarioDumpExtra =
    mc_game_core::world::entity::scenario_dump::ScenarioDumpExtra {
        order: 30,
        applies: |e| e.as_any().is::<Pufferfish>(),
        fields: |e, _level, _ctx| {
            let fish = e.as_any().downcast_ref::<Pufferfish>().expect("applies");
            vec![
                ("puff".into(), fish.get_puff_state().to_string()),
                ("inflate".into(), fish.inflate_counter.to_string()),
                ("deflate".into(), fish.deflate_timer.to_string()),
            ]
        },
    };
