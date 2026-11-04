//! Registry ids of the built-in registries (`ByteBufCodecs.registry(..)` / `idMapper`): a VarInt
//! protocol id, resolved against `mc_data`'s tables. Unknown ids fail like `IdMap.byIdOrThrow`.
use mc_data::registry::RegistryId;

use super::{read_varint, write_varint, Decode, DecodeError, DecodeResult, Encode};

/// Decode the VarInt protocol id of a built-in registry entry.
pub fn read_registry_id<T: RegistryId>(input: &mut &[u8]) -> DecodeResult<T> {
    let id = read_varint(input)?;
    u32::try_from(id).ok().and_then(T::from_id).ok_or_else(|| DecodeError::Message(format!("No value with id {id}")))
}

macro_rules! registry_codecs {
    ($($ty:ident),* $(,)?) => {$(
        impl Encode for mc_data::$ty {
            fn encode(&self, out: &mut Vec<u8>) { write_varint(out, self.id() as i32) }
        }
        impl Decode for mc_data::$ty {
            fn decode(input: &mut &[u8]) -> DecodeResult<Self> { read_registry_id(input) }
        }
    )*};
}

registry_codecs!(
    Activity, Attribute, AttributeType, Block, BlockEntityType, BlockPredicateType, ChunkStatus, CommandArgumentType,
    ConsumeEffectType, ContextFloatProviderType, ContextIntProviderType, ContextKeySet, CreativeModeTab, CustomStat,
    DataComponentPredicateType, DataComponentType, DebugSubscription, DialogActionType, DialogBodyType, DialogType,
    EnchantmentEffectComponentType, EnchantmentEntityEffectType, EnchantmentLevelBasedValueType,
    EnchantmentLocationBasedEffectType, EnchantmentProviderType, EnchantmentValueEffectType, EntitySubPredicateType,
    EntityType, EnvironmentAttribute, FloatProviderType, Fluid, GameEvent, GameRule, HeightProviderType,
    IncomingRpcMethods, InputControlType, IntProviderType, Item, LootConditionType, LootFunctionType, LootNbtProviderType,
    LootPoolEntryType, LootScoreProviderType, MapDecorationType, MemoryModuleType, MenuType, MobEffect, NumberFormatType,
    OutgoingRpcMethods, ParticleType, PermissionCheckType, PermissionType, PointOfInterestType, PosRuleTest,
    PositionSourceType, Potion, RecipeBookCategory, RecipeDisplay, RecipeSerializer, RecipeType, RuleBlockEntityModifier,
    RuleTest, SensorType, SlotDisplay, SlotSourceType, SoundEvent, SpawnConditionType, StatType,
    TestEnvironmentDefinitionType, TestFunction, TestInstanceType, TicketType, TriggerType, VillagerProfession,
    VillagerType, WorldgenBiomeSource, WorldgenBlockStateProviderType, WorldgenCarverType, WorldgenChunkGenerator,
    WorldgenDensityFunctionType, WorldgenFeatureSizeType, WorldgenFeatureType, WorldgenFoliagePlacerType,
    WorldgenMaterialConditionType, WorldgenMaterialRuleType, WorldgenPlacementModifierType, WorldgenPoolAliasBinding,
    WorldgenRootPlacerType, WorldgenStructurePiece, WorldgenStructurePlacement, WorldgenStructurePoolElement,
    WorldgenStructureProcessor, WorldgenStructureType, WorldgenTreeDecoratorType, WorldgenTrunkPlacerType,
);

/// `Block.BLOCK_STATE_REGISTRY` (`ByteBufCodecs.idMapper`): the global block state id as a VarInt.
impl Encode for mc_data::block::BlockState {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.id() as i32)
    }
}
impl Decode for mc_data::block::BlockState {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let id = read_varint(input)?;
        u32::try_from(id)
            .ok()
            .and_then(mc_data::block::BlockState::from_id)
            .ok_or_else(|| DecodeError::Message(format!("No value with id {id}")))
    }
}
