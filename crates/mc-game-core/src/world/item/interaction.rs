//! `InteractionHand` and the sealed `InteractionResult` (26.3).

use super::ItemStack;
use crate::world::entity::equipment::EquipmentSlot;

/// `InteractionHand` (wire id = discriminant).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InteractionHand {
    MainHand = 0,
    OffHand = 1,
}

impl InteractionHand {
    pub fn id(self) -> i32 {
        self as i32
    }
    /// `BY_ID` with `OutOfBoundsStrategy.ZERO`.
    pub fn by_id(id: i32) -> InteractionHand {
        if id == 1 { Self::OffHand } else { Self::MainHand }
    }
    pub fn to_protocol(self) -> mc_protocol::packets::play::InteractionHand {
        match self {
            Self::MainHand => mc_protocol::packets::play::InteractionHand::MainHand,
            Self::OffHand => mc_protocol::packets::play::InteractionHand::OffHand,
        }
    }
    pub fn from_protocol(hand: mc_protocol::packets::play::InteractionHand) -> InteractionHand {
        match hand {
            mc_protocol::packets::play::InteractionHand::MainHand => Self::MainHand,
            mc_protocol::packets::play::InteractionHand::OffHand => Self::OffHand,
        }
    }
    pub fn as_equipment_slot(self) -> EquipmentSlot {
        match self {
            Self::MainHand => EquipmentSlot::MainHand,
            Self::OffHand => EquipmentSlot::OffHand,
        }
    }
}

/// `InteractionResult.SwingSource`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwingSource {
    None,
    Predicted,
    ServerOnly,
}

/// `InteractionResult.ItemContext`.
#[derive(Clone, PartialEq, Debug)]
pub struct ItemContext {
    pub was_item_interaction: bool,
    pub held_item_transformed_to: Option<ItemStack>,
}

impl ItemContext {
    pub const NONE: ItemContext = ItemContext { was_item_interaction: false, held_item_transformed_to: None };
    pub const DEFAULT: ItemContext = ItemContext { was_item_interaction: true, held_item_transformed_to: None };
}

/// `InteractionResult.Success`.
#[derive(Clone, PartialEq, Debug)]
pub struct Success {
    pub swing_source: SwingSource,
    pub item_context: ItemContext,
}

impl Success {
    pub fn should_swing(&self) -> bool {
        self.swing_source != SwingSource::None
    }
    pub fn held_item_transformed_to(self, stack: ItemStack) -> Success {
        Success { swing_source: self.swing_source, item_context: ItemContext { was_item_interaction: true, held_item_transformed_to: Some(stack) } }
    }
    pub fn without_item(self) -> Success {
        Success { swing_source: self.swing_source, item_context: ItemContext::NONE }
    }
    pub fn was_item_interaction(&self) -> bool {
        self.item_context.was_item_interaction
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum InteractionResult {
    Success(Success),
    Fail,
    Pass,
    TryEmptyHandInteraction,
}

impl InteractionResult {
    pub const SUCCESS: InteractionResult = InteractionResult::Success(Success { swing_source: SwingSource::Predicted, item_context: ItemContext::DEFAULT });
    pub const SUCCESS_SERVER: InteractionResult = InteractionResult::Success(Success { swing_source: SwingSource::ServerOnly, item_context: ItemContext::DEFAULT });
    pub const CONSUME: InteractionResult = InteractionResult::Success(Success { swing_source: SwingSource::None, item_context: ItemContext::DEFAULT });
    pub const FAIL: InteractionResult = InteractionResult::Fail;
    pub const PASS: InteractionResult = InteractionResult::Pass;
    pub const TRY_WITH_EMPTY_HAND: InteractionResult = InteractionResult::TryEmptyHandInteraction;

    /// `Success.heldItemTransformedTo(stack)` (a no-op for the other results).
    pub fn held_item_transformed_to(self, stack: ItemStack) -> InteractionResult {
        match self {
            InteractionResult::Success(s) => InteractionResult::Success(s.held_item_transformed_to(stack)),
            other => other,
        }
    }

    /// `consumesAction()`.
    pub fn consumes_action(&self) -> bool {
        matches!(self, Self::Success(_))
    }
}
