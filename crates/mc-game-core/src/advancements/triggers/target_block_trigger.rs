//! `TargetBlockTrigger`: `target_hit`.

use super::prelude::*;

/// `TargetBlockTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct TargetBlockTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub signal_strength: Ints,
    pub projectile: Option<ConditionHolder>,
}

impl TriggerInstance for TargetBlockTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<TargetBlockTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(TargetBlockTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                signal_strength: Ints::codec().optional_field_of_default("signal_strength", Ints::any()),
                projectile: condition_holder_codec().optional_field_of("projectile"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for TargetBlockTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(TargetBlockTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(&validator.entity_context(), "projectile", &this.projectile);
});

crate::register_triggers!(TargetBlockTriggerInstance {
    TARGET_BLOCK_HIT = "target_hit",
});

impl TargetBlockTriggerInstance {
    /// `matches(projectile, hitPosition, signalStrength)`.
    pub fn matches(&self, projectile: &mut LootContext, _hit_position: Vec3, signal_strength: i32) -> bool {
        self.signal_strength.matches(signal_strength) && holder_matches(&self.projectile, projectile)
    }
}

impl Trigger<TargetBlockTriggerInstance> {
    /// `trigger(player, projectile, hitPosition, signalStrength)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, projectile: &dyn Entity, hit_position: Vec3, signal_strength: i32) {
        let matches = self.matches(&*level, &*player, projectile, hit_position, signal_strength);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(&self, level: &ServerLevel, player: &ServerPlayer, projectile: &dyn Entity, hit_position: Vec3, signal_strength: i32) -> TriggerMatches {
        let params = entity_params(level, player.position(), projectile);
        let mut context = new_context(&params);
        self.collect(level, player, |t| t.matches(&mut context, hit_position, signal_strength))
    }
}
