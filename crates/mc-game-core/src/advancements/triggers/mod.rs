//! `net.minecraft.advancements.triggers`: `CriterionTrigger`s, their instances and the trigger registry.
//!
//! # Firing a trigger (for every subsystem)
//!
//! ```ignore
//! use crate::advancements::triggers::CriteriaTriggers;
//! CriteriaTriggers::CONSUME_ITEM.trigger(level, player, &stack);          // level: &mut ServerLevel, player: &mut ServerPlayer
//! let m = CriteriaTriggers::PLAYER_KILLED_ENTITY.matches(&*level, &*player, victim, &source); // borrows `level` shared
//! m.award(level, player);                                                    // ... then awards (rewards may mutate)
//! ```
//!
//! `X.trigger(level, player, args..)` is vanilla's `CriteriaTriggers.X.trigger(player, args..)`; `matches` + `award` is the
//! same split in two steps for callers whose arguments borrow from `level` (an entity found in the level's storage).
//! `matches` never mutates anything, `award` grants the matched advancement criteria (rewards, chat announcement).

use std::any::Any;
use std::marker::PhantomData;
use std::sync::{Arc, LazyLock};

use mc_core::phys::Vec3;
use mc_dfu::Codec;

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::player::ServerPlayer;
use crate::world::level::storage::loot::codec_ext::dispatch_optional_value;
use crate::world::level::storage::loot::parameters::LootContextParamSets;
use crate::world::level::storage::loot::predicates::ConditionHolder;
use crate::world::level::storage::loot::validation::{
    ValidationContextSource, validate_holder_opt,
};
use crate::world::level::storage::loot::{LootContext, LootParams};

use super::player_advancements::TriggerInstanceKey;

pub mod any_block_interaction_trigger;
pub mod bee_nest_destroyed_trigger;
pub mod bred_animals_trigger;
pub mod brewed_potion_trigger;
pub mod change_dimension_trigger;
pub mod channeled_lightning_trigger;
pub mod construct_beacon_trigger;
pub mod consume_item_trigger;
pub mod cured_zombie_villager_trigger;
pub mod default_block_interaction_trigger;
pub mod distance_trigger;
pub mod effects_changed_trigger;
pub mod enchanted_item_trigger;
pub mod enter_block_trigger;
pub mod entity_hurt_player_trigger;
pub mod fall_after_explosion_trigger;
pub mod filled_bucket_trigger;
pub mod fishing_rod_hooked_trigger;
pub mod impossible_trigger;
pub mod inventory_change_trigger;
pub mod item_durability_trigger;
pub mod item_used_on_location_trigger;
pub mod killed_by_arrow_trigger;
pub mod killed_trigger;
pub mod levitation_trigger;
pub mod lightning_strike_trigger;
pub mod loot_table_trigger;
pub mod picked_up_item_trigger;
pub mod player_hurt_entity_trigger;
pub mod player_interact_trigger;
pub mod player_trigger;
pub mod recipe_crafted_trigger;
pub mod recipe_unlocked_trigger;
pub mod shot_crossbow_trigger;
pub mod slide_down_block_trigger;
pub mod spear_mobs_trigger;
pub mod start_riding_trigger;
pub mod summoned_entity_trigger;
pub mod tame_animal_trigger;
pub mod target_block_trigger;
pub mod trade_trigger;
pub mod used_ender_eye_trigger;
pub mod used_totem_trigger;
pub mod using_item_trigger;

/// What the trigger modules need (`use super::prelude::*;`).
#[allow(unused_imports)]
pub(crate) mod prelude {
    pub use super::{
        CriteriaTriggers, CriterionTriggerInstance, SimpleInstance, Trigger, TriggerInstance,
        TriggerMatches, entity_params, holder_matches, new_context,
    };
    pub use crate::server::level::ServerLevel;
    pub use crate::world::damagesource::DamageSource;
    pub use crate::world::entity::Entity;
    pub use crate::world::entity::player::ServerPlayer;
    pub use crate::world::item::ItemStack;
    pub use crate::world::level::storage::loot::LootContext;
    pub use crate::world::level::storage::loot::predicates::{
        ConditionHolder, condition_holder_codec,
    };
    pub use mc_core::phys::Vec3;
    pub use mc_core::pos::BlockPos;
    pub use mc_data::block::BlockState;
    pub use mc_dfu::{BOOL, Codec, DataResult, FLOAT, INT, STRING};
    pub use mc_protocol::components::{Doubles, Ints, ItemPredicate};
}

/// Namespace of the trigger constants (`CriteriaTriggers.X`): every trigger module declares its own constants with
/// [`register_triggers!`]. Usage: `CriteriaTriggers::CONSUME_ITEM.trigger(level, player, &stack)`.
pub struct CriteriaTriggers;

/// The registered triggers (`BuiltInRegistries.TRIGGER_TYPES`), filled by [`register_triggers!`].
#[linkme::distributed_slice]
pub static TRIGGER_TYPES: [ErasedTrigger];

/// Declares the `CriteriaTriggers` constants of one instance type and registers their names:
/// `register_triggers!(KilledTriggerInstance { PLAYER_KILLED_ENTITY = "player_killed_entity", ... });`
#[macro_export]
macro_rules! register_triggers {
    ($inst:ty { $($name:ident = $id:literal),+ $(,)? }) => {
        impl $crate::advancements::triggers::CriteriaTriggers {
            $(pub const $name: $crate::advancements::triggers::Trigger<$inst> = $crate::advancements::triggers::Trigger::new($id);)+
        }
        $(
            const _: () = {
                #[linkme::distributed_slice($crate::advancements::triggers::TRIGGER_TYPES)]
                static REGISTRATION: $crate::advancements::triggers::ErasedTrigger =
                    $crate::advancements::triggers::ErasedTrigger::of::<$inst>($id);
            };
        )+
    };
}

/// `CriterionTriggerInstance`.
pub trait CriterionTriggerInstance: Any + Send + Sync {
    /// `validate(validator)`.
    fn validate(&self, validator: &ValidationContextSource);
    fn as_any(&self) -> &dyn Any;
}

/// The instance type of a `CriterionTrigger<T>`: its codec.
pub trait TriggerInstance: CriterionTriggerInstance + Clone + Sized {
    /// `CriterionTrigger.codec()`.
    fn codec() -> Codec<Self>;
}

/// `SimpleCriterionTrigger.SimpleInstance`: an instance with the optional `player` condition.
pub trait SimpleInstance: TriggerInstance {
    fn player(&self) -> &Option<ConditionHolder>;

    /// The default `validate` of `SimpleInstance` (`Validatable.validateHolder(entityContext, "player", player)`).
    fn validate_player(&self, validator: &ValidationContextSource) {
        validate_holder_opt(&validator.entity_context(), "player", self.player());
    }
}

/// Implements `CriterionTriggerInstance` for a `SimpleInstance` whose `validate` is the default, or with extra checks:
/// `simple_instance!(Type)` / `simple_instance!(Type, |this, validator| { ... })`.
#[macro_export]
macro_rules! simple_instance {
    ($ty:ty) => {
        impl $crate::advancements::triggers::CriterionTriggerInstance for $ty {
            fn validate(&self, validator: &$crate::world::level::storage::loot::validation::ValidationContextSource) {
                $crate::advancements::triggers::SimpleInstance::validate_player(self, validator);
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
    };
    ($ty:ty, |$this:ident, $validator:ident| $extra:block) => {
        impl $crate::advancements::triggers::CriterionTriggerInstance for $ty {
            fn validate(&self, $validator: &$crate::world::level::storage::loot::validation::ValidationContextSource) {
                $crate::advancements::triggers::SimpleInstance::validate_player(self, $validator);
                let $this = self;
                $extra
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
    };
}

/// A registered `CriterionTrigger<I>` (`BuiltInRegistries.TRIGGER_TYPES` entry): identified by its registry name.
pub struct Trigger<I> {
    pub name: &'static str,
    _instance: PhantomData<fn() -> I>,
}

impl<I> Trigger<I> {
    pub const fn new(name: &'static str) -> Trigger<I> {
        Trigger {
            name,
            _instance: PhantomData,
        }
    }
}

impl<I: TriggerInstance> Trigger<I> {
    /// `codec()`.
    pub fn codec(&self) -> Codec<I> {
        I::codec()
    }

    /// `createCriterion(instance)`.
    pub fn create_criterion(&self, instance: I) -> Criterion {
        Criterion {
            trigger: self.name,
            trigger_instance: Arc::new(instance),
        }
    }
}

impl<I: SimpleInstance> Trigger<I> {
    /// `SimpleCriterionTrigger.trigger(player, matcher)` up to awarding: the active listeners of `player` for this
    /// trigger whose instance satisfies `matcher` and whose `player` condition passes.
    pub fn collect(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        mut matcher: impl FnMut(&I) -> bool,
    ) -> TriggerMatches {
        let listeners = player.advancements.listeners_for(self.name);
        if listeners.is_empty() {
            return TriggerMatches(Vec::new());
        }
        let params = entity_params(level, player.position(), player);
        let mut player_context = new_context(&params);
        let mut matched = Vec::new();
        for (key, instance) in listeners {
            let Some(value) = instance.as_any().downcast_ref::<I>() else {
                continue;
            };
            if matcher(value) && holder_matches(value.player(), &mut player_context) {
                matched.push(key);
            }
        }
        TriggerMatches(matched)
    }

    /// `trigger(player, matcher)`: collect then award.
    pub fn trigger_with(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        matcher: impl FnMut(&I) -> bool,
    ) {
        self.collect(level, player, matcher).award(level, player);
    }
}

/// The criteria a trigger firing matched (`matchedConditions`), not yet awarded.
#[must_use = "call .award(level, player)"]
pub struct TriggerMatches(Vec<TriggerInstanceKey>);

impl TriggerMatches {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `advancements.award(advancement, criterion)` for each match, in order.
    pub fn award(self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        let advancements = player.advancements.clone();
        for key in self.0 {
            advancements.award(level, player, &key.advancement, &key.criterion);
        }
    }
}

/// `Criterion<T>`: a trigger (by registry name) with its instance.
#[derive(Clone)]
pub struct Criterion {
    pub trigger: &'static str,
    pub trigger_instance: Arc<dyn CriterionTriggerInstance>,
}

/// A registry entry of [`TRIGGER_TYPES`].
pub struct ErasedTrigger {
    pub name: &'static str,
    pub codec: fn() -> Codec<Arc<dyn CriterionTriggerInstance>>,
}

impl ErasedTrigger {
    pub const fn of<I: TriggerInstance>(name: &'static str) -> ErasedTrigger {
        ErasedTrigger {
            name,
            codec: erased_codec::<I>,
        }
    }
}

pub fn erased_codec<I: TriggerInstance>() -> Codec<Arc<dyn CriterionTriggerInstance>> {
    I::codec().xmap(
        |i: I| Arc::new(i) as Arc<dyn CriterionTriggerInstance>,
        |a: &Arc<dyn CriterionTriggerInstance>| {
            a.as_any()
                .downcast_ref::<I>()
                .expect("criterion instance matches its trigger")
                .clone()
        },
    )
}

impl Criterion {
    /// `Criterion.CODEC`: `{"trigger": <id>, "conditions": {...}}` (conditions optional).
    pub fn codec() -> Codec<Criterion> {
        static C: LazyLock<Codec<Criterion>> = LazyLock::new(|| {
            dispatch_optional_value(
                "trigger",
                "conditions",
                trigger_type_codec(),
                |c: &Criterion| c.trigger,
                |name: &&'static str| {
                    let entry = TRIGGER_TYPES
                        .iter()
                        .find(|t| t.name == *name)
                        .expect("registered trigger");
                    let name: &'static str = entry.name;
                    mc_dfu::DataResult::success((entry.codec)().xmap(
                        move |instance| Criterion {
                            trigger: name,
                            trigger_instance: instance,
                        },
                        |c: &Criterion| c.trigger_instance.clone(),
                    ))
                },
            )
            .codec()
        });
        C.clone()
    }
}

/// `CriteriaTriggers.CODEC` (`TRIGGER_TYPES.byNameCodec()`): the registry name (`minecraft` namespace).
fn trigger_type_codec() -> Codec<&'static str> {
    use mc_core::ident::Identifier;
    use mc_dfu::extra::IDENTIFIER;
    IDENTIFIER.flat_xmap(
        |id: Identifier| match TRIGGER_TYPES.iter().find(|t| id.namespace() == "minecraft" && id.path() == t.name) {
            Some(t) => mc_dfu::DataResult::success(t.name),
            None => {
                let m = format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:trigger_type]: {id}");
                mc_dfu::DataResult::error(move || m.clone())
            }
        },
        |name: &&'static str| mc_dfu::DataResult::success(Identifier::new("minecraft", name).expect("valid trigger name")),
    )
}

/// `EntityPredicate.createContext(player, entity)` parameters: `entity` as `this_entity`, the player's position as origin.
pub fn entity_params<'a>(
    level: &'a ServerLevel,
    player_position: Vec3,
    entity: &'a dyn Entity,
) -> LootParams<'a> {
    LootParams::builder(level)
        .this_entity(entity)
        .origin(player_position)
        .create(&LootContextParamSets::ADVANCEMENT_ENTITY)
}

/// `new LootContext.Builder(params).create(Optional.empty())`.
pub fn new_context<'a>(params: &'a LootParams<'a>) -> LootContext<'a> {
    LootContext::builder(params).create(None)
}

/// `holder.isEmpty() || holder.get().value().test(context)`.
pub fn holder_matches(holder: &Option<ConditionHolder>, context: &mut LootContext) -> bool {
    match holder {
        None => true,
        Some(h) => h.value(context.get_resolver()).test(context),
    }
}

/// Fires a trigger of player `player_id` (needs the player mutably to award) that also reads `entity`, which is out of
/// the level because it is the running entity (a victim being hurt, a projectile ticking). `matcher` is `X.matches(..)`.
/// When the player is not in the level because it is the running entity (a melee attacker), the firing runs when the player
/// is put back and `entity` is looked up by id then.
pub fn fire_with_entity<F>(
    level: &mut ServerLevel,
    player_id: crate::world::entity::EntityId,
    entity: &dyn Entity,
    matcher: F,
) where
    F: Fn(&ServerLevel, &ServerPlayer, &dyn Entity) -> TriggerMatches + 'static,
{
    let fired = level.with_player(player_id, |level, player| {
        let matches = matcher(&*level, &*player, entity);
        matches.award(level, player);
    });
    if fired.is_none() {
        let entity_id = entity.id();
        level.with_entity_or_defer(player_id, move |level, running| {
            let Some(player) = running.as_any_mut().downcast_mut::<ServerPlayer>() else {
                return;
            };
            let Some(entity) = level.get_entity(entity_id) else {
                return;
            };
            let matches = matcher(&*level, &*player, entity);
            matches.award(level, player);
        });
    }
}
