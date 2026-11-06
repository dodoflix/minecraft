//! `HarvestFarmland`: the farmer harvests ripe crops and replants.

use super::position_tracker::{BlockPosTracker, PositionTracker};
use super::{Behavior, BehaviorBase, DEFAULT_DURATION};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::{
    LOOK_TARGET, SECONDARY_JOB_SITE, WALK_TARGET,
};
use crate::world::entity::npc::villager::villager_mut;
use crate::world::item::ItemStack;
use crate::world::level::block::impls::crop_block;
use crate::world::level::{BlockGetter, LevelAccessor as _};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::{GameEvent, Item, SoundEvent, VillagerProfession};
use mc_protocol::packets::play::level::SoundSource;

const HARVEST_DURATION: i32 = 200;
pub const SPEED_MODIFIER: f32 = 0.5;

pub struct HarvestFarmland {
    above_farmland_pos: Option<BlockPos>,
    next_ok_start_time: i64,
    time_worked_so_far: i32,
    valid_farmland_around_villager: Vec<BlockPos>,
}

impl HarvestFarmland {
    pub fn new() -> BehaviorBase<HarvestFarmland> {
        BehaviorBase::with_duration(
            entry_condition![LOOK_TARGET => ValueAbsent, WALK_TARGET => ValueAbsent, SECONDARY_JOB_SITE => ValuePresent],
            DEFAULT_DURATION,
            DEFAULT_DURATION,
            HarvestFarmland {
                above_farmland_pos: None,
                next_ok_start_time: 0,
                time_worked_so_far: 0,
                valid_farmland_around_villager: Vec::new(),
            },
        )
    }

    fn get_valid_farmland(&self, level: &mut ServerLevel) -> Option<BlockPos> {
        if self.valid_farmland_around_villager.is_empty() {
            None
        } else {
            Some(
                self.valid_farmland_around_villager[level
                    .random()
                    .next_int_bounded(self.valid_farmland_around_villager.len() as i32)
                    as usize],
            )
        }
    }
}

fn valid_pos(block_pos: BlockPos, level: &ServerLevel) -> bool {
    let state = level.get_block_state(block_pos);
    let block_below = level.get_block_state(block_pos.below()).block();
    crop_block::is_max_age(state) == Some(true) || state.is_air() && block_below == Block::FARMLAND
}

impl Behavior for HarvestFarmland {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        if !level.mob_griefing() {
            return false;
        }
        if villager_mut(body).get_villager_data().profession != VillagerProfession::FARMER {
            return false;
        }
        self.valid_farmland_around_villager.clear();
        let (bx, by, bz) = (body.base().x(), body.base().y(), body.base().z());
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let pos = BlockPos::containing(
                        bx + f64::from(x),
                        by + f64::from(y),
                        bz + f64::from(z),
                    );
                    if valid_pos(pos, level) {
                        self.valid_farmland_around_villager.push(pos);
                    }
                }
            }
        }
        self.above_farmland_pos = self.get_valid_farmland(level);
        self.above_farmland_pos.is_some()
    }

    fn start(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        if timestamp > self.next_ok_start_time {
            if let Some(pos) = self.above_farmland_pos {
                body.brain_mut().set_memory(
                    &LOOK_TARGET,
                    PositionTracker::Block(BlockPosTracker::new(pos)),
                );
                body.brain_mut().set_memory(
                    &WALK_TARGET,
                    WalkTarget::new(
                        PositionTracker::Block(BlockPosTracker::new(pos)),
                        SPEED_MODIFIER,
                        1,
                    ),
                );
            }
        }
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        body.brain_mut().erase_memory(&LOOK_TARGET);
        body.brain_mut().erase_memory(&WALK_TARGET);
        self.time_worked_so_far = 0;
        self.next_ok_start_time = timestamp + 40;
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        if self
            .above_farmland_pos
            .is_none_or(|p| p.closer_to_center_than(&body.position(), 1.0))
        {
            if let Some(pos) = self
                .above_farmland_pos
                .filter(|_| timestamp > self.next_ok_start_time)
            {
                let block_state = level.get_block_state(pos);
                let block_below = level.get_block_state(pos.below()).block();
                if crop_block::is_max_age(block_state) == Some(true) {
                    level.destroy_block_impl(pos, true, Some(body.id()), 512);
                }
                if block_state.is_air()
                    && block_below == Block::FARMLAND
                    && villager_mut(body).has_farm_seeds()
                {
                    let size = villager_mut(body).av.inventory.get_container_size();
                    let plantable = mc_data::tags::get::<Item>("villager_plantable_seeds");
                    for i in 0..size {
                        let mut item_stack: ItemStack =
                            villager_mut(body).av.inventory.get_item(i).clone();
                        let mut ok = false;
                        if !item_stack.is_empty()
                            && plantable.is_some_and(|t| t.contains(item_stack.item()))
                        {
                            if let Some(block) = item_stack.item().block() {
                                let place = block.default_state();
                                level.set_block_and_update(pos, place);
                                level.game_event_at_block(
                                    GameEvent::BLOCK_PLACE,
                                    pos,
                                    &crate::world::level::gameevent::GameEventContext::of(
                                        Some(body.id()),
                                        Some(place),
                                    ),
                                );
                                ok = true;
                            }
                        }
                        if ok {
                            level.play_sound(
                                None,
                                pos,
                                SoundEvent::ITEM_CROP_PLANT,
                                SoundSource::Blocks,
                                1.0,
                                1.0,
                            );
                            item_stack.shrink(1);
                            if item_stack.is_empty() {
                                item_stack = ItemStack::empty();
                            }
                            villager_mut(body).av.inventory.set_item(i, item_stack);
                            break;
                        }
                    }
                }
                if crop_block::is_max_age(block_state) == Some(false) {
                    self.valid_farmland_around_villager.retain(|p| *p != pos);
                    self.above_farmland_pos = self.get_valid_farmland(level);
                    if let Some(next) = self.above_farmland_pos {
                        self.next_ok_start_time = timestamp + 20;
                        body.brain_mut().set_memory(
                            &WALK_TARGET,
                            WalkTarget::new(
                                PositionTracker::Block(BlockPosTracker::new(next)),
                                SPEED_MODIFIER,
                                1,
                            ),
                        );
                        body.brain_mut().set_memory(
                            &LOOK_TARGET,
                            PositionTracker::Block(BlockPosTracker::new(next)),
                        );
                    }
                }
            }
            self.time_worked_so_far += 1;
        }
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        _body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        self.time_worked_so_far < HARVEST_DURATION
    }
}
