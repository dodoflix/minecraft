//! `Evoker`: the spellcasting illager (fang rows and circles, vex summoning, sheep recolouring).

use super::super::vex::Vex;
use linkme::distributed_slice;
use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, GameEvent, SoundEvent};
use mc_game_ai::world::entity::ai::goal::AvoidEntityGoal;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::goal::look_at_player::LookAtType;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of_type, class_of_types,
};
use mc_game_core::world::entity::ai::goal::{FloatGoal, LookAtPlayerGoal, RandomStrollGoal};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::monster::illager::abstract_illager;
use mc_game_core::world::entity::monster::illager::spellcaster_illager::{
    self, IllagerSpell, SpellcasterCastingSpellGoal, SpellcasterData, SpellcasterUseSpellGoal,
    UseSpell, UseSpellState,
};
use mc_game_core::world::entity::projectile::evoker_fangs::EvokerFangs;
use mc_game_core::world::entity::raid::raid::Raid;
use mc_game_core::world::entity::raid::raider::{self, RaiderData};
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::{BlockGetter, LevelReader};
use mc_nbt::CompoundTag;
use mc_protocol::components::DyeColor;
use std::sync::Arc;

pub struct Evoker {
    pub mob: MobData,
    raider: RaiderData,
    spellcaster: SpellcasterData,
}

impl Evoker {
    /// `new Evoker(type, level)`: `registerGoals()`, the ground navigation and `xpReward = 10`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            raider::define_synched_data(b);
            spellcaster_illager::define_synched_data(b);
        });
        let mut evoker = Evoker {
            mob,
            raider: RaiderData::default(),
            spellcaster: SpellcasterData::new(SoundEvent::ENTITY_EVOKER_CAST_SPELL),
        };
        let navigation = StandardPathNavigation::ground(&evoker);
        evoker.mob.set_navigation(Box::new(navigation));
        evoker.mob.xp_reward = 10;
        evoker.register_goals();
        evoker
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        raider::register_goals(&mut self.mob);
        let float = FloatGoal::new(self);
        let look_at_player = LookAtPlayerGoal::with_probability(self, LookAtType::Player, 3.0, 1.0);
        let look_at_mob = LookAtPlayerGoal::new(
            self,
            LookAtType::Class(Arc::new(|e: &dyn Entity| e.mob().is_some())),
            8.0,
        );
        let sel = &mut self.mob.goal_selector;
        sel.add_goal(0, Box::new(float));
        sel.add_goal(1, Box::new(SpellcasterCastingSpellGoal::evoker()));
        sel.add_goal(
            2,
            Box::new(AvoidEntityGoal::new(
                Arc::new(|e: &dyn Entity| e.is_player()),
                8.0,
                0.6,
                1.0,
            )),
        );
        sel.add_goal(
            3,
            Box::new(AvoidEntityGoal::new(
                Arc::new(|e: &dyn Entity| e.get_type() == EntityType::CREAKING),
                8.0,
                0.6,
                1.0,
            )),
        );
        sel.add_goal(
            4,
            Box::new(SpellcasterUseSpellGoal::new(EvokerSummonSpell::new())),
        );
        sel.add_goal(5, Box::new(SpellcasterUseSpellGoal::new(EvokerAttackSpell)));
        sel.add_goal(
            6,
            Box::new(SpellcasterUseSpellGoal::new(EvokerWololoSpell::new())),
        );
        sel.add_goal(8, Box::new(RandomStrollGoal::new(0.6)));
        sel.add_goal(9, Box::new(look_at_player));
        sel.add_goal(10, Box::new(look_at_mob));
        let t = &mut self.mob.target_selector;
        t.add_goal(
            1,
            Box::new(
                HurtByTargetGoal::new(vec![Arc::new(|e: &dyn Entity| e.raider().is_some())])
                    .set_alert_others(Vec::new()),
            ),
        );
        t.add_goal(
            2,
            Box::new(
                NearestAttackableTargetGoal::new(TargetType::Player, true)
                    .set_unseen_memory_ticks(300),
            ),
        );
        t.add_goal(
            3,
            Box::new(
                NearestAttackableTargetGoal::new(
                    TargetType::Class(class_of_types(&[
                        EntityType::VILLAGER,
                        EntityType::WANDERING_TRADER,
                    ])),
                    false,
                )
                .set_unseen_memory_ticks(300),
            ),
        );
        t.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::new(
                TargetType::Class(class_of_type(EntityType::IRON_GOLEM)),
                false,
            )),
        );
    }
}

impl Entity for Evoker {
    mc_game_core::mob_entity_accessors!(mob);
    mc_game_core::raider_entity!(
        raider;
        ai_step,
        update_no_action_time,
        die,
        hurt_server,
        can_join_patrol,
        remove_when_far_away,
        requires_custom_persistence,
        finalize_spawn,
        pick_up_item,
        after_loaded
    );

    fn spellcaster(&self) -> Option<&SpellcasterData> {
        Some(&self.spellcaster)
    }

    fn spellcaster_mut(&mut self) -> Option<&mut SpellcasterData> {
        Some(&mut self.spellcaster)
    }

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:evoker")
    }

    fn custom_server_ai_step(&mut self, _level: &mut ServerLevel) {
        spellcaster_illager::custom_server_ai_step(self);
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        spellcaster_illager::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        spellcaster_illager::read_additional_save_data(self, level, input);
    }

    fn can_attack(&self, level: &ServerLevel, target: &dyn Entity) -> bool {
        abstract_illager::can_attack(self, level, target)
    }

    fn considers_entity_as_ally(&self, level: &ServerLevel, other: &dyn Entity) -> bool {
        if other.id() == self.id() || abstract_illager::considers_entity_as_ally(self, level, other)
        {
            return true;
        }
        other
            .as_any()
            .downcast_ref::<Vex>()
            .and_then(|vex| vex.get_root_owner(level))
            .is_some_and(|root| {
                root.id() == self.id()
                    || abstract_illager::considers_entity_as_ally(self, level, root)
            })
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_EVOKER_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_EVOKER_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_EVOKER_HURT)
    }

    fn get_celebrate_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_EVOKER_CELEBRATE)
    }

    fn apply_raid_buffs(
        &mut self,
        _level: &mut ServerLevel,
        _wave: i32,
        _is_captain: bool,
        _raid: &Raid,
    ) {
    }
}

/// `Evoker.EvokerAttackSpellGoal`.
struct EvokerAttackSpell;

impl EvokerAttackSpell {
    /// `createSpellEntity(x, z, minY, maxY, angle, delayTicks)`: fangs on the first solid floor at or below `maxY`.
    fn create_spell_entity(
        mob: &dyn Entity,
        level: &mut ServerLevel,
        (x, z): (f64, f64),
        (min_y, max_y): (f64, f64),
        angle: f32,
        delay_ticks: i32,
    ) {
        let mut pos = BlockPos::containing(x, max_y, z);
        let mut success = false;
        let mut top_offset = 0.0;
        loop {
            let below = pos.below();
            let below_state = level.get_block_state(below);
            if below_state.is_face_sturdy_at(&*level, below, Direction::Up) {
                if !level.is_empty_block(pos) {
                    let state = level.get_block_state(pos);
                    let shape = state.get_collision_shape(&*level, pos);
                    if !shape.is_empty() {
                        top_offset = shape.max(Axis::Y);
                    }
                }
                success = true;
                break;
            }
            pos = pos.below();
            if pos.y() < mth::floor_f64(min_y) - 1 {
                break;
            }
        }
        if success {
            let y = f64::from(pos.y()) + top_offset;
            let fangs = EvokerFangs::at(level, x, y, z, angle, delay_ticks, mob);
            level.add_fresh_entity(Box::new(fangs));
            level.game_event_from_entity_ref(mob, GameEvent::ENTITY_PLACE, Vec3::new(x, y, z));
        }
    }
}

impl UseSpell for EvokerAttackSpell {
    fn name(&self) -> &'static str {
        "Evoker$EvokerAttackSpellGoal"
    }

    fn spell(&self) -> IllagerSpell {
        IllagerSpell::Fangs
    }

    fn spell_prepare_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_EVOKER_PREPARE_ATTACK)
    }

    fn casting_time(&self) -> i32 {
        40
    }

    fn casting_interval(&self) -> i32 {
        100
    }

    fn perform_spell_casting(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target) = mob::tick::get_target_checked(mob, level)
            .and_then(|t| level.get_entity(t))
            .map(|t| (t.base().x(), t.base().y(), t.base().z(), t.position()))
        else {
            return;
        };
        let (target_x, target_y, target_z, target_position) = target;
        let (mx, my, mz) = (mob.base().x(), mob.base().y(), mob.base().z());
        let min_y = target_y.min(my);
        let max_y = target_y.max(my) + 1.0;
        let angle_towards_target = mth::atan2(target_z - mz, target_x - mx) as f32;
        let pi = std::f32::consts::PI;
        let mob = &*mob;
        if mob.base().distance_to_sqr_vec(target_position) < 9.0 {
            for i in 0..5 {
                let angle = angle_towards_target + i as f32 * pi * 0.4;
                Self::create_spell_entity(
                    mob,
                    level,
                    (
                        mx + f64::from(mth::cos(f64::from(angle))) * 1.5,
                        mz + f64::from(mth::sin(f64::from(angle))) * 1.5,
                    ),
                    (min_y, max_y),
                    angle,
                    0,
                );
            }
            for i in 0..8 {
                let angle = angle_towards_target
                    + i as f32 * pi * 2.0 / 8.0
                    + (std::f64::consts::PI * 2.0 / 5.0) as f32;
                Self::create_spell_entity(
                    mob,
                    level,
                    (
                        mx + f64::from(mth::cos(f64::from(angle))) * 2.5,
                        mz + f64::from(mth::sin(f64::from(angle))) * 2.5,
                    ),
                    (min_y, max_y),
                    angle,
                    3,
                );
            }
        } else {
            for i in 0..16 {
                let reach = 1.25 * f64::from(i + 1);
                Self::create_spell_entity(
                    mob,
                    level,
                    (
                        mx + f64::from(mth::cos(f64::from(angle_towards_target))) * reach,
                        mz + f64::from(mth::sin(f64::from(angle_towards_target))) * reach,
                    ),
                    (min_y, max_y),
                    angle_towards_target,
                    i,
                );
            }
        }
    }
}

/// `Evoker.EvokerSummonSpellGoal`.
struct EvokerSummonSpell {
    vex_count_targeting: TargetingConditions,
}

impl EvokerSummonSpell {
    fn new() -> Self {
        Self {
            vex_count_targeting: TargetingConditions::for_non_combat()
                .range(16.0)
                .ignore_line_of_sight()
                .ignore_invisibility_testing(),
        }
    }
}

impl UseSpell for EvokerSummonSpell {
    fn name(&self) -> &'static str {
        "Evoker$EvokerSummonSpellGoal"
    }

    fn spell(&self) -> IllagerSpell {
        IllagerSpell::SummonVex
    }

    fn spell_prepare_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_EVOKER_PREPARE_SUMMON)
    }

    fn casting_time(&self) -> i32 {
        100
    }

    fn casting_interval(&self) -> i32 {
        340
    }

    fn can_use(
        &mut self,
        state: &UseSpellState,
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
    ) -> bool {
        if !state.can_use(mob, level) {
            return false;
        }
        let bb = mob.base().bounding_box().inflate(16.0, 16.0, 16.0);
        let vexes = mob::ext::get_nearby_entities(
            level,
            |e| e.as_any().is::<Vex>(),
            &self.vex_count_targeting,
            mob,
            &bb,
        )
        .len() as i32;
        mob.base_mut().random.next_int_bounded(8) + 1 > vexes
    }

    fn perform_spell_casting(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let evoker_team = level
            .scoreboard()
            .get_players_team(&mob.get_scoreboard_name())
            .map(|t| t.get_name().to_string());
        for _ in 0..3 {
            let random = &mut mob.base_mut().random;
            let (dx, dz) = (
                -2 + random.next_int_bounded(5),
                -2 + random.next_int_bounded(5),
            );
            let pos = mob.block_position().offset(dx, 1, dz);
            let Some(mut vex) = mc_game_core::world::entity::factory::create(
                EntityType::VEX,
                level,
                EntitySpawnRequest {
                    reason: EntitySpawnReason::MobSummoned,
                    ignore_checks: false,
                },
            ) else {
                continue;
            };
            vex.snap_to(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()),
                f64::from(pos.z()) + 0.5,
                0.0,
                0.0,
            );
            let difficulty = ServerLevelAccessor::get_current_difficulty_at(&*level, pos);
            vex.finalize_spawn(level, difficulty, EntitySpawnReason::MobSummoned, None);
            let life = 20 * (30 + mob.base_mut().random.next_int_bounded(90));
            let v = vex.as_any_mut().downcast_mut::<Vex>().expect("vex");
            v.set_owner(&*mob);
            v.set_bound_origin(Some(pos));
            v.set_limited_life(life);
            if let Some(team) = &evoker_team {
                level
                    .scoreboard_mut()
                    .add_player_to_team(&vex.get_scoreboard_name(), team);
            }
            level.add_fresh_entity_with_passengers(vec![vex]);
            level.game_event_from_entity_ref(
                &*mob,
                GameEvent::ENTITY_PLACE,
                Vec3::new(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z())),
            );
        }
    }
}

/// `Evoker.EvokerWololoSpellGoal`.
struct EvokerWololoSpell {
    wololo_targeting: TargetingConditions,
}

impl EvokerWololoSpell {
    fn new() -> Self {
        Self {
            wololo_targeting: TargetingConditions::for_non_combat()
                .range(16.0)
                .selector(Some(Arc::new(
                    |target: &dyn Entity, _level: &ServerLevel| {
                        target.sheep_color() == Some(DyeColor::Blue)
                    },
                ))),
        }
    }
}

impl UseSpell for EvokerWololoSpell {
    fn name(&self) -> &'static str {
        "Evoker$EvokerWololoSpellGoal"
    }

    fn spell(&self) -> IllagerSpell {
        IllagerSpell::Wololo
    }

    fn spell_prepare_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_EVOKER_PREPARE_WOLOLO)
    }

    fn cast_warmup_time(&self) -> i32 {
        40
    }

    fn casting_time(&self) -> i32 {
        60
    }

    fn casting_interval(&self) -> i32 {
        140
    }

    fn can_use(
        &mut self,
        state: &UseSpellState,
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
    ) -> bool {
        if mob::tick::get_target_checked(mob, level).is_some()
            || spellcaster_illager::is_casting_spell(mob)
            || mob.base().tick_count < state.next_attack_tick_count
            || !level.mob_griefing()
        {
            return false;
        }
        let bb = mob.base().bounding_box().inflate(16.0, 4.0, 16.0);
        let sheep = mob::ext::get_nearby_entities(
            level,
            |e| e.get_type() == EntityType::SHEEP,
            &self.wololo_targeting,
            mob,
            &bb,
        );
        if sheep.is_empty() {
            return false;
        }
        let pick = mob.base_mut().random.next_int_bounded(sheep.len() as i32) as usize;
        mob.spellcaster_mut().expect("evoker").wololo_target = Some(sheep[pick]);
        true
    }

    fn can_continue_to_use(
        &mut self,
        state: &UseSpellState,
        mob: &mut dyn Entity,
        _level: &mut ServerLevel,
    ) -> bool {
        mob.spellcaster().is_some_and(|s| s.wololo_target.is_some())
            && state.attack_warmup_delay > 0
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        mob.spellcaster_mut().expect("evoker").wololo_target = None;
    }

    fn perform_spell_casting(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(sheep) = mob.spellcaster().and_then(|s| s.wololo_target) {
            level.with_entity(sheep, |_, sheep| {
                if sheep.is_alive() {
                    sheep.set_sheep_color(DyeColor::Red);
                }
            });
        }
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static EVOKER: EntityRegistration = EntityRegistration {
    entity_type: EntityType::EVOKER,
    create: |ty, id, registries| Box::new(Evoker::new(ty, id, registries)),
};

/// Scenario-harness fields of a spellcaster illager (order 41; the twin of the `SpellcasterIllager` entry of
/// `EntityExtras.java`) and of an evoker (order 42).
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static SPELLCASTER_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 41,
    applies: |e| e.spellcaster().is_some(),
    fields: |e, _level, _ctx| {
        let s = e.spellcaster().expect("spellcaster");
        vec![
            ("spell".into(), s.current_spell.name().into()),
            ("spellticks".into(), s.spell_casting_tick_count.to_string()),
        ]
    },
};

#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static EVOKER_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 42,
    applies: |e| e.as_any().is::<Evoker>(),
    fields: |e, _level, ctx| {
        let s = e.spellcaster().expect("evoker");
        vec![("wololo".into(), ctx.name_opt(s.wololo_target))]
    },
};
