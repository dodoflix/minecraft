//! `PiglinSpecificSensor`, `PiglinBruteSpecificSensor` and `HoglinSpecificSensor`.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::*;
use crate::world::entity::ai::memory::{EntityRef, MemoryTypeId, NearestVisibleLivingEntities};
use crate::world::entity::entity::Entity;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::item::ItemStack;
use crate::world::level::level_access::BlockGetter as _;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::{EntityType, Item};

fn item_tag(tag: &str, stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}

/// `PiglinAi.isWearingSafeArmor(entity)`.
pub fn is_wearing_safe_armor(entity: &dyn Entity) -> bool {
    [
        EquipmentSlot::Feet,
        EquipmentSlot::Legs,
        EquipmentSlot::Chest,
        EquipmentSlot::Head,
    ]
    .into_iter()
    .any(|slot| item_tag("piglin_safe_armor", &entity.get_item_by_slot(slot)))
}

/// `PiglinAi.isLovedItem(stack)`.
pub fn is_loved_item(stack: &ItemStack) -> bool {
    item_tag("piglin_loved", stack)
}

/// `PiglinAi.isPlayerHoldingLovedItem(entity)`: `entity.is(PLAYER) && entity.isHolding(PiglinAi::isLovedItem)`.
pub fn is_player_holding_loved_item(entity: &dyn Entity) -> bool {
    entity.get_type() == EntityType::PLAYER
        && [EquipmentSlot::MainHand, EquipmentSlot::OffHand]
            .into_iter()
            .any(|slot| is_loved_item(&entity.get_item_by_slot(slot)))
}

/// `PiglinAi.isZombified(entity)`.
pub fn is_zombified(entity: &dyn Entity) -> bool {
    matches!(
        entity.get_type(),
        EntityType::ZOMBIFIED_PIGLIN | EntityType::ZOGLIN
    )
}

fn is_abstract_piglin(t: EntityType) -> bool {
    matches!(t, EntityType::PIGLIN | EntityType::PIGLIN_BRUTE)
}

/// `PiglinAi.findNearbyAdultPiglins(brain)`.
pub fn find_nearby_adult_piglins(level: &ServerLevel, body: &dyn Entity) -> Vec<EntityRef> {
    let Some(entities) = body.brain().get_memory(&NEAREST_LIVING_ENTITIES) else {
        return Vec::new();
    };
    entities
        .iter()
        .copied()
        .filter(|r| {
            level
                .get_entity(r.0)
                .is_some_and(|e| is_abstract_piglin(e.get_type()) && !e.is_baby())
        })
        .collect()
}

fn visible_entities(body: &dyn Entity) -> NearestVisibleLivingEntities {
    body.brain()
        .get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES)
        .cloned()
        .unwrap_or_else(NearestVisibleLivingEntities::empty)
}

/// `level.findBlocksInBoxByManhattanDistance(pos, 8, 4).filterState(test).findFirst()`.
fn find_nearest_block(
    level: &ServerLevel,
    origin: BlockPos,
    test: impl Fn(mc_data::block::BlockState) -> bool,
) -> Option<BlockPos> {
    BlockPos::within_box_by_manhattan_distance(&origin, 8, 4, 8)
        .find(|p| test(level.get_block_state(*p)))
}

fn is_valid_piglin_repellent(state: mc_data::block::BlockState) -> bool {
    let is_repellent =
        mc_data::tags::get::<Block>("piglin_repellents").is_some_and(|t| t.contains(state.block()));
    if is_repellent && state.block() == Block::SOUL_CAMPFIRE {
        crate::world::level::block::impls::campfire_block::CampfireBlock::is_lit_campfire(state)
    } else {
        is_repellent
    }
}

pub struct PiglinSpecificSensor;

impl Sensor for PiglinSpecificSensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![
            NEAREST_VISIBLE_LIVING_ENTITIES.id(),
            NEAREST_LIVING_ENTITIES.id(),
            NEAREST_VISIBLE_NEMESIS.id(),
            NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD.id(),
            NEAREST_PLAYER_HOLDING_WANTED_ITEM.id(),
            NEAREST_VISIBLE_HUNTABLE_HOGLIN.id(),
            NEAREST_VISIBLE_BABY_HOGLIN.id(),
            NEAREST_VISIBLE_ADULT_PIGLINS.id(),
            NEARBY_ADULT_PIGLINS.id(),
            VISIBLE_ADULT_PIGLIN_COUNT.id(),
            VISIBLE_ADULT_HOGLIN_COUNT.id(),
            NEAREST_REPELLENT.id(),
        ]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let repellent = find_nearest_block(level, body.block_position(), is_valid_piglin_repellent);
        body.brain_mut()
            .set_memory_opt(&NEAREST_REPELLENT, repellent);
        let (mut nemesis, mut huntable_hoglin, mut baby_hoglin, mut baby_piglin, mut zombified): (
            Option<EntityRef>,
            Option<EntityRef>,
            Option<EntityRef>,
            Option<EntityRef>,
            Option<EntityRef>,
        ) = (None, None, None, None, None);
        let (mut player_not_wearing_gold, mut player_holding_wanted_item) =
            (None::<EntityRef>, None::<EntityRef>);
        let mut visible_adult_hoglin_count = 0i32;
        let mut visible_adult_piglins: Vec<EntityRef> = Vec::new();
        let visible = visible_entities(&*body);
        for r in visible.find_all(level, &*body, |_| true) {
            let Some(entity) = level.get_entity(r.0) else {
                continue;
            };
            let t = entity.get_type();
            if t == EntityType::HOGLIN {
                if entity.is_baby() && baby_hoglin.is_none() {
                    baby_hoglin = Some(r);
                } else if !entity.is_baby() {
                    visible_adult_hoglin_count += 1;
                    if huntable_hoglin.is_none() && entity.can_be_hunted() {
                        huntable_hoglin = Some(r);
                    }
                }
            } else if t == EntityType::PIGLIN_BRUTE {
                visible_adult_piglins.push(r);
            } else if t == EntityType::PIGLIN {
                if entity.is_baby() && baby_piglin.is_none() {
                    baby_piglin = Some(r);
                } else if !entity.is_baby() {
                    visible_adult_piglins.push(r);
                }
            } else if t == EntityType::PLAYER {
                if player_not_wearing_gold.is_none()
                    && !is_wearing_safe_armor(entity)
                    && body.can_attack(level, entity)
                {
                    player_not_wearing_gold = Some(r);
                }
                if player_holding_wanted_item.is_none()
                    && !entity.is_spectator()
                    && is_player_holding_loved_item(entity)
                {
                    player_holding_wanted_item = Some(r);
                }
            } else if !nemesis.is_none()
                || !matches!(t, EntityType::WITHER_SKELETON | EntityType::WITHER)
            {
                if zombified.is_none() && is_zombified(entity) {
                    zombified = Some(r);
                }
            } else {
                nemesis = Some(r);
            }
        }
        let adult_piglins = find_nearby_adult_piglins(level, &*body);
        let brain = body.brain_mut();
        brain.set_memory_opt(&NEAREST_VISIBLE_NEMESIS, nemesis);
        brain.set_memory_opt(&NEAREST_VISIBLE_HUNTABLE_HOGLIN, huntable_hoglin);
        brain.set_memory_opt(&NEAREST_VISIBLE_BABY_HOGLIN, baby_hoglin);
        brain.set_memory_opt(&NEAREST_VISIBLE_ZOMBIFIED, zombified);
        brain.set_memory_opt(
            &NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD,
            player_not_wearing_gold,
        );
        brain.set_memory_opt(
            &NEAREST_PLAYER_HOLDING_WANTED_ITEM,
            player_holding_wanted_item,
        );
        brain.set_memory(&NEARBY_ADULT_PIGLINS, adult_piglins);
        let count = visible_adult_piglins.len() as i32;
        brain.set_memory(&NEAREST_VISIBLE_ADULT_PIGLINS, visible_adult_piglins);
        brain.set_memory(&VISIBLE_ADULT_PIGLIN_COUNT, count);
        brain.set_memory(&VISIBLE_ADULT_HOGLIN_COUNT, visible_adult_hoglin_count);
    }
}

pub struct PiglinBruteSpecificSensor;

impl Sensor for PiglinBruteSpecificSensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![
            NEAREST_VISIBLE_LIVING_ENTITIES.id(),
            NEAREST_VISIBLE_NEMESIS.id(),
            NEARBY_ADULT_PIGLINS.id(),
        ]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let visible = visible_entities(&*body);
        let nemesis = visible.find_closest(level, &*body, |e| {
            matches!(
                e.get_type(),
                EntityType::WITHER_SKELETON | EntityType::WITHER
            )
        });
        let adult_piglins = find_nearby_adult_piglins(level, &*body);
        let brain = body.brain_mut();
        brain.set_memory_opt(&NEAREST_VISIBLE_NEMESIS, nemesis);
        brain.set_memory(&NEARBY_ADULT_PIGLINS, adult_piglins);
    }
}

pub struct HoglinSpecificSensor;

impl Sensor for HoglinSpecificSensor {
    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![
            NEAREST_VISIBLE_LIVING_ENTITIES.id(),
            NEAREST_REPELLENT.id(),
            NEAREST_VISIBLE_ADULT_PIGLIN.id(),
            NEAREST_VISIBLE_ADULT_HOGLINS.id(),
            VISIBLE_ADULT_PIGLIN_COUNT.id(),
            VISIBLE_ADULT_HOGLIN_COUNT.id(),
        ]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let repellent = find_nearest_block(level, body.block_position(), |s| {
            mc_data::tags::get::<Block>("hoglin_repellents").is_some_and(|t| t.contains(s.block()))
        });
        body.brain_mut()
            .set_memory_opt(&NEAREST_REPELLENT, repellent);
        let mut adult_piglin: Option<EntityRef> = None;
        let mut adult_piglin_count = 0i32;
        let mut adult_hoglins: Vec<EntityRef> = Vec::new();
        let visible = visible_entities(&*body);
        let matching = visible.find_all(level, &*body, |e| {
            !e.is_baby() && matches!(e.get_type(), EntityType::PIGLIN | EntityType::HOGLIN)
        });
        for r in matching {
            match level.get_entity(r.0).map(|e| e.get_type()) {
                Some(EntityType::PIGLIN) => {
                    adult_piglin_count += 1;
                    adult_piglin.get_or_insert(r);
                }
                Some(EntityType::HOGLIN) => adult_hoglins.push(r),
                _ => {}
            }
        }
        let brain = body.brain_mut();
        brain.set_memory_opt(&NEAREST_VISIBLE_ADULT_PIGLIN, adult_piglin);
        let hoglin_count = adult_hoglins.len() as i32;
        brain.set_memory(&NEAREST_VISIBLE_ADULT_HOGLINS, adult_hoglins);
        brain.set_memory(&VISIBLE_ADULT_PIGLIN_COUNT, adult_piglin_count);
        brain.set_memory(&VISIBLE_ADULT_HOGLIN_COUNT, hoglin_count);
    }
}
