//! `net.minecraft.world.entity.schedule.Activity` and its registry (`BuiltInRegistries.ACTIVITY`).
//!
//! The villager schedules themselves (`Schedule.VILLAGER_DEFAULT` / `VILLAGER_BABY` in older versions) are the
//! `villager_activity` / `baby_villager_activity` environment attribute tracks of
//! `data/minecraft/timeline/villager_schedule.json`; [`crate::world::entity::ai::brain::Brain::update_activity_from_schedule`]
//! consumes the attribute value through a closure so this module stays independent of the attribute system.

use mc_core::ident::Identifier;
use mc_core::math::java::java_string_hash;

/// `Activity`: a named, registered constant. Equality is by name (vanilla `equals`), `hash_code` is the cached
/// `String.hashCode` of the name (it decides the `HashMap` iteration order inside [`crate::world::entity::ai::brain::Brain`]).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Activity {
    name: &'static str,
}

macro_rules! activities {
    ($($ident:ident = $name:literal;)*) => {
        impl Activity {
            $(pub const $ident: Activity = Activity { name: $name };)*
        }
        /// Registry order of `Activity` (the declaration order in vanilla).
        pub const ACTIVITIES: &[Activity] = &[$(Activity::$ident),*];
    };
}

activities! {
    CORE = "core";
    IDLE = "idle";
    WORK = "work";
    PLAY = "play";
    REST = "rest";
    MEET = "meet";
    PANIC = "panic";
    RAID = "raid";
    PRE_RAID = "pre_raid";
    HIDE = "hide";
    FIGHT = "fight";
    CELEBRATE = "celebrate";
    ADMIRE_ITEM = "admire_item";
    AVOID = "avoid";
    RIDE = "ride";
    PLAY_DEAD = "play_dead";
    LONG_JUMP = "long_jump";
    RAM = "ram";
    TONGUE = "tongue";
    SWIM = "swim";
    LAY_SPAWN = "lay_spawn";
    SNIFF = "sniff";
    INVESTIGATE = "investigate";
    ROAR = "roar";
    EMERGE = "emerge";
    DIG = "dig";
}

impl Activity {
    /// `getName()` (also `toString()`).
    pub fn get_name(self) -> &'static str {
        self.name
    }

    /// `hashCode()` = `name.hashCode()`.
    pub fn hash_code(self) -> i32 {
        java_string_hash(self.name)
    }

    /// `BuiltInRegistries.ACTIVITY.getValue(id)` (default namespace `minecraft`).
    pub fn by_id(id: &Identifier) -> Option<Activity> {
        if id.namespace() != "minecraft" {
            return None;
        }
        ACTIVITIES.iter().copied().find(|a| a.name == id.path())
    }

    /// Registry id (`minecraft:<name>`).
    pub fn id(self) -> Identifier {
        Identifier::with_default_namespace(self.name).expect("valid activity name")
    }
}

impl std::fmt::Display for Activity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}
