//! The `AbstractHorse` surface used by the generic goals (`RandomStandGoal`, `RunAroundLikeCrazyGoal`, ...). Horse-like mobs
//! (horse, donkey, mule, skeleton/zombie horse, llama, camel...: whichever extend `AbstractHorse` in vanilla) implement it
//! and expose it through `Entity::as_abstract_horse{,_mut}`.

use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use mc_data::SoundEvent;

pub trait AbstractHorseAccess {
    /// `isMobControlled()`: the controlling passenger is a `Mob`.
    fn is_mob_controlled(&self, level: &ServerLevel) -> bool;
    /// `isTamed()`.
    fn is_tamed(&self) -> bool;
    /// `standIfPossible()`.
    fn stand_if_possible(&mut self, level: &mut ServerLevel);
    /// `getAmbientStandSound()`.
    fn get_ambient_stand_sound(&self) -> Option<SoundEvent>;
    /// `getAmbientStandInterval()`.
    fn get_ambient_stand_interval(&self) -> i32;
    /// `getTemper()`.
    fn get_temper(&self) -> i32;
    /// `getMaxTemper()`.
    fn get_max_temper(&self) -> i32;
    /// `modifyTemper(amount)`.
    fn modify_temper(&mut self, amount: i32) -> i32;
    /// `tameWithName(player)`.
    fn tame_with_name(&mut self, level: &mut ServerLevel, player: EntityId);
    /// `makeMad()`.
    fn make_mad(&mut self, level: &mut ServerLevel);
    /// `isBred()`.
    fn is_bred(&self) -> bool;
    /// `canParent()`.
    fn can_parent(&self) -> bool;
    /// `setTamed(flag)`.
    fn set_tamed(&mut self, flag: bool);
    /// `getInventoryColumns()`.
    fn get_inventory_columns(&self) -> i32;
}
