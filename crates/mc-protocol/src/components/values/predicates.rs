//! `can_place_on` / `can_break` (`AdventureModePredicate`) and the predicate tree behind them: `BlockPredicate`,
//! `StatePropertiesPredicate`, `NbtPredicate`, `DataComponentMatchers`, the `data_component_predicate_type`
//! predicates, `ItemPredicate`, `CollectionPredicate`, `MinMaxBounds`.

use crate::codec::EncodeResult;
use crate::codec::{Decode, DecodeResult, Encode};
use crate::components::component::{ComponentDef, ComponentKey};
use mc_core::pos::BlockPos;
use mc_data::DataComponentType;
use mc_data::block::BlockState;
use mc_dfu::Codec;
use mc_dfu::extra::{compact_list_codec_with, non_empty_list};
use mc_nbt::CompoundTag;
use std::sync::{Arc, LazyLock, Mutex};

mod block;
mod bounds;
mod collection;
mod component_predicates;
mod matchers;
mod matching;
mod state;

pub use block::BlockPredicate;
pub use bounds::{BoundNumber, Bounds, Doubles, FloatDegrees, Ints};
pub use collection::{CollectionPredicate, CountEntry};
pub use component_predicates::*;
pub use matchers::{DataComponentMatchers, DataComponentPredicate, PredicateMap, PredicateType};
pub use matching::{AdventureBlockEntities, holder_set_contains};
pub use state::{NbtPredicate, PropertyMatcher, StatePropertiesPredicate, ValueMatcher};

pub const CAN_PLACE_ON: ComponentKey<AdventureModePredicate> =
    ComponentKey::new(DataComponentType::CAN_PLACE_ON);
pub const CAN_BREAK: ComponentKey<AdventureModePredicate> =
    ComponentKey::new(DataComponentType::CAN_BREAK);

/// `AdventureModePredicate`: blocks the item may be placed on / may break in adventure mode.
#[derive(Clone, Debug)]
pub struct AdventureModePredicate {
    pub predicates: Vec<BlockPredicate>,
    /// `lastCheckedBlock`, `lastResult`, `checksBlockEntity`: the copies of a value share them, like vanilla's object fields.
    last_checked: Arc<Mutex<Option<LastCheckedBlock>>>,
}

impl PartialEq for AdventureModePredicate {
    fn eq(&self, other: &Self) -> bool {
        self.predicates == other.predicates
    }
}

/// The block `AdventureModePredicate.test` decided on last.
#[derive(Debug)]
struct LastCheckedBlock {
    pos: BlockPos,
    state: BlockState,
    /// `saveWithId` of its block entity when the check loaded it (vanilla keeps the entity object itself).
    entity: Option<CompoundTag>,
    checks_block_entity: bool,
    result: bool,
}

impl AdventureModePredicate {
    pub fn new(predicates: Vec<BlockPredicate>) -> Self {
        Self {
            predicates,
            last_checked: Arc::default(),
        }
    }

    /// `ExtraCodecs.compactListCodec(BlockPredicate.CODEC, nonEmptyList(BlockPredicate.CODEC.listOf()))`.
    pub fn codec() -> Codec<AdventureModePredicate> {
        static C: LazyLock<Codec<AdventureModePredicate>> = LazyLock::new(|| {
            let element = BlockPredicate::codec();
            compact_list_codec_with(&element, &non_empty_list(&element.list_of()))
                .xmap(AdventureModePredicate::new, |p: &AdventureModePredicate| {
                    p.predicates.clone()
                })
        });
        C.clone()
    }
}

impl Encode for AdventureModePredicate {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.predicates.encode(out)?;
        Ok(())
    }
}
impl Decode for AdventureModePredicate {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Vec::<BlockPredicate>::decode(input).map(AdventureModePredicate::new)
    }
}

pub(crate) fn defs(out: &mut Vec<ComponentDef>) {
    out.push(
        ComponentDef::builder(CAN_PLACE_ON)
            .persistent(AdventureModePredicate::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(CAN_BREAK)
            .persistent(AdventureModePredicate::codec())
            .network_synchronized()
            .build(),
    );
}
