//! `PoiType` + `PoiTypes`: the point-of-interest registry (`mc_data::misc::POI_TYPES`, registration order) and the
//! block state -> type table.

use mc_data::block::BlockState;
use mc_data::misc::{POI_TYPES, PoiTypeData};
use std::sync::OnceLock;

/// `Holder<PoiType>`: one of the registered point of interest types. Equality is the registry entry's identity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct PoiTypeHolder(u8);

impl PoiTypeHolder {
    /// Registry entry at `index` of [`POI_TYPES`] (registration order).
    pub fn from_index(index: usize) -> Option<PoiTypeHolder> {
        (index < POI_TYPES.len()).then_some(PoiTypeHolder(index as u8))
    }

    /// The holder registered under `name` (`minecraft:home`).
    pub fn by_name(name: &str) -> Option<PoiTypeHolder> {
        POI_TYPES.iter().position(|t| t.name == name).map(|i| PoiTypeHolder(i as u8))
    }

    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    /// `holder.value()`: matching states, max tickets, valid range.
    pub fn value(self) -> &'static PoiTypeData {
        &POI_TYPES[self.index()]
    }

    /// `holder.getRegisteredName()` (`minecraft:home`).
    pub fn registered_name(self) -> &'static str {
        self.value().name
    }

    /// `holder.is(ResourceKey)`: `key` is one of the [`PoiTypes`] names.
    pub fn is(self, key: &str) -> bool {
        self.registered_name() == key
    }

    /// `holder.value().maxTickets()`.
    pub fn max_tickets(self) -> i32 {
        self.value().max_tickets
    }

    /// `PoiType.is(state)`.
    pub fn matches_state(self, state: BlockState) -> bool {
        self.value().matching_states.binary_search(&(state.id() as u16)).is_ok()
    }
}

/// `PoiTypes`: the registry keys and the state lookups.
pub struct PoiTypes;

impl PoiTypes {
    pub const ARMORER: &'static str = "minecraft:armorer";
    pub const BUTCHER: &'static str = "minecraft:butcher";
    pub const CARTOGRAPHER: &'static str = "minecraft:cartographer";
    pub const CLERIC: &'static str = "minecraft:cleric";
    pub const FARMER: &'static str = "minecraft:farmer";
    pub const FISHERMAN: &'static str = "minecraft:fisherman";
    pub const FLETCHER: &'static str = "minecraft:fletcher";
    pub const LEATHERWORKER: &'static str = "minecraft:leatherworker";
    pub const LIBRARIAN: &'static str = "minecraft:librarian";
    pub const MASON: &'static str = "minecraft:mason";
    pub const SHEPHERD: &'static str = "minecraft:shepherd";
    pub const TOOLSMITH: &'static str = "minecraft:toolsmith";
    pub const WEAPONSMITH: &'static str = "minecraft:weaponsmith";
    pub const HOME: &'static str = "minecraft:home";
    pub const MEETING: &'static str = "minecraft:meeting";
    pub const BEEHIVE: &'static str = "minecraft:beehive";
    pub const BEE_NEST: &'static str = "minecraft:bee_nest";
    pub const NETHER_PORTAL: &'static str = "minecraft:nether_portal";
    pub const LODESTONE: &'static str = "minecraft:lodestone";
    pub const LIGHTNING_ROD: &'static str = "minecraft:lightning_rod";
    pub const TEST_INSTANCE: &'static str = "minecraft:test_instance";

    /// `TYPE_BY_STATE`: type index per block state id, `u8::MAX` = not a point of interest.
    fn type_by_state() -> &'static [u8] {
        static TABLE: OnceLock<Vec<u8>> = OnceLock::new();
        TABLE.get_or_init(|| {
            let max = POI_TYPES.iter().flat_map(|t| t.matching_states.iter().copied()).max().map_or(0, |m| usize::from(m) + 1);
            let mut table = vec![u8::MAX; max];
            for (index, ty) in POI_TYPES.iter().enumerate() {
                for &state in ty.matching_states {
                    // vanilla: "%s is defined in more than one PoI type" (IllegalStateException)
                    assert!(table[usize::from(state)] == u8::MAX, "state {state} is defined in more than one PoI type");
                    table[usize::from(state)] = index as u8;
                }
            }
            table
        })
    }

    /// `PoiTypes.forState(state)`.
    pub fn for_state(state: BlockState) -> Option<PoiTypeHolder> {
        match Self::type_by_state().get(state.id() as usize) {
            Some(&index) if index != u8::MAX => Some(PoiTypeHolder(index)),
            _ => None,
        }
    }

    /// `PoiTypes.hasPoi(state)`.
    pub fn has_poi(state: BlockState) -> bool {
        Self::for_state(state).is_some()
    }
}
