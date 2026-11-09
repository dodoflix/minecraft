//! `UseOnContext`, `BlockPlaceContext`, `DirectionalPlaceContext`.
//!
//! Vanilla's contexts hold the `Level` and the `Player`; those are passed separately in Rust (the level and the player
//! are different `&mut` borrows), so the context is a *snapshot* of everything block/item code reads from them: the
//! hit, the hand and a copy of the held stack (the caller writes `item_stack` back into the hand after the call, which
//! is how vanilla's in-place `consume`/`hurtAndBreak` on the shared `ItemStack` object becomes visible), and the
//! player facts used for orientation (`getHorizontalDirection`, `getRotation`, nearest-looking directions, sneaking).

use super::{InteractionHand, ItemStack};
use crate::world::level::block::BlockStateExt;
use mc_core::direction::{Axis, Direction};
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;

/// The player facts a `UseOnContext` exposes.
#[derive(Clone, Copy, Debug)]
pub struct ContextPlayer {
    pub entity_id: i32,
    pub y_rot: f32,
    pub x_rot: f32,
    pub secondary_use_active: bool,
    /// `Player.canUseGameMasterBlocks()`.
    pub can_use_game_master_blocks: bool,
}

#[derive(Clone, Debug)]
pub struct UseOnContext {
    pub player: Option<ContextPlayer>,
    pub hand: InteractionHand,
    pub hit_result: BlockHitResult,
    /// `getItemInHand()`: mutate it, the caller stores it back.
    pub item_stack: ItemStack,
}

impl UseOnContext {
    pub fn new(
        player: Option<ContextPlayer>,
        hand: InteractionHand,
        item_stack: ItemStack,
        hit_result: BlockHitResult,
    ) -> Self {
        Self {
            player,
            hand,
            hit_result,
            item_stack,
        }
    }

    pub fn get_clicked_pos(&self) -> BlockPos {
        self.hit_result.block_pos
    }
    pub fn get_clicked_face(&self) -> Direction {
        self.hit_result.direction
    }
    pub fn get_click_location(&self) -> Vec3 {
        self.hit_result.location
    }
    pub fn is_inside(&self) -> bool {
        self.hit_result.inside
    }
    pub fn get_item_in_hand(&self) -> &ItemStack {
        &self.item_stack
    }
    pub fn get_hand(&self) -> InteractionHand {
        self.hand
    }
    /// `getHorizontalDirection()`: `player.getDirection()` = `Direction.fromYRot(yRot)`.
    pub fn get_horizontal_direction(&self) -> Direction {
        self.player
            .map_or(Direction::North, |p| Direction::from_y_rot(p.y_rot as f64))
    }
    pub fn is_secondary_use_active(&self) -> bool {
        self.player.is_some_and(|p| p.secondary_use_active)
    }
    pub fn get_rotation(&self) -> f32 {
        self.player.map_or(0.0, |p| p.y_rot)
    }
}

#[derive(Clone, Copy, Debug)]
enum PlaceKind {
    Normal,
    /// `DirectionalPlaceContext` (dispensers): the facing direction.
    Directional(Direction),
}

#[derive(Clone, Debug)]
pub struct BlockPlaceContext {
    pub use_on: UseOnContext,
    relative_pos: BlockPos,
    replace_clicked: bool,
    kind: PlaceKind,
    /// `level.getBlockState(getClickedPos())` when the context was built: `VineBlock.canBeReplaced` reads it for any state
    /// it is asked about, not only the clicked one.
    clicked_state: BlockState,
}

impl BlockPlaceContext {
    /// `new BlockPlaceContext(UseOnContext)`: `state_at` is `level.getBlockState`; the clicked block is replaced iff its
    /// state's `canBeReplaced(this)` holds for the context under construction (`replaceClicked == true` while it runs).
    pub fn new(use_on: UseOnContext, state_at: impl Fn(BlockPos) -> BlockState) -> Self {
        let relative_pos = use_on
            .hit_result
            .block_pos
            .relative(use_on.hit_result.direction);
        let clicked_state = state_at(use_on.hit_result.block_pos);
        let mut ctx = Self {
            use_on,
            relative_pos,
            replace_clicked: true,
            kind: PlaceKind::Normal,
            clicked_state,
        };
        ctx.replace_clicked = clicked_state.can_be_replaced_by(&ctx);
        if !ctx.replace_clicked {
            ctx.clicked_state = state_at(relative_pos);
        }
        ctx
    }

    /// `BlockPlaceContext.at(context, pos, direction)`.
    pub fn at(
        context: &BlockPlaceContext,
        pos: BlockPos,
        direction: Direction,
        state_at: impl Fn(BlockPos) -> BlockState,
    ) -> Self {
        let hit = BlockHitResult {
            location: Vec3::new(
                pos.x() as f64 + 0.5 + direction.get_step_x() as f64 * 0.5,
                pos.y() as f64 + 0.5 + direction.get_step_y() as f64 * 0.5,
                pos.z() as f64 + 0.5 + direction.get_step_z() as f64 * 0.5,
            ),
            direction,
            block_pos: pos,
            miss: false,
            inside: false,
            world_border_hit: false,
        };
        let use_on = UseOnContext::new(
            context.use_on.player,
            context.use_on.hand,
            context.use_on.item_stack.clone(),
            hit,
        );
        Self::new(use_on, state_at)
    }

    /// `new DirectionalPlaceContext(level, pos, direction, dispensed, clickedFace)`; `state_at_pos` is
    /// `level.getBlockState(pos)`.
    pub fn directional(
        pos: BlockPos,
        direction: Direction,
        dispensed: ItemStack,
        clicked_face: Direction,
        state_at_pos: BlockState,
    ) -> Self {
        let hit = BlockHitResult {
            location: Vec3::new(pos.x() as f64 + 0.5, pos.y() as f64, pos.z() as f64 + 0.5),
            direction: clicked_face,
            block_pos: pos,
            miss: false,
            inside: false,
            world_border_hit: false,
        };
        let use_on = UseOnContext::new(None, InteractionHand::MainHand, dispensed, hit);
        let relative_pos = pos.relative(clicked_face);
        Self {
            use_on,
            relative_pos,
            replace_clicked: true,
            kind: PlaceKind::Directional(direction),
            clicked_state: state_at_pos,
        }
    }

    /// `level.getBlockState(getClickedPos())`.
    pub fn get_clicked_state(&self) -> BlockState {
        self.clicked_state
    }

    pub fn get_clicked_pos(&self) -> BlockPos {
        match self.kind {
            PlaceKind::Directional(_) => self.use_on.hit_result.block_pos,
            PlaceKind::Normal => {
                if self.replace_clicked {
                    self.use_on.get_clicked_pos()
                } else {
                    self.relative_pos
                }
            }
        }
    }

    /// `canPlace()`; `target_replaceable` = `level.getBlockState(getClickedPos()).canBeReplaced(this)`, only consulted
    /// when the clicked block itself is not replaced.
    pub fn can_place(&self, target_replaceable: impl FnOnce(&BlockPlaceContext) -> bool) -> bool {
        match self.kind {
            PlaceKind::Directional(_) => target_replaceable(self),
            PlaceKind::Normal => self.replace_clicked || target_replaceable(self),
        }
    }

    /// `replacingClickedOnBlock()`; for the directional context pass the result of [`BlockPlaceContext::can_place`].
    pub fn replacing_clicked_on_block(&self, directional_can_place: bool) -> bool {
        match self.kind {
            PlaceKind::Directional(_) => directional_can_place,
            PlaceKind::Normal => self.replace_clicked,
        }
    }

    pub fn get_clicked_face(&self) -> Direction {
        self.use_on.get_clicked_face()
    }

    pub fn get_horizontal_direction(&self) -> Direction {
        match self.kind {
            PlaceKind::Directional(d) => {
                if d.get_axis() == Axis::Y {
                    Direction::North
                } else {
                    d
                }
            }
            PlaceKind::Normal => self.use_on.get_horizontal_direction(),
        }
    }

    pub fn is_secondary_use_active(&self) -> bool {
        match self.kind {
            PlaceKind::Directional(_) => false,
            PlaceKind::Normal => self.use_on.is_secondary_use_active(),
        }
    }

    pub fn get_rotation(&self) -> f32 {
        match self.kind {
            PlaceKind::Directional(d) => (d.get_2d_data_value() * 90) as f32,
            PlaceKind::Normal => self.use_on.get_rotation(),
        }
    }

    fn ordered_by_nearest(&self) -> [Direction; 6] {
        let p = self
            .use_on
            .player
            .expect("nearest-looking direction needs a player");
        Direction::ordered_by_nearest(p.x_rot, p.y_rot)
    }

    pub fn get_nearest_looking_direction(&self) -> Direction {
        match self.kind {
            PlaceKind::Directional(_) => Direction::Down,
            PlaceKind::Normal => self.ordered_by_nearest()[0],
        }
    }

    pub fn get_nearest_looking_vertical_direction(&self) -> Direction {
        let p = self
            .use_on
            .player
            .expect("nearest-looking direction needs a player");
        Direction::get_facing_axis(Axis::Y, p.x_rot, p.y_rot)
    }

    pub fn get_nearest_looking_directions(&self) -> [Direction; 6] {
        use Direction::*;
        match self.kind {
            PlaceKind::Directional(d) => match d {
                Down => [Down, North, East, South, West, Up],
                Up => [Down, Up, North, East, South, West],
                North => [Down, North, East, West, Up, South],
                South => [Down, South, East, West, Up, North],
                West => [Down, West, South, Up, North, East],
                East => [Down, East, South, Up, North, West],
            },
            PlaceKind::Normal => {
                let mut directions = self.ordered_by_nearest();
                if !self.replace_clicked {
                    let clicked_face = self.get_clicked_face();
                    let opposite = clicked_face.get_opposite();
                    let mut index = 0;
                    while index < directions.len() && directions[index] != opposite {
                        index += 1;
                    }
                    if index > 0 {
                        directions.copy_within(0..index, 1);
                        directions[0] = opposite;
                    }
                }
                directions
            }
        }
    }
}
