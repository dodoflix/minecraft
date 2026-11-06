//! `BoneMealItem`: growing `BonemealableBlock`s and seagrass/coral from water.

use super::{InteractionResult, ItemBehaviour, ItemStack, ItemStackExt, UseOnContext};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::level::block::{BlockStateExt, BonemealSource, behaviour as block_behaviour};
use crate::world::level::material::FluidStateExt;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::GameEvent;
use mc_data::block::Block;

/// `BoneMealItem.growCrop(itemStack, level, pos)`.
pub fn grow_crop(stack: &mut ItemStack, level: &mut ServerLevel, pos: BlockPos) -> bool {
    let state = level.get_block_state(pos);
    let behaviour = block_behaviour(state.block());
    if behaviour.is_bonemealable()
        && behaviour.is_valid_bonemeal_target(&*level, pos, state, BonemealSource::Interaction)
    {
        if behaviour.is_bonemeal_success(level, pos, state, BonemealSource::Interaction) {
            behaviour.perform_bonemeal(level, pos, state, BonemealSource::Interaction);
        }
        stack.shrink(1);
        true
    } else {
        false
    }
}

/// `registry.getRandomElementOf(tag, random)` over a block tag in file order.
fn random_block_of_tag(tag: &str, level: &mut ServerLevel) -> Option<Block> {
    let names = mc_data::tags::names("block", tag)?;
    if names.is_empty() {
        return None;
    }
    let index = LevelAccessor::random(level).next_int_bounded(names.len() as i32) as usize;
    Block::by_name(&names[index])
}

/// `BoneMealItem.growWaterPlant(itemStack, level, pos, clickedFace)`.
pub fn grow_water_plant(
    stack: &mut ItemStack,
    level: &mut ServerLevel,
    pos: BlockPos,
    clicked_face: Option<Direction>,
) -> bool {
    if !(level.get_block_state(pos).block() == Block::WATER && level.get_fluid_state(pos).is_full())
    {
        return false;
    }
    let corals_biomes = level.gen_env.as_ref().map(|env| {
        env.biomes.holder_set_from_json(&serde_json::json!(
            "#minecraft:produces_corals_from_bonemeal"
        ))
    });
    let wall_corals = mc_data::tags::get::<Block>("minecraft:wall_corals");
    'outer: for j in 0..128 {
        let mut test_pos = pos;
        let mut state_to_grow = Block::SEAGRASS.default_state();
        for _ in 0..(j / 16) {
            let random = LevelAccessor::random(level);
            let dx = random.next_int_bounded(3) - 1;
            let dy = (random.next_int_bounded(3) - 1) * random.next_int_bounded(3) / 2;
            let dz = random.next_int_bounded(3) - 1;
            test_pos = test_pos.offset(dx, dy, dz);
            if level
                .get_block_state(test_pos)
                .is_collision_shape_full_block_at(&*level, test_pos)
            {
                continue 'outer;
            }
        }
        let in_coral_biome = match (&corals_biomes, level.get_biome_id(test_pos)) {
            (Some(Ok(set)), Some(id)) => set.contains(&id),
            _ => false,
        };
        if in_coral_biome {
            if j == 0 && clicked_face.is_some_and(|f| f.get_axis().is_horizontal()) {
                if let Some(block) = random_block_of_tag("minecraft:wall_corals", level) {
                    state_to_grow = block.default_state();
                }
                if state_to_grow.has_property(mc_data::props::FACING) {
                    state_to_grow = state_to_grow
                        .set(mc_data::props::FACING, clicked_face.expect("checked above"));
                }
            } else if LevelAccessor::random(level).next_int_bounded(4) == 0 {
                if let Some(block) = random_block_of_tag("minecraft:underwater_bonemeals", level) {
                    state_to_grow = block.default_state();
                }
            }
        }
        if wall_corals.is_some_and(|t| t.contains(state_to_grow.block()))
            && state_to_grow.has_property(mc_data::props::FACING)
        {
            let mut d = 0;
            while !state_to_grow.can_survive(&*level, test_pos) && d < 4 {
                let direction = random_horizontal_direction(level);
                state_to_grow = state_to_grow.set(mc_data::props::FACING, direction);
                d += 1;
            }
        }
        if state_to_grow.can_survive(&*level, test_pos) {
            let test_state = level.get_block_state(test_pos);
            if test_state.block() == Block::WATER && level.get_fluid_state(test_pos).is_full() {
                level.set_block_and_update(test_pos, state_to_grow);
            } else if test_state.block() == Block::SEAGRASS {
                let seagrass = block_behaviour(Block::SEAGRASS);
                if seagrass.is_valid_bonemeal_target(
                    &*level,
                    test_pos,
                    test_state,
                    BonemealSource::Interaction,
                ) && LevelAccessor::random(level).next_int_bounded(10) == 0
                {
                    seagrass.perform_bonemeal(
                        level,
                        test_pos,
                        test_state,
                        BonemealSource::Interaction,
                    );
                }
            }
        }
    }
    stack.shrink(1);
    true
}

/// `Direction.Plane.HORIZONTAL.getRandomDirection(random)`.
fn random_horizontal_direction(level: &mut ServerLevel) -> Direction {
    const HORIZONTAL: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];
    HORIZONTAL[LevelAccessor::random(level).next_int_bounded(4) as usize]
}

pub struct BoneMealItem;

impl ItemBehaviour for BoneMealItem {
    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let pos = context.get_clicked_pos();
        let relative = pos.relative(context.get_clicked_face());
        if grow_crop(&mut context.item_stack, level, pos) {
            player.cause_use_vibration(level, &context.item_stack, GameEvent::ITEM_INTERACT_FINISH);
            level.level_event(None, 1505, pos, 15);
            return InteractionResult::SUCCESS_SERVER;
        }
        let clicked_state = level.get_block_state(pos);
        let solid_block_face =
            clicked_state.is_face_sturdy_at(&*level, pos, context.get_clicked_face());
        let clicked_face = context.get_clicked_face();
        if solid_block_face
            && grow_water_plant(&mut context.item_stack, level, relative, Some(clicked_face))
        {
            player.cause_use_vibration(level, &context.item_stack, GameEvent::ITEM_INTERACT_FINISH);
            level.level_event(None, 1505, relative, 15);
            return InteractionResult::SUCCESS;
        }
        InteractionResult::PASS
    }
}

register_item_class!("BoneMealItem", |_| Box::new(BoneMealItem));
