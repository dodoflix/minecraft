//! `AbstractPiglin` (`Piglin` and `PiglinBrute` share it): zombification in the overworld, door opening, malus, arm pose.
//! Its virtual methods are the [`AbstractPiglin`] trait; the non-virtual behavior is free functions over `&dyn Entity`.

use super::piglin::Piglin;
use super::piglin_ai;
use super::piglin_arm_pose::PiglinArmPose;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, MobEffect};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::attribute::attributes::PIGLINS_ZOMBIFY;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::mob::MobData;
use mc_game_core::world::entity::mob::convert::{self, ConversionParams};
use mc_game_core::world::level::pathfinder::PathType;
use mc_nbt::CompoundTag;
use mc_protocol::components::TOOL;
use mc_world::level_storage::Difficulty;

define_accessor!(pub DATA_IMMUNE_TO_ZOMBIFICATION: Boolean = AbstractPiglin);

/// `CONVERSION_TIME`.
pub const CONVERSION_TIME: i32 = 300;

/// The virtual methods of `AbstractPiglin`.
pub trait AbstractPiglin: Entity + Sized {
    /// `timeInOverworld`.
    fn time_in_overworld(&self) -> i32;
    fn set_time_in_overworld(&mut self, time_in_overworld: i32);
    /// `canHunt()`.
    fn can_hunt(&self) -> bool;
    /// `finishConversion(level)`.
    fn finish_conversion(&mut self, level: &mut ServerLevel);
    /// `playConvertedSound()`.
    fn play_converted_sound(&mut self, level: &mut ServerLevel);
    /// `getArmPose()`.
    fn get_arm_pose(&self) -> PiglinArmPose;
}

/// `AbstractPiglin.<init>` after `Monster.<init>`: the pick-up-loot flag and the fire maluses (the open-doors ability is
/// applied to the navigation by the constructor of the concrete class, see [`apply_open_doors_ability`]).
pub fn init(mob: &mut MobData) {
    mob.can_pick_up_loot = true;
    mob.set_pathfinding_malus(PathType::FireInNeighbor, 16.0);
    mob.set_pathfinding_malus(PathType::Fire, -1.0);
}

/// `applyOpenDoorsAbility()` for a freshly created ground navigation (`GoalUtils.hasGroundPathNavigation(this)`).
pub fn apply_open_doors_ability(
    navigation: &mut dyn mc_game_core::world::entity::ai::navigation::PathNavigation,
) {
    if navigation.can_navigate_ground() {
        navigation.set_can_open_doors(true);
    }
}

/// `isImmuneToZombification()`.
pub fn is_immune_to_zombification(e: &dyn Entity) -> bool {
    e.base().entity_data.get(DATA_IMMUNE_TO_ZOMBIFICATION)
}

/// `setImmuneToZombification(isImmuneToZombification)`.
pub fn set_immune_to_zombification(e: &mut dyn Entity, immune: bool) {
    e.base_mut()
        .entity_data
        .set(DATA_IMMUNE_TO_ZOMBIFICATION, immune);
}

/// `addAdditionalSaveData(output)` (after `Monster`'s).
pub fn add_additional_save_data(e: &dyn Entity, time_in_overworld: i32, out: &mut CompoundTag) {
    mc_game_core::world::entity::mob::nbt::add_additional_save_data(e, out);
    out.put_boolean("IsImmuneToZombification", is_immune_to_zombification(e));
    out.put_int("TimeInOverworld", time_in_overworld);
}

/// `readAdditionalSaveData(input)` (after `Monster`'s); returns `timeInOverworld`.
pub fn read_additional_save_data(
    e: &mut dyn Entity,
    level: Option<&ServerLevel>,
    input: &CompoundTag,
) -> i32 {
    mc_game_core::world::entity::mob::nbt::read_additional_save_data(e, level, input);
    e.set_can_pick_up_loot(input.get_boolean_or("CanPickUpLoot", true));
    set_immune_to_zombification(e, input.get_boolean_or("IsImmuneToZombification", false));
    input.get_int_or("TimeInOverworld", 0)
}

/// `isConverting()`: `!isImmuneToZombification() && !isNoAi() && environmentAttributes.getValue(PIGLINS_ZOMBIFY, position())`.
pub fn is_converting(e: &dyn Entity, level: &ServerLevel) -> bool {
    !is_immune_to_zombification(e)
        && !e.mob().is_some_and(MobData::is_no_ai)
        && level.attribute_bool_vec(PIGLINS_ZOMBIFY, &e.position())
}

/// `customServerAiStep(level)`.
pub fn custom_server_ai_step<T: AbstractPiglin>(this: &mut T, level: &mut ServerLevel) {
    // `super.customServerAiStep(level)`: nothing for a `Monster`
    if is_converting(&*this, level) {
        let t = this.time_in_overworld();
        this.set_time_in_overworld(t.wrapping_add(1));
    } else {
        this.set_time_in_overworld(0);
    }
    if this.time_in_overworld() > CONVERSION_TIME {
        if level.difficulty() != Difficulty::Peaceful {
            this.play_converted_sound(level);
        }
        this.finish_conversion(level);
    }
}

/// `finishConversion(level)` of `AbstractPiglin`: becomes a zombified piglin with nausea.
pub fn finish_conversion(this: &mut dyn Entity, level: &mut ServerLevel) {
    let params = ConversionParams::single(&*this, level, true, true);
    convert::convert_to(
        this,
        level,
        EntityType::ZOMBIFIED_PIGLIN,
        &params,
        EntitySpawnReason::Conversion,
        |zombified, level| {
            zombified.add_effect(
                level,
                MobEffectInstance::of(MobEffect::NAUSEA, 200, 0),
                None,
            );
        },
    );
}

/// `isAdult()`.
pub fn is_adult(e: &dyn Entity) -> bool {
    !e.is_baby()
}

/// `isHoldingMeleeWeapon()`: the main hand item has the `tool` component.
pub fn is_holding_melee_weapon(e: &dyn Entity) -> bool {
    e.get_item_by_slot(EquipmentSlot::MainHand)
        .get(&TOOL)
        .is_some()
}

/// `canHunt()` of any `AbstractPiglin` (`PiglinBrute` never hunts).
pub fn can_hunt_entity(e: &dyn Entity) -> bool {
    e.as_any()
        .downcast_ref::<Piglin>()
        .is_some_and(Piglin::can_hunt_piglin)
}

/// `AbstractPiglin.playAmbientSound()`.
pub fn play_ambient_sound(e: &mut dyn Entity, level: &mut ServerLevel) {
    if piglin_ai::is_idle(&*e) {
        mc_game_core::world::entity::mob::tick::play_ambient_sound(e, level);
    }
}
