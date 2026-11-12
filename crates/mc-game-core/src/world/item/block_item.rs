//! `BlockItem` and its subclasses (`StandingAndWallBlockItem` incl. banners/heads/hanging signs, `DoubleHighBlockItem`,
//! `GameMasterBlockItem`, `PlaceOnWaterBlockItem`, `ScaffoldingBlockItem`, `SolidBucketItem`): placing blocks.

use super::{
    BlockPlaceContext, InteractionHand, InteractionResult, ItemBehaviour, ItemStack, ItemStackExt,
    PlainItem, UseOnContext, bucket_item::DispensibleContainerItem, consumable,
};
use crate::commands::arguments::coordinates::block_pos_argument::is_in_world_bounds_horizontal;
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::block::impls::scaffolding;
use crate::world::level::block::{BlockStateExt, behaviour as block_behaviour};
use crate::world::level::collision::{
    BlockGetterExt, ClipContext, CollisionContext, CollisionGetter,
};
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::direction::Direction;
use mc_core::phys::shapes::{BooleanOp, Shapes};
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::{DataComponentType, GameEvent, Item, SoundEvent};
use mc_protocol::components::{BLOCK_ENTITY_DATA, BLOCK_STATE, CONTAINER, ItemContainerContents};
use mc_protocol::packets::play::SoundSource;
use mc_world::level_storage::FeatureFlagSet;

#[derive(Clone, Copy, PartialEq, Eq)]
enum StandingKind {
    Plain,
    /// `HangingSignItem`: `WallHangingSignBlock.canPlace` must hold for wall states.
    HangingSign,
}

/// (standing block, wall block, attachment direction, kind) of every `StandingAndWallBlockItem` (extracted from
/// `Items.java`: torches, coral fans, signs, hanging signs, heads, banners).
static STANDING_AND_WALL: &[(&str, &str, Direction, StandingKind)] = &[
    ("torch", "wall_torch", Direction::Down, StandingKind::Plain),
    (
        "soul_torch",
        "soul_wall_torch",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "copper_torch",
        "copper_wall_torch",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "tube_coral_fan",
        "tube_coral_wall_fan",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "brain_coral_fan",
        "brain_coral_wall_fan",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "bubble_coral_fan",
        "bubble_coral_wall_fan",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "fire_coral_fan",
        "fire_coral_wall_fan",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "horn_coral_fan",
        "horn_coral_wall_fan",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "dead_tube_coral_fan",
        "dead_tube_coral_wall_fan",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "dead_brain_coral_fan",
        "dead_brain_coral_wall_fan",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "dead_bubble_coral_fan",
        "dead_bubble_coral_wall_fan",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "dead_fire_coral_fan",
        "dead_fire_coral_wall_fan",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "dead_horn_coral_fan",
        "dead_horn_coral_wall_fan",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "redstone_torch",
        "redstone_wall_torch",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "oak_sign",
        "oak_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "spruce_sign",
        "spruce_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "birch_sign",
        "birch_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "jungle_sign",
        "jungle_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "acacia_sign",
        "acacia_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "cherry_sign",
        "cherry_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "dark_oak_sign",
        "dark_oak_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "pale_oak_sign",
        "pale_oak_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "mangrove_sign",
        "mangrove_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "poplar_sign",
        "poplar_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "bamboo_sign",
        "bamboo_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "crimson_sign",
        "crimson_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "warped_sign",
        "warped_wall_sign",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "oak_hanging_sign",
        "oak_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "spruce_hanging_sign",
        "spruce_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "birch_hanging_sign",
        "birch_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "jungle_hanging_sign",
        "jungle_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "acacia_hanging_sign",
        "acacia_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "cherry_hanging_sign",
        "cherry_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "dark_oak_hanging_sign",
        "dark_oak_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "pale_oak_hanging_sign",
        "pale_oak_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "mangrove_hanging_sign",
        "mangrove_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "poplar_hanging_sign",
        "poplar_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "bamboo_hanging_sign",
        "bamboo_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "crimson_hanging_sign",
        "crimson_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "warped_hanging_sign",
        "warped_wall_hanging_sign",
        Direction::Up,
        StandingKind::HangingSign,
    ),
    (
        "skeleton_skull",
        "skeleton_wall_skull",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "wither_skeleton_skull",
        "wither_skeleton_wall_skull",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "player_head",
        "player_wall_head",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "zombie_head",
        "zombie_wall_head",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "creeper_head",
        "creeper_wall_head",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "dragon_head",
        "dragon_wall_head",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "piglin_head",
        "piglin_wall_head",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "white_banner",
        "white_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "orange_banner",
        "orange_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "magenta_banner",
        "magenta_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "light_blue_banner",
        "light_blue_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "yellow_banner",
        "yellow_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "lime_banner",
        "lime_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "pink_banner",
        "pink_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "gray_banner",
        "gray_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "light_gray_banner",
        "light_gray_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "cyan_banner",
        "cyan_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "purple_banner",
        "purple_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "blue_banner",
        "blue_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "brown_banner",
        "brown_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "green_banner",
        "green_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "red_banner",
        "red_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
    (
        "black_banner",
        "black_wall_banner",
        Direction::Down,
        StandingKind::Plain,
    ),
];

#[derive(Clone, Copy)]
enum Kind {
    Plain,
    StandingAndWall {
        wall: Block,
        attachment: Direction,
        kind: StandingKind,
    },
    DoubleHigh,
    GameMaster,
    PlaceOnWater,
    Scaffolding,
    SolidBucket {
        sound: SoundEvent,
    },
}

pub struct BlockItem {
    block: Block,
    kind: Kind,
    /// `PlayerHeadItem`: its `descriptionId` (the name becomes `<id>.named` for a profile with a name).
    player_head: Option<&'static str>,
}

/// `Block.isEnabled(enabledFeatures)`.
pub fn block_is_enabled(block: Block, enabled: FeatureFlagSet) -> bool {
    let required: Vec<mc_core::ident::Identifier> = block
        .data()
        .required_features
        .iter()
        .filter_map(|n| mc_core::ident::Identifier::try_parse(n))
        .collect();
    FeatureFlagSet::from_names(required.iter(), |_| {}).is_subset_of(enabled)
}

impl BlockItem {
    pub fn for_item(item: Item) -> Self {
        let block = item.block().expect("BlockItem without block");
        let name = block.data().name.trim_start_matches("minecraft:");
        let kind = match STANDING_AND_WALL.iter().find(|(b, ..)| *b == name) {
            Some((_, wall, attachment, kind)) => Kind::StandingAndWall {
                wall: Block::by_name(&format!("minecraft:{wall}")).expect("wall block"),
                attachment: *attachment,
                kind: *kind,
            },
            None => Kind::Plain,
        };
        Self {
            block,
            kind,
            player_head: None,
        }
    }

    fn with_kind(item: Item, kind: Kind) -> Self {
        Self {
            block: item.block().expect("BlockItem without block"),
            kind,
            player_head: None,
        }
    }

    fn player_head(mut self, description_id: &'static str) -> Self {
        self.player_head = Some(description_id);
        self
    }

    pub fn get_block(&self) -> Block {
        self.block
    }

    /// `StandingAndWallBlockItem.wallBlock`: `None` for every other `BlockItem`.
    pub fn wall_block(&self) -> Option<Block> {
        match self.kind {
            Kind::StandingAndWall { wall, .. } => Some(wall),
            _ => None,
        }
    }

    fn must_survive(&self) -> bool {
        !matches!(self.kind, Kind::Scaffolding)
    }

    fn get_place_sound(&self, state: BlockState) -> SoundEvent {
        match self.kind {
            Kind::SolidBucket { sound } => sound,
            _ => state.get_sound_type().place_sound,
        }
    }

    /// `BlockItem.place(placeContext)`. `ctx.use_on.item_stack` is consumed in place.
    pub fn place(
        &self,
        ctx: &mut BlockPlaceContext,
        level: &mut ServerLevel,
        mut player: Option<&mut ServerPlayer>,
    ) -> InteractionResult {
        if !block_is_enabled(self.block, level.enabled_features()) {
            return InteractionResult::FAIL;
        }
        if !ctx.can_place(|c| {
            level
                .get_block_state(c.get_clicked_pos())
                .can_be_replaced_by(c)
        }) {
            return InteractionResult::FAIL;
        }
        let Some(mut updated) = self.update_placement_context(ctx, level, player.as_deref_mut())
        else {
            return InteractionResult::FAIL;
        };
        let Some(placement_state) =
            self.get_placement_state(&updated, &mut *level, player.as_deref())
        else {
            return InteractionResult::FAIL;
        };
        if !self.place_block(&updated, placement_state, level) {
            return InteractionResult::FAIL;
        }
        let pos = updated.get_clicked_pos();
        let mut placed_state = level.get_block_state(pos);
        if placed_state.block() == placement_state.block() {
            placed_state =
                update_block_state_from_tag(pos, level, &updated.use_on.item_stack, placed_state);
            update_custom_block_entity_tag(
                level,
                player.as_deref(),
                pos,
                &updated.use_on.item_stack,
            );
            update_block_entity_components(level, pos, &updated.use_on.item_stack);
            block_behaviour(placed_state.block()).set_placed_by(
                placed_state,
                level,
                pos,
                player.as_deref_mut(),
                &updated.use_on.item_stack,
            );
            if let Some(player) = player.as_deref_mut() {
                crate::advancements::triggers::CriteriaTriggers::PLACED_BLOCK.trigger(
                    level,
                    player,
                    pos,
                    &updated.use_on.item_stack,
                );
            }
        }
        let sound_type = placed_state.get_sound_type();
        let player_id = player.as_deref().map(|p| p.base_id());
        level.play_sound(
            player_id,
            pos,
            self.get_place_sound(placed_state),
            SoundSource::Blocks,
            (sound_type.volume + 1.0) / 2.0,
            sound_type.pitch * 0.8,
        );
        level.game_event(
            GameEvent::BLOCK_PLACE,
            mc_core::phys::Vec3::at_center_of(&pos),
            &match player.as_deref() {
                Some(player) => GameEventContext::of_entity_ref(level, player, Some(placed_state)),
                None => GameEventContext::of_state(Some(placed_state)),
            },
        );
        let infinite = player
            .as_deref()
            .is_some_and(|p| p.player.abilities.instabuild);
        updated.use_on.item_stack.consume(1, infinite);
        ctx.use_on.item_stack = updated.use_on.item_stack;
        InteractionResult::SUCCESS
    }

    /// `updatePlacementContext(context)`.
    fn update_placement_context(
        &self,
        ctx: &BlockPlaceContext,
        level: &mut ServerLevel,
        player: Option<&mut ServerPlayer>,
    ) -> Option<BlockPlaceContext> {
        match self.kind {
            Kind::Scaffolding => self.update_scaffolding_placement_context(ctx, level, player),
            _ => Some(ctx.clone()),
        }
    }

    /// `ScaffoldingBlockItem.updatePlacementContext`: clicking scaffolding places at the end of the line of scaffolding
    /// (up, or along the player's horizontal direction when the top face was clicked), at most 7 blocks sideways.
    fn update_scaffolding_placement_context(
        &self,
        ctx: &BlockPlaceContext,
        level: &mut ServerLevel,
        player: Option<&mut ServerPlayer>,
    ) -> Option<BlockPlaceContext> {
        let pos = ctx.get_clicked_pos();
        if level.get_block_state(pos).block() != self.block {
            return (scaffolding::get_distance(&*level, pos) != 7).then(|| ctx.clone());
        }
        let direction = if ctx.is_secondary_use_active() {
            if ctx.use_on.is_inside() {
                ctx.get_clicked_face().get_opposite()
            } else {
                ctx.get_clicked_face()
            }
        } else if ctx.get_clicked_face() == Direction::Up {
            ctx.get_horizontal_direction()
        } else {
            Direction::Up
        };
        let mut horizontal_distance = 0;
        let mut placement_pos = pos.relative(direction);
        while horizontal_distance < 7 {
            if !level.is_in_world_bounds(placement_pos) {
                let max_y = level.get_max_y();
                if let Some(player) = player
                    && placement_pos.y() > max_y
                {
                    player.send_build_limit_message(true, max_y);
                }
                break;
            }
            let replaced = level.get_block_state(placement_pos);
            if replaced.block() != self.block {
                if replaced.can_be_replaced_by(ctx) {
                    return Some(BlockPlaceContext::at(ctx, placement_pos, direction, |p| {
                        level.get_block_state(p)
                    }));
                }
                break;
            }
            placement_pos = placement_pos.relative(direction);
            if direction.get_axis().is_horizontal() {
                horizontal_distance += 1;
            }
        }
        None
    }

    /// `getPlacementState(context)`.
    fn get_placement_state(
        &self,
        ctx: &BlockPlaceContext,
        level: &mut ServerLevel,
        player: Option<&ServerPlayer>,
    ) -> Option<BlockState> {
        match self.kind {
            Kind::GameMaster => {
                if player.is_some_and(|p| !p.can_use_game_master_blocks()) {
                    return None;
                }
                self.base_placement_state(ctx, &mut *level, player)
            }
            Kind::StandingAndWall {
                wall,
                attachment,
                kind,
            } => {
                let wall_state =
                    block_behaviour(wall).get_state_for_placement(wall, &mut *level, ctx);
                let pos = ctx.get_clicked_pos();
                let mut state_for_placement = None;
                for direction in ctx.get_nearest_looking_directions() {
                    if direction != attachment.get_opposite() {
                        let possible = if direction == attachment {
                            block_behaviour(self.block).get_state_for_placement(
                                self.block,
                                &mut *level,
                                ctx,
                            )
                        } else {
                            wall_state
                        };
                        if let Some(possible) = possible
                            && self.standing_can_place(kind, level, possible, pos)
                        {
                            state_for_placement = Some(possible);
                            break;
                        }
                    }
                }
                state_for_placement
                    .filter(|s| level.is_unobstructed_state(*s, pos, &CollisionContext::empty()))
            }
            _ => self.base_placement_state(ctx, &mut *level, player),
        }
    }

    fn standing_can_place(
        &self,
        kind: StandingKind,
        level: &ServerLevel,
        possible: BlockState,
        pos: BlockPos,
    ) -> bool {
        // `HangingSignItem.canPlace`: a wall hanging sign state also needs `WallHangingSignBlock.canPlace`
        if kind == StandingKind::HangingSign
            && possible
                .block()
                .data()
                .classes
                .contains(&"WallHangingSignBlock")
            && !crate::world::level::block::impls::wall_hanging_sign_block::can_place(
                possible, level, pos,
            )
        {
            return false;
        }
        possible.can_survive(level, pos)
    }

    fn base_placement_state(
        &self,
        ctx: &BlockPlaceContext,
        level: &mut ServerLevel,
        player: Option<&ServerPlayer>,
    ) -> Option<BlockState> {
        let state_for_placement =
            block_behaviour(self.block).get_state_for_placement(self.block, &mut *level, ctx);
        state_for_placement.filter(|s| self.can_place(ctx, *s, &*level, player))
    }

    /// `canPlace(context, stateForPlacement)`.
    fn can_place(
        &self,
        ctx: &BlockPlaceContext,
        state: BlockState,
        level: &ServerLevel,
        player: Option<&ServerPlayer>,
    ) -> bool {
        let pos = ctx.get_clicked_pos();
        let context = match player {
            Some(p) => CollisionContext::with_position(
                p.base_id(),
                p.is_shift_key_down(),
                p.base().y(),
                p.held_item_for_collision(),
                p.can_stand_on_fluid_fn(),
            ),
            None => CollisionContext::placement_without_entity(),
        };
        if self.must_survive() && !state.can_survive(level, pos) {
            return false;
        }
        let shape = state.get_collision_shape_ctx(level, pos, &context);
        if shape.is_empty() {
            return true;
        }
        let shape = shape.move_vec3i(*pos);
        // the placing player is out of the level's entity storage while it acts: `isUnobstructed` cannot find it
        let player_in_the_way = player.is_some_and(|p| {
            !p.is_spectator()
                && !p.is_removed()
                && p.base().blocks_building
                && Shapes::join_is_not_empty(
                    &shape,
                    &Shapes::create(p.get_bounding_box()),
                    BooleanOp::AND,
                )
        });
        !player_in_the_way && level.is_unobstructed(None, &shape)
    }

    /// `placeBlock(context, placementState)`.
    fn place_block(
        &self,
        ctx: &BlockPlaceContext,
        state: BlockState,
        level: &mut ServerLevel,
    ) -> bool {
        if let Kind::DoubleHigh = self.kind {
            let above = ctx.get_clicked_pos().above();
            let above_state = if crate::world::level::biome_ext::is_water_at(level, above) {
                Block::WATER.default_state()
            } else {
                Block::AIR.default_state()
            };
            level.set_block_flags(above, above_state, 27);
        }
        level.set_block_flags(ctx.get_clicked_pos(), state, 11)
    }

    fn block_use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let mut place_ctx =
            BlockPlaceContext::new(context.clone(), |pos| level.get_block_state(pos));
        let place_result = self.place(&mut place_ctx, level, Some(player));
        context.item_stack = place_ctx.use_on.item_stack;
        if place_result.consumes_action() {
            return place_result;
        }
        let result = PlainItem.use_on(context, level, player);
        if result.consumes_action() {
            return result;
        }
        if context.item_stack.has(DataComponentType::CONSUMABLE) {
            consumable::base_use(level, player, context.hand)
        } else {
            place_result
        }
    }
}

/// `updateBlockStateFromTag(pos, level, stack, placedState)`.
fn update_block_state_from_tag(
    pos: BlockPos,
    level: &mut ServerLevel,
    stack: &ItemStack,
    placed: BlockState,
) -> BlockState {
    let Some(props) = stack.get(&BLOCK_STATE) else {
        return placed;
    };
    if props.properties.is_empty() {
        return placed;
    }
    // `BlockItemStateProperties.apply(state)`: every named property that exists and parses
    let mut modified = placed;
    for (name, value) in &props.properties {
        if let Some(s) = modified.with_property_str(name, value) {
            modified = s;
        }
    }
    if modified != placed {
        level.set_block_flags(pos, modified, 2);
    }
    modified
}

/// `BlockItem.updateCustomBlockEntityTag(level, player, pos, stack)`.
pub fn update_custom_block_entity_tag(
    level: &mut ServerLevel,
    player: Option<&ServerPlayer>,
    pos: BlockPos,
    stack: &ItemStack,
) -> bool {
    let Some(data) = stack.get(&BLOCK_ENTITY_DATA).cloned() else {
        return false;
    };
    let can_set = !data.ty.data().only_op_can_set_nbt
        || player.is_some_and(|p| p.can_use_game_master_blocks());
    let Some(be) = level.get_block_entity_immediate(pos) else {
        return false;
    };
    if be.get_type() != data.ty || !can_set {
        return false;
    }
    // `TypedEntityData.loadInto(blockEntity)`: merge the saved custom data with the item's and reload when changed
    let mut old = mc_nbt::CompoundTag::new();
    be.save_additional(&mut old);
    let original = old.clone();
    old.merge(&data.tag);
    if old == original {
        return false;
    }
    level.load_block_entity_additional_in_level(pos, &old);
    let state = level.get_block_state(pos);
    level.block_entity_set_changed(pos, state);
    true
}

/// `updateBlockEntityComponents(level, pos, stack)`.
fn update_block_entity_components(level: &mut ServerLevel, pos: BlockPos, stack: &ItemStack) {
    if let Some(be) = level.get_block_entity_immediate(pos) {
        be.apply_components_from_item_stack(stack);
        let state = be.get_block_state();
        level.block_entity_set_changed(pos, state);
    }
}

/// `SolidBucketItem.emptyContents`: the only `BlockItem` that is a `DispensibleContainerItem`.
impl DispensibleContainerItem for BlockItem {
    fn empty_contents(
        &self,
        user: Option<&dyn Entity>,
        level: &mut ServerLevel,
        pos: BlockPos,
        _hit: Option<&BlockHitResult>,
    ) -> bool {
        let Kind::SolidBucket { sound } = self.kind else {
            return false;
        };
        if level.is_outside_build_height(pos.y())
            || !is_in_world_bounds_horizontal(pos)
            || !level.is_empty_block(pos)
        {
            return false;
        }
        level.set_block_and_update(pos, self.block.default_state());
        let user_id = user.map(|u| u.id());
        level.game_event(
            GameEvent::FLUID_PLACE,
            Vec3::at_center_of(&pos),
            &GameEventContext::of_entity(user_id),
        );
        level.play_sound(user_id, pos, sound, SoundSource::Blocks, 1.0, 1.0);
        true
    }
}

impl ItemBehaviour for BlockItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        match self.kind {
            Kind::PlaceOnWater => InteractionResult::PASS,
            Kind::SolidBucket { .. } => {
                let place_result = self.block_use_on(context, level, player);
                if place_result.consumes_action() {
                    // `BucketItem.getEmptySuccessItem(stack, player)`; the caller stores `context.item_stack` into the hand
                    context.item_stack = bucket_empty_success_item(&context.item_stack, player);
                }
                place_result
            }
            _ => self.block_use_on(context, level, player),
        }
    }

    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        if let Kind::PlaceOnWater = self.kind {
            let hit = player_pov_hit_result(level, player, true);
            let above = BlockHitResult {
                block_pos: hit.block_pos.above(),
                ..hit
            };
            let mut ctx = UseOnContext::new(
                Some(player.context_player()),
                hand,
                player.get_item_in_hand(hand),
                above,
            );
            // `super.useOn(context)` = BlockItem.useOn (not the overridden PASS)
            let result = self.block_use_on(&mut ctx, level, player);
            if ctx.item_stack != player.get_item_in_hand(hand) {
                player.set_item_in_hand(hand, ctx.item_stack);
            }
            return result;
        }
        consumable::base_use(level, player, hand)
    }

    /// `PlayerHeadItem.getName`.
    fn get_name(&self, stack: &ItemStack) -> mc_text::Component {
        let name = self.player_head.and_then(|id| {
            let profile = stack.get(&mc_protocol::components::PROFILE)?;
            let name = match &profile.contents {
                mc_protocol::codec::Either::Left(game_profile) => Some(game_profile.name.clone()),
                mc_protocol::codec::Either::Right(partial) => partial.name.clone(),
            }?;
            Some((id, name))
        });
        match name {
            Some((id, name)) => {
                mc_text::Component::translatable(format!("{id}.named"), vec![name.into()])
            }
            None => PlainItem.get_name(stack),
        }
    }

    fn can_fit_inside_container_items(&self) -> bool {
        !self.block.data().name.ends_with("shulker_box")
    }

    fn as_dispensible_container(&self) -> Option<&dyn DispensibleContainerItem> {
        matches!(self.kind, Kind::SolidBucket { .. }).then_some(self)
    }

    /// `BlockItem.onDestroyed(entity)`: container contents drop out.
    fn on_destroyed(
        &self,
        level: &mut ServerLevel,
        stack: &mut ItemStack,
        pos: mc_core::phys::Vec3,
    ) {
        let contents: Vec<ItemStack> = match stack.get(&CONTAINER) {
            Some(c) => c.non_empty_items().map(|t| t.create()).collect(),
            None => return,
        };
        stack.set(CONTAINER, ItemContainerContents::default());
        super::on_container_destroyed(level, pos, contents);
    }
}

/// `BucketItem.getEmptySuccessItem(stack, player)`.
pub fn bucket_empty_success_item(stack: &ItemStack, player: &ServerPlayer) -> ItemStack {
    if !player.player.abilities.instabuild {
        ItemStack::new(Item::BUCKET)
    } else {
        stack.clone()
    }
}

/// `Item.getPlayerPOVHitResult(level, player, fluid)`.
pub fn player_pov_hit_result(
    level: &ServerLevel,
    player: &ServerPlayer,
    source_only: bool,
) -> BlockHitResult {
    let fluid = if source_only {
        crate::world::level::collision::ClipFluid::SourceOnly
    } else {
        crate::world::level::collision::ClipFluid::None
    };
    player_pov_hit_result_fluid(level, player, fluid)
}

/// `Item.getPlayerPOVHitResult(level, player, fluid)` with any `ClipContext.Fluid` (`BoatItem`: `ANY`).
pub fn player_pov_hit_result_fluid(
    level: &ServerLevel,
    player: &ServerPlayer,
    fluid: crate::world::level::collision::ClipFluid,
) -> BlockHitResult {
    let from = player.base().get_eye_position();
    let (x_rot, y_rot) = (player.base().x_rot(), player.base().y_rot());
    let view = calculate_view_vector(x_rot, y_rot);
    let range = player
        .player
        .living
        .get_attribute_value(mc_data::Attribute::BLOCK_INTERACTION_RANGE);
    let to = from.add_xyz(view.x * range, view.y * range, view.z * range);
    level.clip(&ClipContext::new(
        from,
        to,
        crate::world::level::collision::ClipBlock::Outline,
        fluid,
        crate::world::entity::movement::collision_context(player),
    ))
}

/// `Entity.calculateViewVector(xRot, yRot)`.
pub fn calculate_view_vector(x_rot: f32, y_rot: f32) -> mc_core::phys::Vec3 {
    use mc_core::math::mth;
    let deg = std::f32::consts::PI / 180.0;
    let (xr, yr) = (x_rot * deg, -y_rot * deg);
    let (cos_y, sin_y) = (mth::cos(f64::from(yr)), mth::sin(f64::from(yr)));
    let (cos_x, sin_x) = (mth::cos(f64::from(xr)), mth::sin(f64::from(xr)));
    mc_core::phys::Vec3::new(
        f64::from(sin_y * cos_x),
        f64::from(-sin_x),
        f64::from(cos_y * cos_x),
    )
}

register_item_class!("BlockItem", |item| Box::new(BlockItem::for_item(item)));
register_item_class!("StandingAndWallBlockItem", |item| Box::new(
    BlockItem::for_item(item)
));
register_item_class!("PlayerHeadItem", |item| Box::new(
    BlockItem::for_item(item).player_head(item.data().description_id)
));
register_item_class!("DoubleHighBlockItem", |item| Box::new(
    BlockItem::with_kind(item, Kind::DoubleHigh)
));
register_item_class!("GameMasterBlockItem", |item| Box::new(
    BlockItem::with_kind(item, Kind::GameMaster)
));
register_item_class!("PlaceOnWaterBlockItem", |item| Box::new(
    BlockItem::with_kind(item, Kind::PlaceOnWater)
));
register_item_class!("ScaffoldingBlockItem", |item| Box::new(
    BlockItem::with_kind(item, Kind::Scaffolding)
));
register_item_class!("SolidBucketItem", |item| {
    // `Items.POWDER_SNOW_BUCKET`: `SoundEvents.BUCKET_EMPTY_POWDER_SNOW`
    Box::new(BlockItem::with_kind(
        item,
        Kind::SolidBucket {
            sound: SoundEvent::ITEM_BUCKET_EMPTY_POWDER_SNOW,
        },
    ))
});
