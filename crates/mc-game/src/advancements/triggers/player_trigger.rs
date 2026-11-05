//! `net.minecraft.advancements.triggers.PlayerTrigger`: `location`, `slept_in_bed`, `tick`, `hero_of_the_village`,
//! `voluntary_exile`, `avoid_vibration`.

use mc_dfu::Codec;

use super::{SimpleInstance, Trigger, TriggerInstance};
use crate::server::level::ServerLevel;
use crate::world::entity::player::ServerPlayer;
use crate::world::level::storage::loot::predicates::{ConditionHolder, condition_holder_codec};

/// `PlayerTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct PlayerTriggerInstance {
    pub player: Option<ConditionHolder>,
}

impl TriggerInstance for PlayerTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<PlayerTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(PlayerTriggerInstance { player: condition_holder_codec().optional_field_of("player") }).codec()
        });
        C.clone()
    }
}

impl SimpleInstance for PlayerTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(PlayerTriggerInstance);

crate::register_triggers!(PlayerTriggerInstance {
    LOCATION = "location",
    SLEPT_IN_BED = "slept_in_bed",
    TICK = "tick",
    RAID_WIN = "hero_of_the_village",
    RAID_OMEN = "voluntary_exile",
    AVOID_VIBRATION = "avoid_vibration",
});

impl Trigger<PlayerTriggerInstance> {
    /// `trigger(player)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        self.trigger_with(level, player, |_| true);
    }
}
