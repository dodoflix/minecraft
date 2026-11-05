//! `LootContextParamSets` (the `context_key_set` registry) and its codec.

use mc_dfu::{Codec, DataResult};

use super::context::ContextKey::*;
use super::context::ContextKeySet;

#[allow(non_snake_case)]
pub mod LootContextParamSets {
    use super::*;

    pub const EMPTY: ContextKeySet = ContextKeySet::new(&[], &[]);
    pub const ALL_PARAMS: ContextKeySet = ContextKeySet::new(
        &[
            ThisEntity,
            LastDamagePlayer,
            DamageSource,
            AttackingEntity,
            DirectAttackingEntity,
            Origin,
            BlockState,
            BlockEntity,
            Tool,
            ExplosionRadius,
            AdditionalCostComponentAllowed,
            Container,
            InteractingEntity,
            TargetEntity,
            EnchantmentActive,
            EnchantmentLevel,
        ],
        &[],
    );
    pub const CHEST: ContextKeySet = ContextKeySet::new(&[Origin], &[ThisEntity]);
    pub const COMMAND: ContextKeySet = ContextKeySet::new(&[Origin], &[ThisEntity]);
    pub const COMMAND_SLOT_SOURCE: ContextKeySet = ContextKeySet::new(&[Origin, Container], &[ThisEntity]);
    pub const COMMAND_COMPUTE_DEFAULT: ContextKeySet = ContextKeySet::new(&[Origin], &[ThisEntity]);
    pub const COMMAND_COMPUTE_POSITION: ContextKeySet = ContextKeySet::new(&[Origin, BlockState], &[ThisEntity, BlockEntity]);
    pub const COMMAND_COMPUTE_ENTITY: ContextKeySet = ContextKeySet::new(&[Origin, TargetEntity], &[ThisEntity]);
    pub const SELECTOR: ContextKeySet = ContextKeySet::new(&[Origin, ThisEntity], &[]);
    pub const VILLAGER_TRADE: ContextKeySet = ContextKeySet::new(&[Origin, ThisEntity, AdditionalCostComponentAllowed], &[]);
    pub const FISHING: ContextKeySet = ContextKeySet::new(&[Origin, Tool], &[ThisEntity]);
    pub const ENTITY: ContextKeySet = ContextKeySet::new(&[ThisEntity, Origin, DamageSource], &[AttackingEntity, DirectAttackingEntity, LastDamagePlayer]);
    pub const EQUIPMENT: ContextKeySet = ContextKeySet::new(&[Origin, ThisEntity], &[]);
    pub const ARCHAEOLOGY: ContextKeySet = ContextKeySet::new(&[Origin, ThisEntity, Tool], &[]);
    pub const GIFT: ContextKeySet = ContextKeySet::new(&[Origin, ThisEntity], &[]);
    pub const PIGLIN_BARTER: ContextKeySet = ContextKeySet::new(&[ThisEntity], &[]);
    pub const VAULT: ContextKeySet = ContextKeySet::new(&[Origin], &[ThisEntity, Tool]);
    pub const ADVANCEMENT_REWARD: ContextKeySet = ContextKeySet::new(&[ThisEntity, Origin], &[]);
    pub const ADVANCEMENT_ENTITY: ContextKeySet = ContextKeySet::new(&[ThisEntity, Origin], &[]);
    pub const ADVANCEMENT_LOCATION: ContextKeySet = ContextKeySet::new(&[ThisEntity, Origin, Tool, BlockState], &[BlockEntity]);
    pub const BLOCK_USE: ContextKeySet = ContextKeySet::new(&[ThisEntity, Origin, BlockState], &[]);
    pub const BLOCK: ContextKeySet = ContextKeySet::new(&[BlockState, Origin, Tool], &[ThisEntity, BlockEntity, ExplosionRadius]);
    pub const SHEARING: ContextKeySet = ContextKeySet::new(&[Origin, ThisEntity, Tool], &[]);
    pub const ENTITY_INTERACT: ContextKeySet = ContextKeySet::new(&[TargetEntity, Tool], &[InteractingEntity]);
    pub const BLOCK_INTERACT: ContextKeySet = ContextKeySet::new(&[BlockState, Origin], &[BlockEntity, InteractingEntity, Tool]);
    pub const CONTAINER_PROCESS: ContextKeySet = ContextKeySet::new(&[BlockEntity, BlockState, Container, Origin], &[]);
    pub const ENCHANTED_DAMAGE: ContextKeySet =
        ContextKeySet::new(&[ThisEntity, EnchantmentLevel, Origin, DamageSource], &[DirectAttackingEntity, AttackingEntity]);
    pub const ENCHANTED_ITEM: ContextKeySet = ContextKeySet::new(&[Tool, EnchantmentLevel], &[]);
    pub const ENCHANTED_LOCATION: ContextKeySet = ContextKeySet::new(&[ThisEntity, EnchantmentLevel, Origin, EnchantmentActive], &[]);
    pub const ENCHANTED_ENTITY: ContextKeySet = ContextKeySet::new(&[ThisEntity, EnchantmentLevel, Origin], &[]);
    pub const HIT_BLOCK: ContextKeySet = ContextKeySet::new(&[ThisEntity, EnchantmentLevel, Origin, BlockState], &[]);

    /// The registered sets by name (`BuiltInRegistries.CONTEXT_KEY_SET`, registration order).
    pub const ALL: &[(&str, ContextKeySet)] = &[
        ("empty", EMPTY),
        ("generic", ALL_PARAMS),
        ("chest", CHEST),
        ("command", COMMAND),
        ("command_slot_source", COMMAND_SLOT_SOURCE),
        ("command_compute_default", COMMAND_COMPUTE_DEFAULT),
        ("command_compute_position", COMMAND_COMPUTE_POSITION),
        ("command_compute_entity", COMMAND_COMPUTE_ENTITY),
        ("selector", SELECTOR),
        ("villager_trade", VILLAGER_TRADE),
        ("fishing", FISHING),
        ("entity", ENTITY),
        ("equipment", EQUIPMENT),
        ("archaeology", ARCHAEOLOGY),
        ("gift", GIFT),
        ("barter", PIGLIN_BARTER),
        ("vault", VAULT),
        ("advancement_reward", ADVANCEMENT_REWARD),
        ("advancement_entity", ADVANCEMENT_ENTITY),
        ("advancement_location", ADVANCEMENT_LOCATION),
        ("block_use", BLOCK_USE),
        ("block", BLOCK),
        ("shearing", SHEARING),
        ("entity_interact", ENTITY_INTERACT),
        ("block_interact", BLOCK_INTERACT),
        ("container_process", CONTAINER_PROCESS),
        ("enchanted_damage", ENCHANTED_DAMAGE),
        ("enchanted_item", ENCHANTED_ITEM),
        ("enchanted_location", ENCHANTED_LOCATION),
        ("enchanted_entity", ENCHANTED_ENTITY),
        ("hit_block", HIT_BLOCK),
    ];

    /// The registered name of `set` (`registry.getKey(set)`).
    pub fn name_of(set: &ContextKeySet) -> Option<&'static str> {
        ALL.iter().find(|(_, s)| s == set).map(|(n, _)| *n)
    }

    pub fn by_name(name: &str) -> Option<ContextKeySet> {
        ALL.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
    }

    /// `CODEC = BuiltInRegistries.CONTEXT_KEY_SET.byNameCodec()`.
    pub fn codec() -> Codec<ContextKeySet> {
        mc_dfu::extra::IDENTIFIER.flat_xmap(
            |id| match (id.namespace() == "minecraft").then(|| by_name(id.path())).flatten() {
                Some(s) => DataResult::success(s),
                None => DataResult::error(move || format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:context_key_set]: {id}")),
            },
            |set: &ContextKeySet| match name_of(set) {
                Some(n) => DataResult::success(mc_core::ident::Identifier::new("minecraft", n).expect("valid")),
                None => DataResult::error_str("Element with unknown name"),
            },
        )
    }

    /// `validate()`: every set's keys are a subset of `generic`'s.
    pub fn validate() {
        for (name, set) in ALL {
            if *name != "generic" {
                for k in set.allowed() {
                    assert!(ALL_PARAMS.is_required(k), "Missing parameters from 'all_params': {k}");
                }
            }
        }
    }
}
