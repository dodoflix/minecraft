//! `Vindicator`: the axe illager (breaks doors during raids, "Johnny" attacks every living entity).

use super::raider_open_door_goal::RaiderOpenDoorGoal;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, SoundEvent};
use mc_game_ai::world::entity::ai::goal::AvoidEntityGoal;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::look_at_player::LookAtType;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of_type, class_of_types,
};
use mc_game_core::world::entity::ai::goal::{
    BreakDoorGoal, FloatGoal, Goal, GoalFlags, LookAtPlayerGoal, MeleeAttackGoal, RandomStrollGoal,
    reduced_tick_delay,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::util::goal_utils;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::monster::illager::abstract_illager::{self, IllagerArmPose};
use mc_game_core::world::entity::raid::raid::Raid;
use mc_game_core::world::entity::raid::raider::{self, RaiderData};
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::item::enchantment_helper::enchant_item_from_provider;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_text::Component;
use mc_world::level_storage::Difficulty;
use std::sync::Arc;

const TAG_JOHNNY: &str = "Johnny";

/// `new Vindicator.VindicatorBreakDoorGoal(mob)`: the door breaking of the raid, `BreakDoorGoal(mob, 6,
/// DOOR_BREAKING_PREDICATE)` that only runs while the raid is active.
struct VindicatorBreakDoorGoal {
    break_door: BreakDoorGoal,
}

impl VindicatorBreakDoorGoal {
    fn new(mob: &dyn Entity) -> Self {
        Self {
            break_door: BreakDoorGoal::with_seconds(
                mob,
                6,
                Arc::new(|d: Difficulty| d == Difficulty::Normal || d == Difficulty::Hard),
            ),
        }
    }
}

impl Goal for VindicatorBreakDoorGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        raider::has_active_raid(mob, level)
            && mob
                .base_mut()
                .random
                .next_int_bounded(reduced_tick_delay(10))
                == 0
            && self.break_door.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        raider::has_active_raid(mob, level) && self.break_door.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.break_door.start(mob, level);
        mob.living_mut().expect("living").no_action_time = 0;
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.break_door.stop(mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.break_door.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.break_door.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[mc_game_core::world::entity::ai::goal::Flag::Move])
    }
}

/// `Vindicator.VindicatorJohnnyAttackGoal`: targets any attackable living entity, only for a Johnny.
struct VindicatorJohnnyAttackGoal {
    nearest: NearestAttackableTargetGoal,
}

impl VindicatorJohnnyAttackGoal {
    fn new() -> Self {
        Self {
            nearest: NearestAttackableTargetGoal::build(
                TargetType::Class(Arc::new(|e: &dyn Entity| e.living().is_some())),
                0,
                true,
                true,
                Some(Arc::new(|target: &dyn Entity, _level: &ServerLevel| {
                    target.attackable()
                })),
            ),
        }
    }
}

impl Goal for VindicatorJohnnyAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob.as_any()
            .downcast_ref::<Vindicator>()
            .is_some_and(|v| v.is_johnny)
            && self.nearest.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.nearest.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.nearest.start(mob, level);
        mob.living_mut().expect("living").no_action_time = 0;
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.nearest.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.nearest.flags()
    }
}

pub struct Vindicator {
    pub mob: MobData,
    raider: RaiderData,
    is_johnny: bool,
}

impl Vindicator {
    /// `new Vindicator(type, level)`: `registerGoals()` and the ground navigation.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, raider::define_synched_data);
        let mut vindicator = Vindicator {
            mob,
            raider: RaiderData::default(),
            is_johnny: false,
        };
        let navigation = StandardPathNavigation::ground(&vindicator);
        vindicator.mob.set_navigation(Box::new(navigation));
        vindicator.register_goals();
        vindicator
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        raider::register_goals(&mut self.mob);
        let float = FloatGoal::new(self);
        let break_door = VindicatorBreakDoorGoal::new(self);
        let open_door = RaiderOpenDoorGoal::new(self);
        let look_at_player = LookAtPlayerGoal::with_probability(self, LookAtType::Player, 3.0, 1.0);
        let look_at_mob = LookAtPlayerGoal::new(
            self,
            LookAtType::Class(Arc::new(|e: &dyn Entity| e.mob().is_some())),
            8.0,
        );
        let sel = &mut self.mob.goal_selector;
        sel.add_goal(0, Box::new(float));
        sel.add_goal(
            1,
            Box::new(AvoidEntityGoal::new(
                Arc::new(|e: &dyn Entity| e.get_type() == EntityType::CREAKING),
                8.0,
                1.0,
                1.2,
            )),
        );
        sel.add_goal(2, Box::new(break_door));
        sel.add_goal(3, Box::new(open_door));
        sel.add_goal(4, Box::new(raider::HoldGroundAttackGoal::new(10.0)));
        sel.add_goal(5, Box::new(MeleeAttackGoal::new(1.0, false)));
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
            Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)),
        );
        t.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::new(
                TargetType::Class(class_of_types(&[
                    EntityType::VILLAGER,
                    EntityType::WANDERING_TRADER,
                ])),
                true,
            )),
        );
        t.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::new(
                TargetType::Class(class_of_type(EntityType::IRON_GOLEM)),
                true,
            )),
        );
        t.add_goal(4, Box::new(VindicatorJohnnyAttackGoal::new()));
        let sel = &mut self.mob.goal_selector;
        sel.add_goal(8, Box::new(RandomStrollGoal::new(0.6)));
        sel.add_goal(9, Box::new(look_at_player));
        sel.add_goal(10, Box::new(look_at_mob));
    }

    /// `getArmPose()`.
    pub fn get_arm_pose(&self) -> IllagerArmPose {
        if self.mob.is_aggressive() {
            IllagerArmPose::Attacking
        } else if raider::is_celebrating(self) {
            IllagerArmPose::Celebrating
        } else {
            IllagerArmPose::Crossed
        }
    }

    fn set_can_open_doors(&mut self, can_open_doors: bool) {
        if let Some(navigation) = self.mob.navigation.as_mut() {
            navigation.set_can_open_doors(can_open_doors);
        }
    }
}

impl Entity for Vindicator {
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
        after_loaded
    );

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:vindicator")
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        if !self.mob.is_no_ai() && goal_utils::has_ground_path_navigation(self, level) {
            let can_open_doors = level.is_raided(self.block_position());
            self.set_can_open_doors(can_open_doors);
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        raider::add_additional_save_data(self, out);
        if self.is_johnny {
            out.put_boolean(TAG_JOHNNY, true);
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        raider::read_additional_save_data(self, input);
        self.is_johnny = input.get_boolean_or(TAG_JOHNNY, false);
    }

    fn set_custom_name(&mut self, name: Option<Component>) {
        if !self.is_johnny && name.as_ref().is_some_and(|n| n.get_string() == "Johnny") {
            self.is_johnny = true;
        }
        self.base_mut().set_custom_name(name);
    }

    fn can_attack(&self, level: &ServerLevel, target: &dyn Entity) -> bool {
        abstract_illager::can_attack(self, level, target)
    }

    fn considers_entity_as_ally(&self, level: &ServerLevel, other: &dyn Entity) -> bool {
        abstract_illager::considers_entity_as_ally(self, level, other)
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let spawn_group_data = raider::finalize_spawn(self, level, difficulty, reason, group_data);
        self.set_can_open_doors(true);
        self.populate_default_equipment_slots(level.random(), &difficulty);
        let registries = level.registry_access().clone();
        self.populate_default_equipment_enchantments(&registries, level.random(), &difficulty);
        spawn_group_data
    }

    fn populate_default_equipment_slots(
        &mut self,
        _random: &mut dyn RandomSource,
        _difficulty: &DifficultyInstance,
    ) {
        if raider::get_current_raid(self).is_none() {
            mob::equipment::set_item_slot_spawn(
                self,
                EquipmentSlot::MainHand,
                ItemStack::new(Item::IRON_AXE),
            );
        }
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_VINDICATOR_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_VINDICATOR_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_VINDICATOR_HURT)
    }

    fn get_celebrate_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_VINDICATOR_CELEBRATE)
    }

    fn apply_raid_buffs(
        &mut self,
        level: &mut ServerLevel,
        wave: i32,
        _is_captain: bool,
        raid: &Raid,
    ) {
        let mut axe = ItemStack::new(Item::IRON_AXE);
        let should_enchant = self.base_mut().random.next_float() <= raid.get_enchant_odds();
        if should_enchant {
            let provider = if wave > raid.get_num_groups(Difficulty::Normal) {
                "raid/vindicator_post_wave_5"
            } else {
                "raid/vindicator"
            };
            let difficulty =
                ServerLevelAccessor::get_current_difficulty_at(&*level, self.block_position());
            let registries = level.registries.clone();
            enchant_item_from_provider(
                &mut axe,
                &registries,
                &Identifier::new("minecraft", provider).expect("valid id"),
                &difficulty,
                &mut self.base_mut().random,
            );
        }
        mob::equipment::set_item_slot(self, level, EquipmentSlot::MainHand, axe);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static VINDICATOR: EntityRegistration = EntityRegistration {
    entity_type: EntityType::VINDICATOR,
    create: |ty, id, registries| Box::new(Vindicator::new(ty, id, registries)),
};

/// Scenario-harness fields of a vindicator (order 41; the twin of the `Vindicator` entry of `EntityExtras.java`).
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static VINDICATOR_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 41,
    applies: |e| e.as_any().is::<Vindicator>(),
    fields: |e, _level, _ctx| {
        let vindicator = e.as_any().downcast_ref::<Vindicator>().expect("vindicator");
        vec![(
            "johnny".into(),
            if vindicator.is_johnny { "1" } else { "0" }.into(),
        )]
    },
};
