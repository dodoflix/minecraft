//! Fluids and fluid states (`Fluid.FLUID_STATE_REGISTRY` ids).

use crate::block::BlockState;
use crate::shape::ShapeId;

pub use crate::ids::Fluid;
use crate::ids::{Item, SoundEvent};

#[derive(Debug)]
pub struct FluidData {
    pub id: u32,
    pub name: &'static str,
    pub class: &'static str,
    pub bucket: Option<Item>,
    pub explosion_resistance: f32,
    pub pickup_sound: Option<SoundEvent>,
    pub default_state: u16,
    pub first_state: u16,
    pub state_count: u16,
}

#[derive(Debug)]
pub struct FluidStateData {
    pub fluid: Fluid,
    /// `(property name, value name)` pairs (`falling`, `level`).
    pub properties: &'static [(&'static str, &'static str)],
    pub amount: u8,
    pub is_source: bool,
    pub is_empty: bool,
    pub own_height: f32,
    pub random_ticking: bool,
    /// `createLegacyBlock()`.
    pub legacy_block_state: BlockState,
    /// `Fluid.getShape(state, EmptyBlockGetter, ZERO)`.
    pub shape: ShapeId,
}

/// Fluid state id (`Fluid.FLUID_STATE_REGISTRY`), as stored in [`crate::block::StateData::fluid_state`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct FluidState(pub(crate) u8);

include!(concat!(env!("OUT_DIR"), "/fluids.rs"));

impl Fluid {
    pub fn data(self) -> &'static FluidData {
        &FLUID_DATA[self.0 as usize]
    }

    pub fn default_state(self) -> FluidState {
        FluidState(self.data().default_state as u8)
    }

    pub fn states(self) -> impl ExactSizeIterator<Item = FluidState> {
        let d = self.data();
        (d.first_state..d.first_state + d.state_count).map(|i| FluidState(i as u8))
    }
}

impl FluidState {
    pub const EMPTY: FluidState = FluidState(0);

    pub const fn id(self) -> u32 {
        self.0 as u32
    }

    pub fn from_id(id: u32) -> Option<FluidState> {
        ((id as usize) < FLUID_STATE_DATA.len()).then_some(FluidState(id as u8))
    }

    pub fn data(self) -> &'static FluidStateData {
        &FLUID_STATE_DATA[self.0 as usize]
    }

    pub fn fluid(self) -> Fluid {
        self.data().fluid
    }

    pub fn is_empty(self) -> bool {
        self.data().is_empty
    }

    pub fn is_source(self) -> bool {
        self.data().is_source
    }

    /// `getAmount()` (0..=9, 8 = full).
    pub fn amount(self) -> u8 {
        self.data().amount
    }

    pub fn own_height(self) -> f32 {
        self.data().own_height
    }

    /// `createLegacyBlock()`.
    pub fn create_legacy_block(self) -> BlockState {
        self.data().legacy_block_state
    }
}
