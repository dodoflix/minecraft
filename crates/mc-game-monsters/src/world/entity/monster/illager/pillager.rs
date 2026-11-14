//! `Pillager`: the crossbow illager (patrols, raids, outposts).

use super::super::crossbow_attack_mob::perform_crossbow_attack;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, SoundEvent};
use mc_game_ai::world::entity::ai::goal::{AvoidEntityGoal, RangedCrossbowAttackGoal};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::container::SimpleContainer;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::goal::look_at_player::LookAtType;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of_type, class_of_types,
};
use mc_game_core::world::entity::ai::goal::{FloatGoal, LookAtPlayerGoal, RandomStrollGoal};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::monster::illager::abstract_illager::{self, IllagerArmPose};
use mc_game_core::world::entity::monster::patrolling_monster::check_patrolling_monster_spawn_rules;
use mc_game_core::world::entity::npc::inventory_carrier::{self, InventoryCarrier};
use mc_game_core::world::entity::raid::raid::Raid;
use mc_game_core::world::entity::raid::raider::{self, RaiderData};
use mc_game_core::world::entity::scenario_dump::{
    SCENARIO_DUMP_EXTRA, ScenarioDumpExtra, item_full,
};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::item::enchantment_helper::enchant_item_from_provider;
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_world::level_storage::Difficulty;
use std::sync::Arc;

define_accessor!(IS_CHARGING_CROSSBOW: Boolean = Pillager);

const INVENTORY_SIZE: usize = 5;

/// The key of a `VanillaEnchantmentProviders` constant.
fn enchantment_provider(path: &str) -> Identifier {
    Identifier::new("minecraft", path).expect("valid id")
}

pub struct Pillager {
    pub mob: MobData,
    raider: RaiderData,
    inventory: SimpleContainer,
}

impl Pillager {
    /// `new Pillager(type, level)`: `registerGoals()` and the ground navigation.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            raider::define_synched_data(b);
            b.define(IS_CHARGING_CROSSBOW, false);
        });
        let mut pillager = Pillager {
            mob,
            raider: RaiderData::default(),
            inventory: SimpleContainer::new(INVENTORY_SIZE),
        };
        let navigation = StandardPathNavigation::ground(&pillager);
        pillager.mob.set_navigation(Box::new(navigation));
        pillager.register_goals();
        pillager
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        raider::register_goals(&mut self.mob);
        let float = FloatGoal::new(self);
        let look_at_player =
            LookAtPlayerGoal::with_probability(self, LookAtType::Player, 15.0, 1.0);
        let look_at_mob = LookAtPlayerGoal::new(
            self,
            LookAtType::Class(Arc::new(|e: &dyn Entity| e.mob().is_some())),
            15.0,
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
        sel.add_goal(2, Box::new(raider::HoldGroundAttackGoal::new(10.0)));
        sel.add_goal(3, Box::new(RangedCrossbowAttackGoal::new(1.0, 8.0)));
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
            Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)),
        );
        t.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::new(
                TargetType::Class(class_of_types(&[
                    EntityType::VILLAGER,
                    EntityType::WANDERING_TRADER,
                ])),
                false,
            )),
        );
        t.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::new(
                TargetType::Class(class_of_type(EntityType::IRON_GOLEM)),
                true,
            )),
        );
    }

    /// `isChargingCrossbow()`.
    pub fn is_charging_crossbow(&self) -> bool {
        self.base().entity_data.get(IS_CHARGING_CROSSBOW)
    }

    /// `getArmPose()`.
    pub fn get_arm_pose(&self) -> IllagerArmPose {
        if self.is_charging_crossbow() {
            IllagerArmPose::CrossbowCharge
        } else if mob::combat::is_holding(self, Item::CROSSBOW) {
            IllagerArmPose::CrossbowHold
        } else if self.mob.is_aggressive() {
            IllagerArmPose::Attacking
        } else {
            IllagerArmPose::Neutral
        }
    }

    /// `wantsItem(itemStack)`.
    fn wants_item(&self, level: &ServerLevel, item_stack: &ItemStack) -> bool {
        raider::has_active_raid(self, level) && item_stack.item() == Item::WHITE_BANNER
    }
}

impl InventoryCarrier for Pillager {
    fn get_inventory(&self) -> &SimpleContainer {
        &self.inventory
    }

    fn get_inventory_mut(&mut self) -> &mut SimpleContainer {
        &mut self.inventory
    }
}

impl Entity for Pillager {
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
        Some("minecraft:pillager")
    }

    fn can_use_non_melee_weapon(&self, item: &ItemStack) -> bool {
        item.item() == Item::CROSSBOW
    }

    fn get_preferred_weapon_type(&self) -> Option<&'static str> {
        Some("pillager_preferred_weapons")
    }

    fn set_charging_crossbow(&mut self, is_charging: bool) {
        self.base_mut()
            .entity_data
            .set(IS_CHARGING_CROSSBOW, is_charging);
    }

    fn on_crossbow_attack_performed(&mut self) {
        self.mob.living.no_action_time = 0;
    }

    fn perform_ranged_attack(&mut self, level: &mut ServerLevel, _target: EntityId, _power: f32) {
        // `performCrossbowAttack(this, 1.6F)`
        let uncertainty = self.ranged_attack_uncertainty(level);
        let target = self.get_target();
        perform_crossbow_attack(level, self, 1.6, uncertainty, target);
        self.on_crossbow_attack_performed();
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        raider::add_additional_save_data(self, out);
        inventory_carrier::write_inventory_to_tag(&self.inventory, out);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        raider::read_additional_save_data(self, level, input);
        inventory_carrier::read_inventory_from_tag(&mut self.inventory, input);
        self.set_can_pick_up_loot(true);
    }

    fn get_walk_target_value(&self, _pos: BlockPos, _level: &dyn LevelReader) -> f32 {
        0.0
    }

    fn get_max_spawn_cluster_size(&self) -> i32 {
        1
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
        self.populate_default_equipment_slots(level.random(), &difficulty);
        let registries = level.registry_access().clone();
        self.populate_default_equipment_enchantments(&registries, level.random(), &difficulty);
        raider::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn populate_default_equipment_slots(
        &mut self,
        _random: &mut dyn RandomSource,
        _difficulty: &DifficultyInstance,
    ) {
        mob::equipment::set_item_slot_spawn(
            self,
            EquipmentSlot::MainHand,
            ItemStack::new(Item::CROSSBOW),
        );
    }

    fn enchant_spawned_weapon(
        &mut self,
        registries: &RegistryAccess,
        random: &mut dyn RandomSource,
        difficulty: &DifficultyInstance,
    ) {
        mob::equipment::enchant_spawned_weapon(self, registries, random, difficulty);
        if random.next_int_bounded(300) == 0 {
            let mut weapon = self.get_item_by_slot(EquipmentSlot::MainHand);
            if weapon.item() == Item::CROSSBOW {
                enchant_item_from_provider(
                    &mut weapon,
                    registries,
                    &enchantment_provider("pillager_spawn_crossbow"),
                    difficulty,
                    random,
                );
                mob::equipment::set_item_slot_spawn(self, EquipmentSlot::MainHand, weapon);
            }
        }
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PILLAGER_AMBIENT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PILLAGER_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PILLAGER_HURT)
    }

    fn get_celebrate_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PILLAGER_CELEBRATE)
    }

    fn get_slot(&self, slot: i32) -> Option<ItemStack> {
        inventory_carrier::get_slot(self, slot)
    }

    fn set_slot(&mut self, level: &mut ServerLevel, slot: i32, stack: ItemStack) {
        inventory_carrier::set_slot(self, level, slot, stack);
    }

    fn pick_up_item(&mut self, level: &mut ServerLevel, item: EntityId) {
        let Some(item_stack) = level
            .get_entity(item)
            .and_then(|e| e.as_any().downcast_ref::<ItemEntity>())
            .map(|e| e.get_item())
        else {
            return;
        };
        if mc_data::tags::get::<Item>("banners").is_some_and(|t| t.contains(item_stack.item())) {
            raider::pick_up_item(self, level, item);
        } else if self.wants_item(level, &item_stack) {
            mc_game_core::world::entity::player::pickup::on_item_pickup_by_id(level, &*self, item);
            let remainder = self.inventory.add_item(&item_stack);
            level.with_entity(item, |level, entity| {
                if let Some(item_entity) = entity.as_any_mut().downcast_mut::<ItemEntity>() {
                    if remainder.is_empty() {
                        item_entity.discard_in_level(level);
                    } else {
                        // `itemStack.setCount(remainder.getCount())`
                        let mut stack = item_entity.get_item();
                        stack.set_count(remainder.count());
                        item_entity.set_item_in_place(stack);
                    }
                }
            });
        }
    }

    fn apply_raid_buffs(
        &mut self,
        level: &mut ServerLevel,
        wave: i32,
        _is_captain: bool,
        raid: &Raid,
    ) {
        let should_enchant = self.base_mut().random.next_float() <= raid.get_enchant_odds();
        if should_enchant {
            let mut crossbow = ItemStack::new(Item::CROSSBOW);
            let provider = if wave > raid.get_num_groups(Difficulty::Normal) {
                Some("raid/pillager_post_wave_5")
            } else if wave > raid.get_num_groups(Difficulty::Easy) {
                Some("raid/pillager_post_wave_3")
            } else {
                None
            };
            if let Some(provider) = provider {
                let difficulty =
                    ServerLevelAccessor::get_current_difficulty_at(&*level, self.block_position());
                let registries = level.registries.clone();
                enchant_item_from_provider(
                    &mut crossbow,
                    &registries,
                    &enchantment_provider(provider),
                    &difficulty,
                    &mut self.base_mut().random,
                );
                mob::equipment::set_item_slot(self, level, EquipmentSlot::MainHand, crossbow);
            }
        }
    }
}

mc_game_core::register_spawn_placement!(EntityType::PILLAGER, check_patrolling_monster_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static PILLAGER: EntityRegistration = EntityRegistration {
    entity_type: EntityType::PILLAGER,
    create: |ty, id, registries| Box::new(Pillager::new(ty, id, registries)),
};

/// Scenario-harness fields of a pillager (order 41; the twin of the `Pillager` entry of `EntityExtras.java`).
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static PILLAGER_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 41,
    applies: |e| e.as_any().is::<Pillager>(),
    fields: |e, _level, _ctx| {
        let pillager = e.as_any().downcast_ref::<Pillager>().expect("pillager");
        let inventory: Vec<String> = pillager
            .inventory
            .get_items()
            .iter()
            .enumerate()
            .filter(|(_, stack)| !stack.is_empty())
            .map(|(i, stack)| format!("{i}:{}", item_full(stack)))
            .collect();
        vec![
            (
                "charging".into(),
                if pillager.is_charging_crossbow() {
                    "1"
                } else {
                    "0"
                }
                .into(),
            ),
            (
                "inventory".into(),
                if inventory.is_empty() {
                    "-".into()
                } else {
                    inventory.join(";")
                },
            ),
        ]
    },
};
