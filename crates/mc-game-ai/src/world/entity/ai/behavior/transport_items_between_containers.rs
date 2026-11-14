//! `TransportItemsBetweenContainers`: walk to a chest of the source type, take up to 16 items of its first stack, walk to a
//! chest of the destination type and put them down (the copper golem's behavior).
//!
//! The `Container` of a target is the chest block entity at `pos` or, for a double chest, the compound of the two halves
//! (`ChestBlock.getContainer(.., false)`); it is reached through [`TransportItemTarget::with_container`] because block
//! entities live in their chunk. The block entity itself is identified by its instance id (vanilla compares the object).
//! The chunk's block entities are walked in the order of the chunk's open hash map, ties of the distance keep the first.

use mc_core::direction::Direction;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::BlockState;
use mc_data::enums::ChestType;
use mc_data::props;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::chest_block_entity::ChestBlockEntity;
use mc_game_core::world::container::{Container, with_block_entity_containers};
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::behavior::behavior_utils::set_walk_and_look_target_memories_pos;
use mc_game_core::world::entity::ai::behavior::position_tracker::{
    BlockPosTracker, PositionTracker,
};
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::GlobalPosSet;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    IS_PANICKING, LOOK_TARGET, TRANSPORT_ITEMS_COOLDOWN_TICKS,
    UNREACHABLE_TRANSPORT_BLOCK_POSITIONS, VISITED_BLOCK_POSITIONS, WALK_TARGET,
};
use mc_game_core::world::entity::ai::navigation::NavClass;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::mob::{self, leashable, navigation};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::BlockGetter as _;
use mc_game_core::world::level::block::helpers::is_instance_of;
use mc_game_core::world::level::block::impls::chest_block;
use mc_game_core::world::level::block::state::BlockStateExt as _;
use mc_game_core::world::level::collision::{
    BlockGetterExt as _, ClipBlock, ClipContext, ClipFluid,
};
use mc_protocol::codec::GlobalPos;
use std::sync::Arc;

pub const TARGET_INTERACTION_TIME: i32 = 60;
const VISITED_POSITIONS_MEMORY_TIME: i64 = 6000;
const TRANSPORTED_ITEM_MAX_STACK_SIZE: i32 = 16;
const MAX_VISITED_POSITIONS: usize = 10;
const MAX_UNREACHABLE_POSITIONS: usize = 50;
const IDLE_COOLDOWN: i32 = 140;
const CLOSE_ENOUGH_TO_START_QUEUING_DISTANCE: f64 = 3.0;
const CLOSE_ENOUGH_TO_START_INTERACTING_WITH_TARGET_DISTANCE: f64 = 0.5;
const CLOSE_ENOUGH_TO_START_INTERACTING_WITH_TARGET_PATH_END_DISTANCE: f64 = 1.0;
const CLOSE_ENOUGH_TO_CONTINUE_INTERACTING_WITH_TARGET: f64 = 2.0;

/// `TransportItemsBetweenContainers.ContainerInteractionState`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContainerInteractionState {
    PickupItem,
    PickupNoItem,
    PlaceItem,
    PlaceNoItem,
}

/// `TransportItemsBetweenContainers.TransportItemState`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransportItemState {
    Travelling,
    Queuing,
    Interacting,
}

/// `TransportItemsBetweenContainers.TransportItemTarget(pos, container, blockEntity, state)`; the container is the block
/// entity at `pos` (`first`) or the compound of `first` and `second`.
#[derive(Clone, Debug)]
pub struct TransportItemTarget {
    pub pos: BlockPos,
    first: BlockPos,
    second: Option<BlockPos>,
    /// `BlockEntityData.instance_id` of `blockEntity`.
    block_entity: u64,
    pub state: BlockState,
}

impl TransportItemTarget {
    /// `tryCreatePossibleTarget(blockPos, level)`.
    pub fn try_create_possible_target(level: &ServerLevel, pos: BlockPos) -> Option<Self> {
        let block_entity = level.get_block_entity(pos)?;
        let state = block_entity.get_block_state();
        let (first, second) = if is_instance_of(state, "ChestBlock") {
            // `ChestBlock.getContainer(chestBlock, state, level, pos, false)`
            chest_block::combined_positions(chest_block::combine(level, state, pos, false))?
        } else {
            block_entity.as_container()?;
            (pos, None)
        };
        Some(Self {
            pos,
            first,
            second,
            block_entity: block_entity.base().instance_id,
            state,
        })
    }

    /// Runs `f` on the target's `Container`.
    pub fn with_container<R>(
        &self,
        level: &mut ServerLevel,
        f: impl FnOnce(&mut ServerLevel, &mut dyn Container) -> R,
    ) -> Option<R> {
        with_block_entity_containers(level, self.first, self.second, f)
    }

    /// The chest block entities of the container, `container1` first.
    pub fn block_entity_positions(&self) -> impl Iterator<Item = BlockPos> {
        std::iter::once(self.first).chain(self.second)
    }
}

/// `TransportItemsBetweenContainers.OnTargetReachedInteraction`: `(body, level, target, ticksSinceReachingTarget)`.
pub type OnTargetReachedInteraction =
    Arc<dyn Fn(&mut dyn Entity, &mut ServerLevel, &TransportItemTarget, i32) + Send + Sync>;
/// `Consumer<PathfinderMob> onStartTravelling`.
pub type OnStartTravelling = fn(&mut dyn Entity);
/// `Predicate<TransportItemTarget> shouldQueueForTarget`.
pub type ShouldQueueForTarget = fn(&ServerLevel, &TransportItemTarget) -> bool;

pub struct TransportItemsBetweenContainers {
    speed_modifier: f32,
    source_block_type: fn(BlockState) -> bool,
    destination_block_type: fn(BlockState) -> bool,
    horizontal_search_distance: i32,
    vertical_search_distance: i32,
    should_queue_for_target: ShouldQueueForTarget,
    on_start_travelling: OnStartTravelling,
    on_target_interaction_actions: Vec<(ContainerInteractionState, OnTargetReachedInteraction)>,
    target: Option<TransportItemTarget>,
    state: TransportItemState,
    interaction_state: Option<ContainerInteractionState>,
    ticks_since_reaching_target: i32,
}

/// How a `doReachedTargetInteraction` call resolves its four outcomes.
#[derive(Clone, Copy)]
enum Interaction {
    /// `startOnReachedTargetInteraction`: only records which interaction state applies.
    Record,
    /// `onReachedTarget` after 60 ticks: really pick up / put down.
    Perform,
}

impl TransportItemsBetweenContainers {
    /// `new TransportItemsBetweenContainers(speedModifier, sourceBlockType, destinationBlockType, horizontalSearchDistance,
    /// verticalSearchDistance, onTargetInteractionActions, onStartTravelling, shouldQueueForTarget)`.
    #[allow(
        clippy::too_many_arguments,
        reason = "the eight constructor arguments of the vanilla behavior"
    )]
    pub fn new(
        speed_modifier: f32,
        source_block_type: fn(BlockState) -> bool,
        destination_block_type: fn(BlockState) -> bool,
        horizontal_search_distance: i32,
        vertical_search_distance: i32,
        on_target_interaction_actions: Vec<(ContainerInteractionState, OnTargetReachedInteraction)>,
        on_start_travelling: OnStartTravelling,
        should_queue_for_target: ShouldQueueForTarget,
    ) -> BehaviorBase<TransportItemsBetweenContainers> {
        BehaviorBase::new(
            entry_condition![
                VISITED_BLOCK_POSITIONS => Registered,
                UNREACHABLE_TRANSPORT_BLOCK_POSITIONS => Registered,
                TRANSPORT_ITEMS_COOLDOWN_TICKS => ValueAbsent,
                IS_PANICKING => ValueAbsent,
            ],
            TransportItemsBetweenContainers {
                speed_modifier,
                source_block_type,
                destination_block_type,
                horizontal_search_distance,
                vertical_search_distance,
                should_queue_for_target,
                on_start_travelling,
                on_target_interaction_actions,
                target: None,
                state: TransportItemState::Travelling,
                interaction_state: None,
                ticks_since_reaching_target: 0,
            },
        )
    }

    /// `updateInvalidTarget(level, body)`.
    fn update_invalid_target(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) -> bool {
        if self.has_valid_target(level, body) {
            return false;
        }
        self.stop_targeting_current_target(level, body);
        match self.get_transport_target(level, body) {
            Some(target) => {
                let pos = target.pos;
                self.target = Some(target);
                self.on_start_travelling(body);
                self.set_visited_block_pos(level, body, pos);
            }
            None => self.enter_cooldown_after_no_matching_target_found(level, body),
        }
        true
    }

    /// `onQueuingForTarget(target, level, body)`.
    fn on_queuing_for_target(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: &TransportItemTarget,
    ) {
        if !self.is_another_mob_interacting_with_target(level, target) {
            self.resume_travelling(level, body);
        }
    }

    /// `onTravelToTarget(target, level, body)`.
    fn on_travel_to_target(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: &TransportItemTarget,
    ) {
        let center = self.get_center_pos(body);
        if self.is_within_target_distance(
            CLOSE_ENOUGH_TO_START_QUEUING_DISTANCE,
            target,
            level,
            body,
            center,
        ) && self.is_another_mob_interacting_with_target(level, target)
        {
            self.start_queuing(level, body);
        } else if self.is_within_target_distance(
            Self::get_interaction_range(body, level),
            target,
            level,
            body,
            self.get_center_pos(body),
        ) {
            self.start_on_reached_target_interaction(level, body, target);
        } else {
            self.walk_towards_target(body);
        }
    }

    fn get_center_pos(&self, body: &dyn Entity) -> Vec3 {
        Self::set_middle_y_position(body, body.position())
    }

    /// `onReachedTarget(target, level, body)`.
    fn on_reached_target(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: &TransportItemTarget,
    ) {
        if !self.is_within_target_distance(
            CLOSE_ENOUGH_TO_CONTINUE_INTERACTING_WITH_TARGET,
            target,
            level,
            body,
            self.get_center_pos(body),
        ) {
            self.on_start_travelling(body);
        } else {
            self.ticks_since_reaching_target += 1;
            self.on_target_interaction(level, body, target);
            if self.ticks_since_reaching_target >= TARGET_INTERACTION_TIME {
                self.do_reached_target_interaction(level, body, target, Interaction::Perform);
                self.on_start_travelling(body);
            }
        }
    }

    fn start_queuing(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        Self::stop_in_place(level, body);
        self.state = TransportItemState::Queuing;
    }

    fn resume_travelling(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity) {
        self.state = TransportItemState::Travelling;
        self.walk_towards_target(body);
    }

    fn walk_towards_target(&self, body: &mut dyn Entity) {
        if let Some(target) = &self.target {
            set_walk_and_look_target_memories_pos(body, target.pos, self.speed_modifier, 0);
        }
    }

    fn start_on_reached_target_interaction(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: &TransportItemTarget,
    ) {
        self.do_reached_target_interaction(level, body, target, Interaction::Record);
        self.state = TransportItemState::Interacting;
    }

    /// `onStartTravelling(body)`.
    fn on_start_travelling(&mut self, body: &mut dyn Entity) {
        (self.on_start_travelling)(body);
        self.state = TransportItemState::Travelling;
        self.interaction_state = None;
        self.ticks_since_reaching_target = 0;
    }

    /// `onTargetInteraction(target, body)`.
    fn on_target_interaction(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: &TransportItemTarget,
    ) {
        body.brain_mut().set_memory(
            &LOOK_TARGET,
            PositionTracker::Block(BlockPosTracker::new(target.pos)),
        );
        Self::stop_in_place(level, body);
        if let Some(state) = self.interaction_state
            && let Some((_, action)) = self
                .on_target_interaction_actions
                .iter()
                .find(|(s, _)| *s == state)
        {
            let action = Arc::clone(action);
            action(body, level, target, self.ticks_since_reaching_target);
        }
    }

    /// `doReachedTargetInteraction(body, container, onPickupSuccess, onPickupFailure, onPlaceSuccess, onPlaceFailure)`.
    fn do_reached_target_interaction(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: &TransportItemTarget,
        how: Interaction,
    ) {
        let outcome = if Self::is_picking_up_items(body) {
            // `matchesGettingItemsRequirement(container)`
            let non_empty = target
                .with_container(level, |_, c| !c.is_empty())
                .unwrap_or(false);
            if non_empty {
                ContainerInteractionState::PickupItem
            } else {
                ContainerInteractionState::PickupNoItem
            }
        } else {
            // `matchesLeavingItemsRequirement(body, container)`
            let hand = body.get_item_by_slot(EquipmentSlot::MainHand);
            let matches = target
                .with_container(level, |_, c| {
                    c.is_empty() || Self::has_item_matching_hand_item(&hand, c)
                })
                .unwrap_or(false);
            if matches {
                ContainerInteractionState::PlaceItem
            } else {
                ContainerInteractionState::PlaceNoItem
            }
        };
        match how {
            Interaction::Record => self.interaction_state = Some(outcome),
            Interaction::Perform => match outcome {
                ContainerInteractionState::PickupItem => self.pick_up_items(level, body, target),
                ContainerInteractionState::PlaceItem => self.put_down_item(level, body, target),
                ContainerInteractionState::PickupNoItem
                | ContainerInteractionState::PlaceNoItem => {
                    self.stop_targeting_current_target(level, body);
                }
            },
        }
    }

    /// `getTransportTarget(level, body)`.
    fn get_transport_target(
        &self,
        level: &ServerLevel,
        body: &dyn Entity,
    ) -> Option<TransportItemTarget> {
        let target_block_search_area = self.get_target_search_area(body);
        let visited_positions = Self::get_visited_positions(body);
        let unreachable_positions = Self::get_unreachable_positions(body);
        let center = ChunkPos::containing(&body.block_position());
        let radius = self.get_horizontal_search_distance(body).div_euclid(16) + 1;
        let body_position = body.position();
        let mut target = None;
        let mut closest_distance = f64::from(f32::MAX);
        for chunk_pos in ChunkPos::range_closed(&center, radius) {
            let Some(chunk) = level.chunk_now(chunk_pos.x, chunk_pos.z) else {
                continue;
            };
            for block_entity in chunk.block_entities.values() {
                if !block_entity.as_any().is::<ChestBlockEntity>() {
                    continue;
                }
                let pos = block_entity.get_block_pos();
                let distance = pos.dist_to_center_sqr(&body_position);
                if distance < closest_distance
                    && let Some(valid) = self.is_target_valid_to_pick(
                        body,
                        level,
                        pos,
                        &visited_positions,
                        &unreachable_positions,
                        &target_block_search_area,
                    )
                {
                    target = Some(valid);
                    closest_distance = distance;
                }
            }
        }
        target
    }

    /// `isTargetValidToPick(body, level, blockEntity, visitedPositions, unreachablePositions, targetBlockSearchArea)`.
    fn is_target_valid_to_pick(
        &self,
        body: &dyn Entity,
        level: &ServerLevel,
        block_pos: BlockPos,
        visited_positions: &GlobalPosSet,
        unreachable_positions: &GlobalPosSet,
        target_block_search_area: &AABB,
    ) -> Option<TransportItemTarget> {
        if !target_block_search_area.contains(
            f64::from(block_pos.x()),
            f64::from(block_pos.y()),
            f64::from(block_pos.z()),
        ) {
            return None;
        }
        let target = TransportItemTarget::try_create_possible_target(level, block_pos)?;
        let is_valid_target = self.is_wanted_block(body, target.state)
            && !Self::is_position_already_visited(
                visited_positions,
                unreachable_positions,
                &target,
                level,
            )
            && !Self::is_container_locked(level, &target);
        is_valid_target.then_some(target)
    }

    /// `isContainerLocked(target)`.
    fn is_container_locked(level: &ServerLevel, target: &TransportItemTarget) -> bool {
        level
            .get_block_entity(target.pos)
            .and_then(|be| be.as_any().downcast_ref::<ChestBlockEntity>())
            .is_some_and(|chest| chest.container.is_locked())
    }

    /// `hasValidTarget(level, body)`.
    fn has_valid_target(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) -> bool {
        let Some(target) = self.target.clone() else {
            return false;
        };
        let target_is_of_valid_type = self.is_wanted_block(&*body, target.state)
            && Self::target_has_not_changed(level, &target);
        if target_is_of_valid_type && !chest_block::is_chest_blocked_at(level, target.pos) {
            if self.state != TransportItemState::Travelling {
                return true;
            }
            if self.has_valid_travelling_path(level, &target, body) {
                return true;
            }
            self.mark_visited_block_pos_as_unreachable(level, body, target.pos);
        }
        false
    }

    /// `hasValidTravellingPath(level, target, body)`.
    fn has_valid_travelling_path(
        &self,
        level: &mut ServerLevel,
        target: &TransportItemTarget,
        body: &mut dyn Entity,
    ) -> bool {
        let path = match navigation::with_navigation_ref(&*body, level, |n| n.get_path()).flatten()
        {
            Some(path) => Some(path),
            None => navigation::create_path_pos(body, level, target.pos, 0),
        };
        let position_from_which_to_reach_target =
            Self::get_position_to_reach_target_from(path.as_ref(), &*body);
        let can_reach_target = self.is_within_target_distance(
            Self::get_interaction_range(&*body, level),
            target,
            level,
            &*body,
            position_from_which_to_reach_target,
        );
        let has_not_yet_created_path_to_target = path.is_none() && !can_reach_target;
        has_not_yet_created_path_to_target
            || Self::target_is_reachable_from_position(
                level,
                can_reach_target,
                position_from_which_to_reach_target,
                target,
                &*body,
            )
    }

    /// `getPositionToReachTargetFrom(path, body)`.
    fn get_position_to_reach_target_from(
        path: Option<&mc_game_core::world::level::pathfinder::Path>,
        body: &dyn Entity,
    ) -> Vec3 {
        let bottom_center = match path.and_then(|p| p.get_end_node()) {
            None => body.position(),
            Some(end) => Vec3::at_bottom_center_of(&end.as_block_pos()),
        };
        Self::set_middle_y_position(body, bottom_center)
    }

    /// `setMiddleYPosition(body, pos)`.
    fn set_middle_y_position(body: &dyn Entity, pos: Vec3) -> Vec3 {
        pos.add(Vec3::new(
            0.0,
            body.base().bounding_box().get_ysize() / 2.0,
            0.0,
        ))
    }

    /// `targetHasNotChanged(level, target)`: the same block entity object is still at the position.
    fn target_has_not_changed(level: &ServerLevel, target: &TransportItemTarget) -> bool {
        level
            .get_block_entity(target.pos)
            .is_some_and(|be| be.base().instance_id == target.block_entity)
    }

    /// `getConnectedTargets(target, level)`.
    fn get_connected_targets(
        target: &TransportItemTarget,
        level: &ServerLevel,
    ) -> Vec<TransportItemTarget> {
        let chest_type = target
            .state
            .try_get(props::CHEST_TYPE)
            .unwrap_or(ChestType::Single);
        if chest_type == ChestType::Single {
            return vec![target.clone()];
        }
        match TransportItemTarget::try_create_possible_target(
            level,
            chest_block::get_connected_block_pos(target.pos, target.state),
        ) {
            Some(connected) => vec![target.clone(), connected],
            None => vec![target.clone()],
        }
    }

    /// `getTargetSearchArea(mob)`.
    fn get_target_search_area(&self, mob: &dyn Entity) -> AABB {
        let horizontal = self.get_horizontal_search_distance(mob);
        AABB::from_block_pos(&mob.block_position()).inflate(
            f64::from(horizontal),
            f64::from(self.get_vertical_search_distance(mob)),
            f64::from(horizontal),
        )
    }

    fn get_horizontal_search_distance(&self, mob: &dyn Entity) -> i32 {
        if mob.base().is_passenger() {
            1
        } else {
            self.horizontal_search_distance
        }
    }

    fn get_vertical_search_distance(&self, mob: &dyn Entity) -> i32 {
        if mob.base().is_passenger() {
            1
        } else {
            self.vertical_search_distance
        }
    }

    fn get_visited_positions(mob: &dyn Entity) -> GlobalPosSet {
        mob.brain()
            .get_memory(&VISITED_BLOCK_POSITIONS)
            .cloned()
            .unwrap_or_default()
    }

    fn get_unreachable_positions(mob: &dyn Entity) -> GlobalPosSet {
        mob.brain()
            .get_memory(&UNREACHABLE_TRANSPORT_BLOCK_POSITIONS)
            .cloned()
            .unwrap_or_default()
    }

    fn global_pos(level: &ServerLevel, pos: BlockPos) -> GlobalPos {
        GlobalPos {
            dimension: level.dimension.clone(),
            pos,
        }
    }

    /// `isPositionAlreadyVisited(visitedPositions, unreachablePositions, target, level)`.
    fn is_position_already_visited(
        visited_positions: &GlobalPosSet,
        unreachable_positions: &GlobalPosSet,
        target: &TransportItemTarget,
        level: &ServerLevel,
    ) -> bool {
        Self::get_connected_targets(target, level)
            .iter()
            .map(|t| Self::global_pos(level, t.pos))
            .any(|pos| visited_positions.contains(&pos) || unreachable_positions.contains(&pos))
    }

    /// `hasFinishedPath(body)`.
    fn has_finished_path(body: &dyn Entity, level: &ServerLevel) -> bool {
        navigation::with_navigation_ref(body, level, |n| n.get_path())
            .flatten()
            .is_some_and(|p| p.is_done())
    }

    /// `setVisitedBlockPos(body, level, target)`.
    fn set_visited_block_pos(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: BlockPos,
    ) {
        let mut visited_positions = Self::get_visited_positions(body);
        visited_positions.insert(Self::global_pos(level, target));
        if visited_positions.len() > MAX_VISITED_POSITIONS {
            self.enter_cooldown_after_no_matching_target_found(level, body);
        } else {
            body.brain_mut().set_memory_with_expiry(
                &VISITED_BLOCK_POSITIONS,
                visited_positions,
                VISITED_POSITIONS_MEMORY_TIME,
            );
        }
    }

    /// `markVisitedBlockPosAsUnreachable(body, level, target)`.
    fn mark_visited_block_pos_as_unreachable(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: BlockPos,
    ) {
        let mut visited_positions = Self::get_visited_positions(body);
        visited_positions.remove(&Self::global_pos(level, target));
        let mut unreachable_positions = Self::get_unreachable_positions(body);
        unreachable_positions.insert(Self::global_pos(level, target));
        if unreachable_positions.len() > MAX_UNREACHABLE_POSITIONS {
            self.enter_cooldown_after_no_matching_target_found(level, body);
        } else {
            body.brain_mut().set_memory_with_expiry(
                &VISITED_BLOCK_POSITIONS,
                visited_positions,
                VISITED_POSITIONS_MEMORY_TIME,
            );
            body.brain_mut().set_memory_with_expiry(
                &UNREACHABLE_TRANSPORT_BLOCK_POSITIONS,
                unreachable_positions,
                VISITED_POSITIONS_MEMORY_TIME,
            );
        }
    }

    /// `isWantedBlock(mob, block)`.
    fn is_wanted_block(&self, mob: &dyn Entity, block: BlockState) -> bool {
        if Self::is_picking_up_items(mob) {
            (self.source_block_type)(block)
        } else {
            (self.destination_block_type)(block)
        }
    }

    /// `getInteractionRange(body)`.
    fn get_interaction_range(body: &dyn Entity, level: &ServerLevel) -> f64 {
        if Self::has_finished_path(body, level) {
            CLOSE_ENOUGH_TO_START_INTERACTING_WITH_TARGET_PATH_END_DISTANCE
        } else {
            CLOSE_ENOUGH_TO_START_INTERACTING_WITH_TARGET_DISTANCE
        }
    }

    /// `isWithinTargetDistance(distance, target, level, body, fromPos)`.
    fn is_within_target_distance(
        &self,
        distance: f64,
        target: &TransportItemTarget,
        level: &ServerLevel,
        body: &dyn Entity,
        from_pos: Vec3,
    ) -> bool {
        let bounding_box = body.base().bounding_box();
        let moved_bound_box = AABB::of_size(
            from_pos,
            bounding_box.get_xsize(),
            bounding_box.get_ysize(),
            bounding_box.get_zsize(),
        );
        target
            .state
            .get_collision_shape(level, target.pos)
            .bounds()
            .inflate(distance, 0.5, distance)
            .move_block_pos(&target.pos)
            .intersects_aabb(&moved_bound_box)
    }

    /// `targetIsReachableFromPosition(level, canReachTarget, pos, target, body)`.
    fn target_is_reachable_from_position(
        level: &ServerLevel,
        can_reach_target: bool,
        pos: Vec3,
        target: &TransportItemTarget,
        body: &dyn Entity,
    ) -> bool {
        can_reach_target && Self::can_see_any_target_side(target, level, body, pos)
    }

    /// `canSeeAnyTargetSide(target, level, body, eyePosition)`.
    fn can_see_any_target_side(
        target: &TransportItemTarget,
        level: &ServerLevel,
        body: &dyn Entity,
        eye_position: Vec3,
    ) -> bool {
        let center = Vec3::at_center_of(&target.pos);
        Direction::ALL.iter().any(|d| {
            let hit_target = center.add(Vec3::new(
                0.5 * f64::from(d.get_step_x()),
                0.5 * f64::from(d.get_step_y()),
                0.5 * f64::from(d.get_step_z()),
            ));
            let hit = level.clip(&ClipContext::new(
                eye_position,
                hit_target,
                ClipBlock::Collider,
                ClipFluid::None,
                mc_game_core::world::entity::movement::collision_context(body),
            ));
            !hit.miss && hit.block_pos == target.pos
        })
    }

    /// `isAnotherMobInteractingWithTarget(target, level)`.
    fn is_another_mob_interacting_with_target(
        &self,
        level: &ServerLevel,
        target: &TransportItemTarget,
    ) -> bool {
        Self::get_connected_targets(target, level)
            .iter()
            .any(|t| (self.should_queue_for_target)(level, t))
    }

    /// `isPickingUpItems(body)`.
    fn is_picking_up_items(body: &dyn Entity) -> bool {
        body.get_item_by_slot(EquipmentSlot::MainHand).is_empty()
    }

    /// `hasItemMatchingHandItem(body, container)`.
    fn has_item_matching_hand_item(hand: &ItemStack, container: &dyn Container) -> bool {
        (0..container.get_container_size())
            .any(|slot| ItemStack::is_same_item(container.get_item(slot), hand))
    }

    /// `pickUpItems(body, container)`.
    fn pick_up_items(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: &TransportItemTarget,
    ) {
        let stack = target
            .with_container(level, |_, c| {
                let picked = Self::pickup_item_from_container(c);
                c.set_changed();
                picked
            })
            .unwrap_or_else(ItemStack::empty);
        mob::equipment::set_item_slot(body, level, EquipmentSlot::MainHand, stack);
        mob::equipment::set_guaranteed_drop(body, EquipmentSlot::MainHand);
        self.clear_memories_after_matching_target_found(level, body);
    }

    /// `putDownItem(body, container)`.
    fn put_down_item(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: &TransportItemTarget,
    ) {
        let hand = body.get_item_by_slot(EquipmentSlot::MainHand);
        let (items_left_after_visiting_chest, hand_after) = target
            .with_container(level, |_, c| {
                let left = Self::add_items_to_container(hand, c);
                c.set_changed();
                left
            })
            .unwrap_or_else(|| (ItemStack::empty(), ItemStack::empty()));
        // vanilla mutates the stack object in the hand, so `setItemSlot` below compares the mutated stack with the new one
        if let Some(living) = body.living_mut() {
            living.equipment.set(EquipmentSlot::MainHand, hand_after);
        }
        let left_is_empty = items_left_after_visiting_chest.is_empty();
        mob::equipment::set_item_slot(
            body,
            level,
            EquipmentSlot::MainHand,
            items_left_after_visiting_chest,
        );
        if left_is_empty {
            self.clear_memories_after_matching_target_found(level, body);
        } else {
            self.stop_targeting_current_target(level, body);
        }
    }

    /// `pickupItemFromContainer(container)`.
    fn pickup_item_from_container(container: &mut dyn Container) -> ItemStack {
        for slot in 0..container.get_container_size() {
            let item_stack = container.get_item(slot);
            if !item_stack.is_empty() {
                let item_count = item_stack.count().min(TRANSPORTED_ITEM_MAX_STACK_SIZE);
                return container.remove_item(slot, item_count);
            }
        }
        ItemStack::empty()
    }

    /// `addItemsToContainer(body, container)`: `item_stack` is the stack in the golem's hand, mutated like vanilla's shared
    /// object (including `setCount(count - countThatCanBeAdded)`). Returns the stack left over and the hand stack as it
    /// is after the call (the object the container may have taken over).
    fn add_items_to_container(
        mut item_stack: ItemStack,
        container: &mut dyn Container,
    ) -> (ItemStack, ItemStack) {
        for slot in 0..container.get_container_size() {
            let container_item_stack = container.get_item(slot).clone();
            if container_item_stack.is_empty() {
                container.set_item(slot, item_stack.clone());
                return (ItemStack::empty(), item_stack);
            }
            if ItemStack::is_same_item_same_components(&container_item_stack, &item_stack)
                && container_item_stack.count() < container_item_stack.get_max_stack_size()
            {
                let count_that_can_be_added =
                    container_item_stack.get_max_stack_size() - container_item_stack.count();
                let count_to_add = count_that_can_be_added.min(item_stack.count());
                let mut updated = container_item_stack;
                updated.set_count(updated.count() + count_to_add);
                item_stack.set_count(item_stack.count() - count_that_can_be_added);
                container.set_item(slot, updated);
                if item_stack.is_empty() {
                    return (ItemStack::empty(), item_stack);
                }
            }
        }
        (item_stack.clone(), item_stack)
    }

    /// `stopTargetingCurrentTarget(body)`.
    fn stop_targeting_current_target(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        self.ticks_since_reaching_target = 0;
        self.target = None;
        navigation::stop(body, level);
        body.brain_mut().erase_memory(&WALK_TARGET);
    }

    /// `clearMemoriesAfterMatchingTargetFound(body)`.
    fn clear_memories_after_matching_target_found(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) {
        self.stop_targeting_current_target(level, body);
        body.brain_mut().erase_memory(&VISITED_BLOCK_POSITIONS);
        body.brain_mut()
            .erase_memory(&UNREACHABLE_TRANSPORT_BLOCK_POSITIONS);
    }

    /// `enterCooldownAfterNoMatchingTargetFound(body)`.
    fn enter_cooldown_after_no_matching_target_found(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) {
        self.stop_targeting_current_target(level, body);
        body.brain_mut()
            .set_memory(&TRANSPORT_ITEMS_COOLDOWN_TICKS, IDLE_COOLDOWN);
        body.brain_mut().erase_memory(&VISITED_BLOCK_POSITIONS);
        body.brain_mut()
            .erase_memory(&UNREACHABLE_TRANSPORT_BLOCK_POSITIONS);
    }

    /// `stopInPlace(mob)`.
    fn stop_in_place(level: &mut ServerLevel, mob: &mut dyn Entity) {
        navigation::stop(mob, level);
        mob::set_xxa(mob, 0.0);
        mob::set_yya(mob, 0.0);
        mob.set_speed(0.0);
        let movement = mob.base().delta_movement();
        mob.base_mut()
            .set_delta_movement(Vec3::new(0.0, movement.y, 0.0));
    }

    /// `GroundPathNavigation.setCanPathToTargetsBelowSurface(value)` of the body's navigation.
    fn set_can_path_to_targets_below_surface(
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        value: bool,
    ) {
        navigation::with_navigation(body, level, |nav, _, _| {
            if matches!(nav.data().class, NavClass::Ground | NavClass::WallClimber) {
                nav.data_mut().set_can_path_to_targets_below_surface(value);
            }
        });
    }
}

impl Behavior for TransportItemsBetweenContainers {
    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        Self::set_can_path_to_targets_below_surface(level, body, true);
    }

    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        !leashable::is_leashed(body)
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.brain()
            .get_memory(&TRANSPORT_ITEMS_COOLDOWN_TICKS)
            .is_none()
            && !body.is_panicking()
            && !leashable::is_leashed(body)
    }

    fn timed_out(&self, _timestamp: i64, _end_timestamp: i64) -> bool {
        false
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let updated_invalid_target = self.update_invalid_target(level, body);
        match self.target.clone() {
            None => self.stop(level, body, timestamp),
            Some(target) => {
                if !updated_invalid_target {
                    if self.state == TransportItemState::Queuing {
                        self.on_queuing_for_target(level, body, &target);
                    }
                    if self.state == TransportItemState::Travelling {
                        self.on_travel_to_target(level, body, &target);
                    }
                    if self.state == TransportItemState::Interacting {
                        self.on_reached_target(level, body, &target);
                    }
                }
            }
        }
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        self.on_start_travelling(body);
        Self::set_can_path_to_targets_below_surface(level, body, false);
    }
}
