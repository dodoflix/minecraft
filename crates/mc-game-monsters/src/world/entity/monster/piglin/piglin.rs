//! `Piglin`.

use super::abstract_piglin::{self, AbstractPiglin, DATA_IMMUNE_TO_ZOMBIFICATION};
use super::piglin_ai::{self, BARTERING_ITEM};
use super::piglin_arm_pose::PiglinArmPose;
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::container::SimpleContainer;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::brain::{Brain, Provider};
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATE_RECENTLY, SPEAR_CHARGE_POSITION, SPEAR_ENGAGE_TIME, SPEAR_FLEEING_POSITION, SPEAR_FLEEING_TIME, UNIVERSAL_ANGER};
use mc_game_core::world::entity::ai::memory::memory_module_type::ATTACK_TARGET;
use mc_game_core::world::entity::ai::memory::{BrainPacked, MemoryTypeId};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::sensing::sensor_type::{HURT_BY, NEAREST_ITEMS, NEAREST_LIVING_ENTITIES, NEAREST_PLAYERS, PIGLIN_SPECIFIC_SENSOR};
use mc_game_core::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::damage::make_sound;
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use crate::world::entity::monster::crossbow_attack_mob::perform_crossbow_attack;
use mc_game_core::world::entity::npc::inventory_carrier::{self, InventoryCarrier};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::crossbow_item;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::KINETIC_WEAPON;
use std::sync::LazyLock;

define_accessor!(pub DATA_BABY_ID: Boolean = Piglin);
define_accessor!(DATA_IS_CHARGING_CROSSBOW: Boolean = Piglin);
define_accessor!(DATA_IS_DANCING: Boolean = Piglin);

const MAX_PASSENGERS_ON_ONE_HOGLIN: i32 = 3;
const PROBABILITY_OF_SPAWNING_AS_BABY: f32 = 0.2;
const PROBABILITY_OF_SPAWNING_WITH_CROSSBOW_INSTEAD_OF_SWORD: f64 = 0.5;
const CHANCE_OF_WEARING_EACH_ARMOUR_ITEM: f32 = 0.1;
const INVENTORY_SLOT_OFFSET: i32 = 300;
const INVENTORY_SIZE: usize = 8;

/// `SPEED_MODIFIER_BABY`.
static SPEED_MODIFIER_BABY_ID: LazyLock<Identifier> = LazyLock::new(|| Identifier::with_default_namespace("baby").expect("valid id"));

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    EntityDimensions::scalable(0.49, 0.98).with_eye_height(0.78).with_attachments(EntityAttachments::create_default(0.49, 0.98).with_vehicle(Vec3::new(0.0, f64::from(0.1875f32), 0.0)))
}

static BRAIN_MEMORIES: [MemoryTypeId; 6] = [UNIVERSAL_ANGER.id(), ATE_RECENTLY.id(), SPEAR_FLEEING_TIME.id(), SPEAR_FLEEING_POSITION.id(), SPEAR_CHARGE_POSITION.id(), SPEAR_ENGAGE_TIME.id()];

/// `BRAIN_PROVIDER`.
static BRAIN_PROVIDER: Provider = Provider::with_memories(&BRAIN_MEMORIES, &[&NEAREST_LIVING_ENTITIES, &NEAREST_PLAYERS, &NEAREST_ITEMS, &HURT_BY, &PIGLIN_SPECIFIC_SENSOR], piglin_ai::get_activities);

pub struct Piglin {
    pub mob: MobData,
    time_in_overworld: i32,
    inventory: SimpleContainer,
    cannot_hunt: bool,
}

impl Piglin {
    /// `new Piglin(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_IMMUNE_TO_ZOMBIFICATION, false);
            b.define(DATA_BABY_ID, false);
            b.define(DATA_IS_CHARGING_CROSSBOW, false);
            b.define(DATA_IS_DANCING, false);
        });
        let mut piglin = Piglin { mob, time_in_overworld: 0, inventory: SimpleContainer::new(INVENTORY_SIZE), cannot_hunt: false };
        // `LivingEntity.<init>`: `this.brain = this.makeBrain(Brain.Packed.EMPTY)`
        let brain = piglin.make_brain(&BrainPacked::EMPTY);
        piglin.mob.living.brain = brain;
        // `Mob.<init>`: `createNavigation`, then `AbstractPiglin.<init>`
        let mut navigation = StandardPathNavigation::ground(&piglin);
        abstract_piglin::apply_open_doors_ability(&mut navigation);
        piglin.mob.set_navigation(Box::new(navigation));
        abstract_piglin::init(&mut piglin.mob);
        piglin.mob.xp_reward = 5;
        piglin
    }

    /// `canHunt()`.
    pub fn can_hunt_piglin(&self) -> bool {
        !self.cannot_hunt
    }

    fn set_cannot_hunt(&mut self, cannot_hunt: bool) {
        self.cannot_hunt = cannot_hunt;
    }

    /// `isConverting()`.
    pub fn is_converting(&self, level: &ServerLevel) -> bool {
        abstract_piglin::is_converting(self, level)
    }

    /// `isDancing()`.
    pub fn is_dancing(&self) -> bool {
        self.base().entity_data.get(DATA_IS_DANCING)
    }

    /// `setDancing(dancing)`.
    pub fn set_dancing(&mut self, dancing: bool) {
        self.base_mut().entity_data.set(DATA_IS_DANCING, dancing);
    }

    fn is_charging_crossbow(&self) -> bool {
        self.base().entity_data.get(DATA_IS_CHARGING_CROSSBOW)
    }

    /// `addToInventory(itemStack)`: the part that did not fit.
    pub fn add_to_inventory(&mut self, item_stack: &ItemStack) -> ItemStack {
        self.inventory.add_item(item_stack)
    }

    /// `canAddToInventory(itemStack)`.
    pub fn can_add_to_inventory(&self, item_stack: &ItemStack) -> bool {
        self.inventory.can_add_item(item_stack)
    }

    /// `canReplaceCurrentItem(newItemStack)`.
    pub fn can_replace_current_item_stack(&self, level: &ServerLevel, new_item_stack: &ItemStack) -> bool {
        let slot = self.get_equipment_slot_for_item(new_item_stack);
        let current = self.get_item_by_slot(slot);
        self.can_replace_current_item(level, new_item_stack, &current, slot)
    }

    /// `holdInMainHand(itemStack)`.
    pub fn hold_in_main_hand(&mut self, level: &mut ServerLevel, item_stack: ItemStack) {
        mob::equipment::set_item_slot_and_drop_when_killed(self, level, EquipmentSlot::MainHand, item_stack);
        self.set_persistence_required();
    }

    /// `holdInOffHand(itemStack)`.
    pub fn hold_in_off_hand(&mut self, level: &mut ServerLevel, item_stack: ItemStack) {
        let is_bartering_item = item_stack.item() == BARTERING_ITEM;
        mob::equipment::set_item_slot_and_drop_when_killed(self, level, EquipmentSlot::OffHand, item_stack);
        if !is_bartering_item {
            self.set_persistence_required();
        }
    }

    /// `createSpawnWeapon()`: draws from the piglin's own random.
    fn create_spawn_weapon(&mut self) -> ItemStack {
        let random = &mut self.base_mut().random;
        if f64::from(random.next_float()) < PROBABILITY_OF_SPAWNING_WITH_CROSSBOW_INSTEAD_OF_SWORD {
            ItemStack::new(Item::CROSSBOW)
        } else {
            ItemStack::new(if random.next_int_bounded(10) == 0 { Item::GOLDEN_SPEAR } else { Item::GOLDEN_SWORD })
        }
    }

    /// `maybeWearArmor(slot, itemStack, random)` (spawn time: `setItemSlot` before the first tick).
    fn maybe_wear_armor(&mut self, slot: EquipmentSlot, item_stack: ItemStack, random: &mut dyn RandomSource) {
        if random.next_float() < CHANCE_OF_WEARING_EACH_ARMOUR_ITEM {
            mob::equipment::set_item_slot_spawn(self, slot, item_stack);
        }
    }

    /// `getArmPose()` (`PiglinArmPose`, client-side model state).
    pub fn get_piglin_arm_pose(&self) -> PiglinArmPose {
        self.get_arm_pose()
    }

    /// `getTopPassenger(vehicle, counter)`.
    fn get_top_passenger(level: &ServerLevel, vehicle: EntityId, counter: i32) -> EntityId {
        let first = level.get_entity(vehicle).and_then(|v| v.base().passengers.first().copied());
        match first {
            Some(passenger) if counter != 1 => Self::get_top_passenger(level, passenger, counter - 1),
            _ => vehicle,
        }
    }
}

impl AbstractPiglin for Piglin {
    fn time_in_overworld(&self) -> i32 {
        self.time_in_overworld
    }

    fn set_time_in_overworld(&mut self, time_in_overworld: i32) {
        self.time_in_overworld = time_in_overworld;
    }

    fn can_hunt(&self) -> bool {
        self.can_hunt_piglin()
    }

    fn finish_conversion(&mut self, level: &mut ServerLevel) {
        piglin_ai::cancel_admiring(level, self);
        for item_stack in self.inventory.remove_all_items() {
            mc_game_core::world::entity::item::spawn_at_location(level, &*self, item_stack, Vec3::ZERO);
        }
        abstract_piglin::finish_conversion(self, level);
    }

    fn play_converted_sound(&mut self, level: &mut ServerLevel) {
        make_sound(self, level, SoundEvent::ENTITY_PIGLIN_CONVERTED_TO_ZOMBIFIED);
    }

    fn get_arm_pose(&self) -> PiglinArmPose {
        if self.is_dancing() {
            PiglinArmPose::Dancing
        } else if piglin_ai_is_loved(&self.get_item_by_slot(EquipmentSlot::OffHand)) {
            PiglinArmPose::AdmiringItem
        } else if self.mob.is_aggressive() && abstract_piglin::is_holding_melee_weapon(self) {
            PiglinArmPose::AttackingWithMeleeWeapon
        } else if self.is_charging_crossbow() {
            PiglinArmPose::CrossbowCharge
        } else {
            let main = self.get_item_by_slot(EquipmentSlot::MainHand);
            let off = self.get_item_by_slot(EquipmentSlot::OffHand);
            let holding_crossbow = main.item() == Item::CROSSBOW || off.item() == Item::CROSSBOW;
            // `getWeaponItem()`: the main hand
            if holding_crossbow && crossbow_item::is_charged(&main) { PiglinArmPose::CrossbowHold } else { PiglinArmPose::Default }
        }
    }
}

fn piglin_ai_is_loved(stack: &ItemStack) -> bool {
    mc_game_core::world::entity::ai::sensing::repellent_sensors::is_loved_item(stack)
}

impl InventoryCarrier for Piglin {
    fn get_inventory(&self) -> &SimpleContainer {
        &self.inventory
    }

    fn get_inventory_mut(&mut self) -> &mut SimpleContainer {
        &mut self.inventory
    }
}

impl Entity for Piglin {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:piglin")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() { baby_dimensions() } else { EntityDimensions::of_type(self.mob.living.entity.entity_type) }
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_BABY_ID.id() {
            self.refresh_dimensions();
        }
    }

    fn make_brain(&mut self, packed: &BrainPacked) -> Brain {
        BRAIN_PROVIDER.make_brain(self, packed)
    }

    fn is_baby(&self) -> bool {
        self.base().entity_data.get(DATA_BABY_ID)
    }

    /// `setBaby(baby)`.
    fn set_baby(&mut self, baby: bool) {
        self.base_mut().entity_data.set(DATA_BABY_ID, baby);
        let living = &mut self.mob.living;
        if let Some(speed) = living.attributes.get_instance(Attribute::MOVEMENT_SPEED) {
            speed.remove_modifier(&SPEED_MODIFIER_BABY_ID);
            if baby {
                speed.add_transient_modifier(AttributeModifier {
                    id: SPEED_MODIFIER_BABY_ID.clone(),
                    amount: f64::from(0.2f32),
                    operation: AttributeModifierOperation::AddMultipliedBase,
                });
            }
        }
    }

    fn get_target(&self) -> Option<EntityId> {
        // `getTargetFromBrain()`
        self.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_piglin::add_additional_save_data(self, self.time_in_overworld, out);
        out.put_boolean("IsBaby", self.is_baby());
        out.put_boolean("CannotHunt", self.cannot_hunt);
        inventory_carrier::write_inventory_to_tag(&self.inventory, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.time_in_overworld = abstract_piglin::read_additional_save_data(self, input);
        self.set_baby(input.get_boolean_or("IsBaby", false));
        self.set_cannot_hunt(input.get_boolean_or("CannotHunt", false));
        inventory_carrier::read_inventory_from_tag(&mut self.inventory, input);
    }

    fn drop_custom_death_loot(&mut self, level: &mut ServerLevel, source: &DamageSource, killed_by_player: bool) {
        mob::equipment::drop_custom_death_loot(self, level, source, killed_by_player);
        for item_stack in self.inventory.remove_all_items() {
            mc_game_core::world::entity::item::spawn_at_location(level, &*self, item_stack, Vec3::ZERO);
        }
    }

    fn get_slot(&self, slot: i32) -> Option<ItemStack> {
        let inventory_slot = slot - INVENTORY_SLOT_OFFSET;
        if inventory_slot >= 0 && (inventory_slot as usize) < self.inventory.get_items().len() {
            Some(self.inventory.get_items()[inventory_slot as usize].clone())
        } else {
            mc_game_core::advancements::predicates::entity::support::get_slot(self.as_dyn(), slot)
        }
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        if reason != EntitySpawnReason::Structure {
            if level.random().next_float() < PROBABILITY_OF_SPAWNING_AS_BABY {
                self.set_baby(true);
            } else if !self.is_baby() {
                let weapon = self.create_spawn_weapon();
                mob::equipment::set_item_slot_spawn(self, EquipmentSlot::MainHand, weapon);
            }
        }
        piglin_ai::init_memories(self, level.random());
        self.populate_default_equipment_slots(level.random(), &difficulty);
        let registries = level.registry_access().clone();
        self.populate_default_equipment_enchantments(&registries, level.random(), &difficulty);
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn populate_default_equipment_slots(&mut self, random: &mut dyn RandomSource, _difficulty: &DifficultyInstance) {
        if !self.is_baby() {
            self.maybe_wear_armor(EquipmentSlot::Head, ItemStack::new(Item::GOLDEN_HELMET), random);
            self.maybe_wear_armor(EquipmentSlot::Chest, ItemStack::new(Item::GOLDEN_CHESTPLATE), random);
            self.maybe_wear_armor(EquipmentSlot::Legs, ItemStack::new(Item::GOLDEN_LEGGINGS), random);
            self.maybe_wear_armor(EquipmentSlot::Feet, ItemStack::new(Item::GOLDEN_BOOTS), random);
        }
    }

    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut mc_game_core::world::entity::ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let interaction_result = mob::interact::mob_interact(self, level, player, hand);
        if interaction_result.consumes_action() {
            return interaction_result;
        }
        piglin_ai::mob_interact(level, self, player, hand)
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        Brain::tick(level, self);
        piglin_ai::update_activity(level, self);
        abstract_piglin::custom_server_ai_step(self, level);
    }

    fn get_base_experience_reward(&mut self, _level: &ServerLevel) -> i32 {
        self.mob.xp_reward
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        let was_hurt = mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, damage);
        if was_hurt && source.causing_is_living() {
            mob::ext::with_causing_entity(level, source, |level, attacker| piglin_ai::was_hurt_by(level, self, attacker));
        }
        was_hurt
    }

    fn can_use_non_melee_weapon(&self, item: &ItemStack) -> bool {
        item.item() == Item::CROSSBOW || item.get(&KINETIC_WEAPON).is_some()
    }

    fn get_preferred_weapon_type(&self) -> Option<&'static str> {
        if self.is_baby() { None } else { Some("piglin_preferred_weapons") }
    }

    fn wants_to_pick_up(&self, level: &ServerLevel, item_stack: &ItemStack) -> bool {
        level.mob_griefing() && self.can_pick_up_loot() && piglin_ai::wants_to_pickup(level, self, item_stack)
    }

    fn can_replace_current_item(&self, level: &ServerLevel, new_item_stack: &ItemStack, current_item_stack: &ItemStack, slot: EquipmentSlot) -> bool {
        use mc_game_core::world::item::enchantment::enchantment_effect_components::EffectComponent;
        if mc_game_core::world::item::enchantment_helper::has(&level.registries, current_item_stack, EffectComponent::prevent_armor_change) {
            return false;
        }
        let preferred = self.get_preferred_weapon_type();
        let tag = |stack: &ItemStack, tag: &str| mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()));
        let wanted = |stack: &ItemStack| piglin_ai_is_loved(stack) || preferred.is_some_and(|p| tag(stack, p));
        let new_item_wanted = wanted(new_item_stack);
        let current_item_wanted = wanted(current_item_stack);
        if new_item_wanted && !current_item_wanted {
            true
        } else if !new_item_wanted && current_item_wanted {
            false
        } else {
            mob::equipment::can_replace_current_item(self, level, new_item_stack, current_item_stack, slot)
        }
    }

    fn pick_up_item(&mut self, level: &mut ServerLevel, item: EntityId) {
        mc_game_core::world::entity::player::pickup::on_item_pickup_by_id(level, &*self, item);
        piglin_ai::pick_up_item(level, self, item);
    }

    fn adjust_vehicle_to_ride(&self, level: &ServerLevel, vehicle: EntityId) -> EntityId {
        if self.is_baby() && level.get_entity(vehicle).is_some_and(|v| v.get_type() == EntityType::HOGLIN) {
            Self::get_top_passenger(level, vehicle, MAX_PASSENGERS_ON_ONE_HOGLIN)
        } else {
            vehicle
        }
    }

    fn play_ambient_sound(&mut self, level: &mut ServerLevel) {
        // `AbstractPiglin.playAmbientSound`: only while idle; the sound comes from `PiglinAi.getSoundForCurrentActivity`
        // (needs the level to resolve the avoid target, so it is chosen here instead of in `get_ambient_sound`)
        if piglin_ai::is_idle(self) {
            if let Some(sound) = piglin_ai::get_sound_for_current_activity(level, self) {
                make_sound(self, level, sound);
            }
        }
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PIGLIN_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_PIGLIN_DEATH)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_PIGLIN_STEP, 0.15, 1.0);
    }

    fn perform_ranged_attack(&mut self, level: &mut ServerLevel, _target: EntityId, _power: f32) {
        // `performCrossbowAttack(this, 1.6F)`
        let uncertainty = self.ranged_attack_uncertainty(level);
        let target = self.get_target();
        perform_crossbow_attack(level, self, 1.6, uncertainty, target);
        self.on_crossbow_attack_performed();
    }

    fn set_charging_crossbow(&mut self, is_charging: bool) {
        self.base_mut().entity_data.set(DATA_IS_CHARGING_CROSSBOW, is_charging);
    }

    fn on_crossbow_attack_performed(&mut self) {
        self.mob.living.no_action_time = 0;
    }
}

/// `Piglin.checkPiglinSpawnRules(type, level, spawnReason, pos, random)`.
fn check_piglin_spawn_rules(_ty: EntityType, level: &dyn ServerLevelAccessor, _reason: EntitySpawnReason, pos: BlockPos, _random: &mut dyn RandomSource) -> bool {
    use mc_game_core::world::level::BlockGetter;
    level.get_block_state(pos.below()).block() != Block::NETHER_WART_BLOCK
}

mc_game_core::register_spawn_placement!(EntityType::PIGLIN, check_piglin_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static PIGLIN: EntityRegistration = EntityRegistration { entity_type: EntityType::PIGLIN, create: |ty, id, registries| Box::new(Piglin::new(ty, id, registries)) };
