//! `net.minecraft.world.entity.monster.Endermite`.

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, SoundEvent};
use mc_game_ai::world::entity::ai::goal::ClimbOnTopOfPowderSnowGoal;
use mc_game_core::register_spawn_placement;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType,
};
use mc_game_core::world::entity::ai::goal::{
    FloatGoal, LookAtPlayerGoal, MeleeAttackGoal, RandomLookAroundGoal,
    WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::entity::MovementEmission;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_predicates::{
    check_any_light_monster_spawn_rules, is_spawner,
};
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;

const MAX_LIFE: i32 = 2400;

/// `Endermite`.
pub struct Endermite {
    pub mob: MobData,
    life: i32,
}

impl Endermite {
    /// `new Endermite(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mut mob = MobData::new(ty, id, MobKind::Monster, |_| {});
        mob.xp_reward = 3;
        let mut endermite = Endermite { mob, life: 0 };
        let navigation = StandardPathNavigation::ground(&endermite);
        endermite.mob.set_navigation(Box::new(navigation));
        endermite.register_goals();
        endermite
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let look_at = LookAtPlayerGoal::new_player(self, 8.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(1, Box::new(ClimbOnTopOfPowderSnowGoal::new()));
        goals.add_goal(2, Box::new(MeleeAttackGoal::new(1.0, false)));
        goals.add_goal(3, Box::new(WaterAvoidingRandomStrollGoal::new(1.0)));
        goals.add_goal(7, Box::new(look_at));
        goals.add_goal(8, Box::new(RandomLookAroundGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(
            1,
            Box::new(HurtByTargetGoal::new(Vec::new()).set_alert_others(Vec::new())),
        );
        targets.add_goal(
            2,
            Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)),
        );
    }
}

impl Entity for Endermite {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::Events
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ENDERMITE_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ENDERMITE_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_ENDERMITE_DEATH)
    }

    fn play_step_sound(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: mc_data::block::BlockState,
    ) {
        self.play_sound(level, SoundEvent::ENTITY_ENDERMITE_STEP, 0.15, 1.0);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        self.life = input.get_int_or("Lifetime", 0);
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_int("Lifetime", self.life);
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.mob.living.y_body_rot = self.base().y_rot();
        mob::tick::tick(self, level);
    }

    fn set_y_body_rot(&mut self, y_body_rot: f32) {
        self.base_mut().set_y_rot(y_body_rot);
        self.mob.living.y_body_rot = y_body_rot;
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        mob::tick::ai_step(self, level);
        if !self.is_persistence_required() {
            self.life += 1;
        }
        if self.life >= MAX_LIFE {
            self.discard_in_level(level);
        }
    }
}

/// `Endermite.checkEndermiteSpawnRules`.
fn check_endermite_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    if !check_any_light_monster_spawn_rules(ty, level, reason, pos, random) {
        false
    } else if is_spawner(reason) {
        true
    } else {
        level
            .get_nearest_player_ignoring_creative(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()) + 0.5,
                f64::from(pos.z()) + 0.5,
                5.0,
            )
            .is_none()
    }
}

register_spawn_placement!(EntityType::ENDERMITE, check_endermite_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static ENDERMITE: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ENDERMITE,
    create: |ty, id, registries| Box::new(Endermite::new(ty, id, registries)),
};
