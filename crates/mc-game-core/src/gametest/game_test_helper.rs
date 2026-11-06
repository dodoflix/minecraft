//! `GameTestHelper`: the API test functions run against (positions relative to the test structure, assertions,
//! block access, timing and sequences).
//!
//! The entity/mob/item/player/container parts of the vanilla helper (`spawn*`, `assertEntity*`, `useBlock`,
//! `makeMockPlayer`, `assertContainer*`, ...) are not ported: the only test functions of the vanilla jar are
//! `always_pass` and the block-based test, and those need none of them. They need the entity layer
//! (`EntitiesCore2`: spawning, `EntityType` queries, mock players).

use super::game_test_exception::GameTestException;
use super::game_test_info::{GameTestInfo, Runnable};
use super::game_test_sequence::GameTestSequence;
use crate::registry::RegistryAccess;
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::translatable;
use crate::world::block_entity::BlockEntity;
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::structure::templatesystem::structure_template::{
    transform, transform_vec3,
};
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::mirror::Mirror;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::rotation::Rotation;
use mc_data::block::{Block, BlockState};
use mc_text::Component;
use std::rc::Rc;

pub type TestResult = Result<(), GameTestException>;

pub struct GameTestHelper<'a> {
    server: &'a mut MinecraftServer,
    test_info: &'a Rc<GameTestInfo>,
}

/// `Block.getName()`.
fn block_name(block: Block) -> Component {
    Component::translatable(block.data().description_id, Vec::new())
}

impl<'a> GameTestHelper<'a> {
    pub fn new(server: &'a mut MinecraftServer, test_info: &'a Rc<GameTestInfo>) -> Self {
        Self { server, test_info }
    }

    pub fn info(&self) -> &Rc<GameTestInfo> {
        self.test_info
    }

    pub fn server(&mut self) -> &mut MinecraftServer {
        self.server
    }

    /// `getLevel().registryAccess()`.
    pub fn registries(&self) -> &RegistryAccess {
        self.server.registries.composite_access()
    }

    /// `getLevel()`.
    pub fn get_level(&mut self) -> &mut ServerLevel {
        self.server
            .level_mut(self.test_info.level())
            .expect("the level of a running game test exists")
    }

    // ---- exceptions ----

    /// `assertionException(description)`.
    pub fn assertion_exception(&self, description: Component) -> GameTestException {
        GameTestException::assert(description, self.test_info.get_tick())
    }

    /// `assertionException(pos, description)`.
    pub fn assertion_exception_at(
        &mut self,
        pos: BlockPos,
        description: Component,
    ) -> GameTestException {
        GameTestException::AssertPos {
            message: Box::new(description),
            absolute_pos: self.absolute_pos(pos),
            relative_pos: pos,
            tick: self.test_info.get_tick(),
        }
    }

    // ---- blocks ----

    /// `getBlockState(pos)`.
    pub fn get_block_state(&mut self, pos: BlockPos) -> BlockState {
        let absolute = self.absolute_pos(pos);
        self.get_level().get_block_state(absolute)
    }

    /// `getBlockEntity(pos, type)`: runs `f` on the block entity when it is a `T`.
    pub fn get_block_entity<T: BlockEntity, R>(
        &mut self,
        pos: BlockPos,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, GameTestException> {
        let absolute = self.absolute_pos(pos);
        let outcome = match self.get_level().get_block_entity_immediate(absolute) {
            None => None,
            Some(be) => {
                let type_name = be.get_type().name().to_owned();
                Some(match be.as_any_mut().downcast_mut::<T>() {
                    Some(typed) => Ok(f(typed)),
                    None => Err(type_name),
                })
            }
        };
        match outcome {
            None => {
                Err(self
                    .assertion_exception_at(pos, translatable!("test.error.missing_block_entity")))
            }
            Some(Ok(result)) => Ok(result),
            Some(Err(type_name)) => Err(self.assertion_exception_at(
                pos,
                translatable!("test.error.wrong_block_entity", type_name),
            )),
        }
    }

    /// `destroyBlock(pos)`.
    pub fn destroy_block(&mut self, pos: BlockPos) {
        let absolute = self.absolute_pos(pos);
        self.get_level().destroy_block(absolute, false, None, 512);
    }

    /// `setBlock(pos, state)`.
    pub fn set_block(&mut self, pos: BlockPos, state: BlockState) {
        let absolute = self.absolute_pos(pos);
        self.get_level().set_block_and_update(absolute, state);
    }

    /// `pulseRedstone(pos, duration)`.
    pub fn pulse_redstone(&mut self, pos: BlockPos, duration: i64) {
        self.set_block(pos, Block::REDSTONE_BLOCK.default_state());
        self.run_after_delay(duration, move |h| {
            h.set_block(pos, Block::AIR.default_state());
            Ok(())
        });
    }

    /// `assertBlockPresent(blockType, pos)`.
    pub fn assert_block_present(&mut self, block_type: Block, pos: BlockPos) -> TestResult {
        let state = self.get_block_state(pos);
        self.assert_block_state_matching(
            pos,
            |s| s.is_block(block_type),
            |_| {
                translatable!(
                    "test.error.expected_block",
                    block_name(block_type),
                    block_name(state.block())
                )
            },
        )
    }

    /// `assertBlockPresent(blockType)`: anywhere in the structure.
    pub fn assert_block_present_anywhere(&mut self, block_type: Block) -> TestResult {
        let aabb = self.get_relative_bounds().contract(1.0, 1.0, 1.0);
        let mut found = false;
        for pos in BlockPos::between_closed_aabb(&aabb) {
            if self.get_block_state(pos).is_block(block_type) {
                found = true;
                break;
            }
        }
        if !found {
            return Err(self.assertion_exception(translatable!(
                "test.error.expected_block_present",
                block_name(block_type)
            )));
        }
        Ok(())
    }

    /// `assertBlockNotPresent(blockType, pos)`.
    pub fn assert_block_not_present(&mut self, block_type: Block, pos: BlockPos) -> TestResult {
        if self.get_block_state(pos).is_block(block_type) {
            return Err(self.assertion_exception_at(
                pos,
                translatable!("test.error.unexpected_block", block_name(block_type)),
            ));
        }
        Ok(())
    }

    /// `assertBlockTag(tag, pos)`.
    pub fn assert_block_tag(&mut self, tag: &str, pos: BlockPos) -> TestResult {
        let state = self.get_block_state(pos);
        if state.is_tag(tag) {
            return Ok(());
        }
        Err(self.assertion_exception_at(
            pos,
            translatable!(
                "test.error.expected_block_tag",
                tag.to_owned(),
                block_name(state.block())
            ),
        ))
    }

    /// `succeedWhenBlockPresent(block, pos)`.
    pub fn succeed_when_block_present(&mut self, block: Block, pos: BlockPos) -> TestResult {
        self.succeed_when(move |h| h.assert_block_present(block, pos))
    }

    /// `assertBlockState(pos, expected)`.
    pub fn assert_block_state(&mut self, pos: BlockPos, expected: BlockState) -> TestResult {
        let block_state = self.get_block_state(pos);
        if block_state != expected {
            return Err(self.assertion_exception_at(
                pos,
                translatable!(
                    "test.error.state_not_equal",
                    expected.to_string(),
                    block_state.to_string()
                ),
            ));
        }
        Ok(())
    }

    /// `assertBlockState(pos, predicate, errorMessage)`.
    pub fn assert_block_state_matching(
        &mut self,
        pos: BlockPos,
        predicate: impl FnOnce(BlockState) -> bool,
        error_message: impl FnOnce(BlockState) -> Component,
    ) -> TestResult {
        let block_state = self.get_block_state(pos);
        if !predicate(block_state) {
            return Err(self.assertion_exception_at(pos, error_message(block_state)));
        }
        Ok(())
    }

    /// `assertBlock(pos, predicate, errorMessage)`.
    pub fn assert_block(
        &mut self,
        pos: BlockPos,
        predicate: impl FnOnce(Block) -> bool,
        error_message: impl FnOnce(Block) -> Component,
    ) -> TestResult {
        self.assert_block_state_matching(
            pos,
            |s| predicate(s.block()),
            |s| error_message(s.block()),
        )
    }

    /// `assertBlockEntityData(pos, type, predicate, errorMessage)`.
    pub fn assert_block_entity_data<T: BlockEntity>(
        &mut self,
        pos: BlockPos,
        predicate: impl FnOnce(&mut T) -> bool,
        error_message: impl FnOnce() -> Component,
    ) -> TestResult {
        if !self.get_block_entity::<T, bool>(pos, predicate)? {
            return Err(self.assertion_exception_at(pos, error_message()));
        }
        Ok(())
    }

    /// `assertRedstoneSignal(pos, direction, levelPredicate, errorMessage)`.
    pub fn assert_redstone_signal(
        &mut self,
        pos: BlockPos,
        direction: Direction,
        level_predicate: impl FnOnce(i32) -> bool,
        error_message: impl FnOnce() -> Component,
    ) -> TestResult {
        let block_pos = self.absolute_pos(pos);
        let level = self.get_level();
        let block_state = level.get_block_state(block_pos);
        let signal = block_state.get_signal(level, block_pos, direction);
        if !level_predicate(signal) {
            return Err(self.assertion_exception_at(pos, error_message()));
        }
        Ok(())
    }

    // ---- completion ----

    /// `succeed()`.
    pub fn succeed(&mut self) {
        self.test_info.succeed(self.server);
    }

    /// `ensureSingleFinalCheck()`.
    fn ensure_single_final_check(&self) -> TestResult {
        if self.test_info.final_check_added.get() {
            return Err(GameTestException::Unknown {
                reason: "This test already has final clause".to_owned(),
            });
        }
        self.test_info.final_check_added.set(true);
        Ok(())
    }

    /// `succeedIf(asserter)`.
    pub fn succeed_if(
        &mut self,
        asserter: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) -> TestResult {
        self.ensure_single_final_check()?;
        self.test_info
            .create_sequence()
            .then_wait_until_after(0, asserter)
            .then_succeed();
        Ok(())
    }

    /// `succeedWhen(asserter)`.
    pub fn succeed_when(
        &mut self,
        asserter: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) -> TestResult {
        self.ensure_single_final_check()?;
        self.test_info
            .create_sequence()
            .then_wait_until(asserter)
            .then_succeed();
        Ok(())
    }

    /// `succeedOnTickWhen(tick, asserter)`.
    pub fn succeed_on_tick_when(
        &mut self,
        tick: i32,
        asserter: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) -> TestResult {
        self.ensure_single_final_check()?;
        self.test_info
            .create_sequence()
            .then_wait_until_after(i64::from(tick), asserter)
            .then_succeed();
        Ok(())
    }

    /// `runAtTickTime(time, asserter)`.
    pub fn run_at_tick_time(
        &mut self,
        time: i64,
        asserter: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) {
        self.test_info.set_run_at_tick_time(time, Rc::new(asserter));
    }

    /// `runBeforeTestEnd(asserter)`.
    pub fn run_before_test_end(
        &mut self,
        asserter: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) {
        self.run_at_tick_time(i64::from(self.test_info.get_timeout_ticks()) - 1, asserter);
    }

    /// `runAfterDelay(ticksToDelay, whatToRun)`.
    pub fn run_after_delay(
        &mut self,
        ticks_to_delay: i64,
        what_to_run: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) {
        self.run_at_tick_time(
            i64::from(self.test_info.get_tick()) + ticks_to_delay,
            what_to_run,
        );
    }

    /// `randomTick(pos)`.
    pub fn random_tick(&mut self, pos: BlockPos) {
        let absolute = self.absolute_pos(pos);
        let level = self.get_level();
        level.get_block_state(absolute).random_tick(level, absolute);
    }

    /// `tickBlock(pos)`.
    pub fn tick_block(&mut self, pos: BlockPos) {
        let absolute = self.absolute_pos(pos);
        let level = self.get_level();
        level.get_block_state(absolute).tick(level, absolute);
    }

    /// `tickPrecipitation(pos)`.
    pub fn tick_precipitation_at(&mut self, pos: BlockPos) {
        let absolute = self.absolute_pos(pos);
        self.get_level().tick_precipitation(absolute);
    }

    /// `tickPrecipitation()`.
    pub fn tick_precipitation(&mut self) {
        let aabb = self.get_relative_bounds();
        let max_x = aabb.max_x.floor() as i32;
        let max_z = aabb.max_z.floor() as i32;
        let max_y = aabb.max_y.floor() as i32;
        for x in (aabb.min_x.floor() as i32)..max_x {
            for z in (aabb.min_z.floor() as i32)..max_z {
                self.tick_precipitation_at(BlockPos::new(x, max_y, z));
            }
        }
    }

    /// `getHeight(heightmap, x, z)`.
    pub fn get_height(
        &mut self,
        heightmap: mc_world::heightmap::HeightmapType,
        x: i32,
        z: i32,
    ) -> i32 {
        let absolute = self.absolute_pos(BlockPos::new(x, 0, z));
        let height_pos = self.get_level().get_heightmap_pos(heightmap, absolute);
        self.relative_pos(height_pos).y()
    }

    // ---- failing ----

    /// `fail(message, pos)`: always `Err`.
    pub fn fail_at(&mut self, message: Component, pos: BlockPos) -> TestResult {
        Err(self.assertion_exception_at(pos, message))
    }

    /// `fail(message)`: always `Err`.
    pub fn fail(&self, message: Component) -> TestResult {
        Err(self.assertion_exception(message))
    }

    /// `fail(String)`.
    pub fn fail_str(&self, message: &str) -> TestResult {
        self.fail(Component::literal(message))
    }

    /// `failIf(asserter)`.
    pub fn fail_if(&mut self, asserter: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static) {
        let tick_source = self.test_info.clone();
        self.test_info
            .create_sequence()
            .then_wait_until(asserter)
            .then_fail(move || {
                GameTestException::assert(translatable!("test.error.fail"), tick_source.get_tick())
            });
    }

    /// `failIfEver(asserter)`.
    pub fn fail_if_ever(
        &mut self,
        asserter: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) {
        let asserter: Runnable = Rc::new(asserter);
        for i in i64::from(self.test_info.get_tick())..i64::from(self.test_info.get_timeout_ticks())
        {
            self.test_info.set_run_at_tick_time(i, asserter.clone());
        }
    }

    /// `startSequence()`.
    pub fn start_sequence(&mut self) -> Rc<GameTestSequence> {
        self.test_info.create_sequence()
    }

    // ---- positions ----

    /// `absolutePos(relativePos)`.
    pub fn absolute_pos(&mut self, relative_pos: BlockPos) -> BlockPos {
        let test_pos = self.test_info.get_test_origin(self.server);
        let absolute_pos_before_transform = test_pos.offset_vec(&relative_pos);
        transform(
            absolute_pos_before_transform,
            Mirror::None,
            self.test_info.get_rotation(),
            test_pos,
        )
    }

    /// `relativePos(absolutePos)`.
    pub fn relative_pos(&mut self, absolute_pos: BlockPos) -> BlockPos {
        let test_pos = self.test_info.get_test_origin(self.server);
        let inverse_rotation = self
            .test_info
            .get_rotation()
            .get_rotated(Rotation::Clockwise180);
        let absolute_pos_before_transform =
            transform(absolute_pos, Mirror::None, inverse_rotation, test_pos);
        absolute_pos_before_transform.subtract(&test_pos)
    }

    /// `absoluteAABB(relativeAABB)`.
    pub fn absolute_aabb(&mut self, relative_aabb: &AABB) -> AABB {
        let min = self.absolute_vec(relative_aabb.get_min_position());
        let max = self.absolute_vec(relative_aabb.get_max_position());
        AABB::from_corners(min, max)
    }

    /// `relativeAABB(absoluteAABB)`.
    pub fn relative_aabb(&mut self, absolute_aabb: &AABB) -> AABB {
        let min = self.relative_vec(absolute_aabb.get_min_position());
        let max = self.relative_vec(absolute_aabb.get_max_position());
        AABB::from_corners(min, max)
    }

    /// `absoluteVec(relativeVec)`.
    pub fn absolute_vec(&mut self, relative_vec: Vec3) -> Vec3 {
        let origin = self.test_info.get_test_origin(self.server);
        let test_pos_vec = Vec3::new(
            f64::from(origin.x()),
            f64::from(origin.y()),
            f64::from(origin.z()),
        );
        transform_vec3(
            test_pos_vec.add(relative_vec),
            Mirror::None,
            self.test_info.get_rotation(),
            origin,
        )
    }

    /// `relativeVec(absoluteVec)`.
    pub fn relative_vec(&mut self, absolute_vec: Vec3) -> Vec3 {
        let origin = self.test_info.get_test_origin(self.server);
        let test_pos_vec = Vec3::new(
            f64::from(origin.x()),
            f64::from(origin.y()),
            f64::from(origin.z()),
        );
        transform_vec3(
            absolute_vec.subtract(test_pos_vec),
            Mirror::None,
            self.test_info.get_rotation(),
            origin,
        )
    }

    /// `getTestRotation()`.
    pub fn get_test_rotation(&self) -> Rotation {
        self.test_info.get_rotation()
    }

    /// `getTestDirection()`.
    pub fn get_test_direction(&self) -> Direction {
        self.test_info.get_rotation().rotate(Direction::South)
    }

    /// `getAbsoluteDirection(direction)`.
    pub fn get_absolute_direction(&self, direction: Direction) -> Direction {
        self.get_test_rotation().rotate(direction)
    }

    // ---- generic assertions ----

    /// `assertTrue(condition, errorMessage)`.
    pub fn assert_true(&self, condition: bool, error_message: Component) -> TestResult {
        if !condition {
            return Err(self.assertion_exception(error_message));
        }
        Ok(())
    }

    /// `assertFalse(condition, errorMessage)`.
    pub fn assert_false(&self, condition: bool, error_message: Component) -> TestResult {
        self.assert_true(!condition, error_message)
    }

    /// `assertValueEqual(value, expected, valueName)`.
    pub fn assert_value_equal<N: PartialEq + std::fmt::Display>(
        &self,
        value: N,
        expected: N,
        value_name: Component,
    ) -> TestResult {
        if value != expected {
            return Err(self.assertion_exception(translatable!(
                "test.error.value_not_equal",
                value_name,
                expected.to_string(),
                value.to_string()
            )));
        }
        Ok(())
    }

    /// `assertValueInBetween(lowerBound, value, upperBound, valueName)`.
    pub fn assert_value_in_between<N: PartialOrd + std::fmt::Display>(
        &self,
        lower_bound: N,
        value: N,
        upper_bound: N,
        value_name: Component,
    ) -> TestResult {
        if value < lower_bound || value > upper_bound {
            return Err(self.assertion_exception(translatable!(
                "test.error.value_not_in_between",
                value_name,
                lower_bound.to_string(),
                upper_bound.to_string(),
                value.to_string()
            )));
        }
        Ok(())
    }

    // ---- bounds ----

    /// `getTick()`.
    pub fn get_tick(&self) -> i64 {
        i64::from(self.test_info.get_tick())
    }

    /// `getBounds()`.
    pub fn get_bounds(&self) -> AABB {
        self.test_info.get_structure_bounds(self.server)
    }

    /// `getBoundsWithPadding()`.
    pub fn get_bounds_with_padding(&self) -> AABB {
        self.get_bounds()
            .inflate_scalar(f64::from(self.test_info.get_test().padding()))
    }

    /// `getRelativeBounds()`.
    pub fn get_relative_bounds(&self) -> AABB {
        let absolute = self.test_info.get_structure_bounds(self.server);
        match self.test_info.get_rotation() {
            Rotation::Counterclockwise90 | Rotation::Clockwise90 => AABB::new(
                0.0,
                0.0,
                0.0,
                absolute.get_zsize(),
                absolute.get_ysize(),
                absolute.get_xsize(),
            ),
            _ => AABB::new(
                0.0,
                0.0,
                0.0,
                absolute.get_xsize(),
                absolute.get_ysize(),
                absolute.get_zsize(),
            ),
        }
    }

    /// `forEveryBlockInStructure(forBlock)`.
    pub fn for_every_block_in_structure(&mut self, mut for_block: impl FnMut(&mut Self, BlockPos)) {
        let aabb = self.get_relative_bounds().contract(1.0, 1.0, 1.0);
        for pos in BlockPos::between_closed_aabb(&aabb) {
            for_block(self, pos);
        }
    }

    /// `onEachTick(action)`.
    pub fn on_each_tick(
        &mut self,
        action: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) {
        let action: Runnable = Rc::new(action);
        for i in i64::from(self.test_info.get_tick())..i64::from(self.test_info.get_timeout_ticks())
        {
            self.test_info.set_run_at_tick_time(i, action.clone());
        }
    }
}
