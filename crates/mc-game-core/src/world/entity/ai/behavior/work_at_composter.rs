//! `WorkAtComposter`: the farmer's job-site behavior (bakes bread, composts seeds).

use super::BehaviorBase;
use super::work_at_poi::WorkAtPoi;
use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::memory_module_type::JOB_SITE;
use crate::world::entity::npc::villager::villager_mut;
use crate::world::item::ItemStack;
use crate::world::level::BlockGetter;
use crate::world::level::block::impls::composter_block;
use mc_data::Item;
use mc_data::block::{Block, BlockState};
use mc_data::props;
use mc_protocol::codec::GlobalPos;

const COMPOSTABLE_ITEMS: [Item; 2] = [Item::WHEAT_SEEDS, Item::BEETROOT_SEEDS];

/// `new WorkAtComposter()`.
pub fn new() -> BehaviorBase<WorkAtPoi> {
    WorkAtPoi::with_workstation(use_workstation)
}

/// `useWorkstation(level, body)`.
fn use_workstation(level: &mut ServerLevel, body: &mut dyn Entity) {
    let Some(job_site_pos) = body.brain().get_memory(&JOB_SITE).cloned() else {
        return;
    };
    let state = level.get_block_state(job_site_pos.pos);
    if state.block() == Block::COMPOSTER {
        make_bread(level, body);
        compost_items(level, body, &job_site_pos, state);
    }
}

/// `compostItems(level, body, jobSitePos, blockState)`.
fn compost_items(
    level: &mut ServerLevel,
    body: &mut dyn Entity,
    job_site_pos: &GlobalPos,
    mut block_state: BlockState,
) {
    let pos = job_site_pos.pos;
    let source = body.id();
    if block_state.get(props::LEVEL_COMPOSTER) == 8 {
        block_state = level.expose(&*body, |level| {
            composter_block::extract_produce(Some(source), block_state, level, pos)
        });
    }
    let mut total_items_to_use = 20;
    let min_stack_size = 10;
    let mut items_seen_so_far = [0i32; COMPOSTABLE_ITEMS.len()];
    let container_size = villager_mut(body).av.inventory.get_container_size();
    let mut temp_state = block_state;
    let mut i = container_size as i32 - 1;
    while i >= 0 && total_items_to_use > 0 {
        let mut item_stack: ItemStack =
            villager_mut(body).av.inventory.get_item(i as usize).clone();
        if let Some(item_index) = COMPOSTABLE_ITEMS
            .iter()
            .position(|&it| it == item_stack.item())
        {
            let stack_size = item_stack.count();
            let total_item_count = items_seen_so_far[item_index] + stack_size;
            items_seen_so_far[item_index] = total_item_count;
            let items_to_use = (total_item_count - min_stack_size)
                .min(total_items_to_use)
                .min(stack_size);
            if items_to_use > 0 {
                total_items_to_use -= items_to_use;
                for _ in 0..items_to_use {
                    temp_state = composter_block::insert_item(
                        Some(&*body),
                        temp_state,
                        level,
                        &mut item_stack,
                        pos,
                    );
                    if temp_state.get(props::LEVEL_COMPOSTER) == 7 {
                        // the stack was shrunk in place: write it back to the slot it came from
                        villager_mut(body)
                            .av
                            .inventory
                            .set_item(i as usize, item_stack);
                        spawn_composter_fill_effects(level, block_state, pos, temp_state);
                        return;
                    }
                }
                villager_mut(body)
                    .av
                    .inventory
                    .set_item(i as usize, item_stack);
            }
        }
        i -= 1;
    }
    spawn_composter_fill_effects(level, block_state, pos, temp_state);
}

/// `spawnComposterFillEffects(level, blockState, pos, newState)`.
fn spawn_composter_fill_effects(
    level: &mut ServerLevel,
    block_state: BlockState,
    pos: mc_core::pos::BlockPos,
    new_state: BlockState,
) {
    use crate::world::level::LevelAccessor as _;
    level.level_event(None, 1500, pos, i32::from(new_state != block_state));
}

/// `makeBread(level, body)`.
fn make_bread(level: &mut ServerLevel, body: &mut dyn Entity) {
    let villager = villager_mut(body);
    if villager.av.inventory.count_item(Item::BREAD) <= 36 {
        let how_much_wheat_i_have = villager.av.inventory.count_item(Item::WHEAT);
        let how_much_bread_to_make = 3.min(how_much_wheat_i_have / 3);
        if how_much_bread_to_make != 0 {
            let how_much_wheat_to_use = how_much_bread_to_make * 3;
            villager
                .av
                .inventory
                .remove_item_type(Item::WHEAT, how_much_wheat_to_use);
            let bread_i_cant_carry = villager
                .av
                .inventory
                .add_item(&ItemStack::with_count(Item::BREAD, how_much_bread_to_make));
            if !bread_i_cant_carry.is_empty() {
                crate::world::entity::item::spawn_at_location(
                    level,
                    villager,
                    bread_i_cant_carry,
                    mc_core::phys::Vec3::new(0.0, 0.5, 0.0),
                );
            }
        }
    }
}
