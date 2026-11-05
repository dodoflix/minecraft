//! `GiveGiftToHero`: villagers throw gifts to a nearby Hero of the Village.

use mc_core::random::RandomSource;
use super::behavior_utils::{look_at_entity, set_walk_and_look_target_memories_entity, throw_item};
use super::{Behavior, BehaviorBase};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::EntityRef;
use crate::world::entity::ai::memory::memory_module_type::{INTERACTION_TARGET, LOOK_TARGET, NEAREST_VISIBLE_PLAYER, WALK_TARGET};
use crate::world::entity::mob::ext::drop_from_gift_loot_table;
use crate::world::entity::npc::villager::villager_mut;
use mc_core::ident::Identifier;
use mc_data::{MobEffect, VillagerProfession};

const THROW_GIFT_AT_DISTANCE: f64 = 5.0;
const MIN_TIME_BETWEEN_GIFTS: i32 = 600;
const MAX_TIME_BETWEEN_GIFTS: i32 = 6600;
const TIME_TO_DELAY_FOR_HEAD_TO_FINISH_TURNING: i64 = 20;
const SPEED_MODIFIER: f32 = 0.5;

pub struct GiveGiftToHero {
    time_until_next_gift: i32,
    gift_given_during_this_run: bool,
    time_since_start: i64,
}

impl GiveGiftToHero {
    pub fn new(timeout: i32) -> BehaviorBase<GiveGiftToHero> {
        BehaviorBase::with_timeout(
            entry_condition![WALK_TARGET => Registered, LOOK_TARGET => Registered, INTERACTION_TARGET => Registered, NEAREST_VISIBLE_PLAYER => ValuePresent],
            timeout,
            GiveGiftToHero { time_until_next_gift: MIN_TIME_BETWEEN_GIFTS, gift_given_during_this_run: false, time_since_start: 0 },
        )
    }
}

/// `getLootTableToThrow(villager)`.
fn get_loot_table_to_throw(villager: &mut dyn Entity) -> Identifier {
    let path = if villager.is_baby() {
        "gameplay/hero_of_the_village/baby_gift"
    } else {
        match villager_mut(villager).get_villager_data().profession {
            VillagerProfession::ARMORER => "gameplay/hero_of_the_village/armorer_gift",
            VillagerProfession::BUTCHER => "gameplay/hero_of_the_village/butcher_gift",
            VillagerProfession::CARTOGRAPHER => "gameplay/hero_of_the_village/cartographer_gift",
            VillagerProfession::CLERIC => "gameplay/hero_of_the_village/cleric_gift",
            VillagerProfession::FARMER => "gameplay/hero_of_the_village/farmer_gift",
            VillagerProfession::FISHERMAN => "gameplay/hero_of_the_village/fisherman_gift",
            VillagerProfession::FLETCHER => "gameplay/hero_of_the_village/fletcher_gift",
            VillagerProfession::LEATHERWORKER => "gameplay/hero_of_the_village/leatherworker_gift",
            VillagerProfession::LIBRARIAN => "gameplay/hero_of_the_village/librarian_gift",
            VillagerProfession::MASON => "gameplay/hero_of_the_village/mason_gift",
            VillagerProfession::SHEPHERD => "gameplay/hero_of_the_village/shepherd_gift",
            VillagerProfession::TOOLSMITH => "gameplay/hero_of_the_village/toolsmith_gift",
            VillagerProfession::WEAPONSMITH => "gameplay/hero_of_the_village/weaponsmith_gift",
            _ => "gameplay/hero_of_the_village/unemployed_gift",
        }
    };
    Identifier::parse(&format!("minecraft:{path}")).expect("valid loot table id")
}

/// `getNearestTargetableHero(body)`.
fn get_nearest_targetable_hero(level: &ServerLevel, body: &dyn Entity) -> Option<crate::world::entity::EntityId> {
    let EntityRef(id) = *body.brain().get_memory(&NEAREST_VISIBLE_PLAYER)?;
    level.get_entity(id).filter(|p| p.has_effect(MobEffect::HERO_OF_THE_VILLAGE)).map(|p| p.id())
}

impl Behavior for GiveGiftToHero {
    fn check_extra_start_conditions(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) -> bool {
        if get_nearest_targetable_hero(level, body).is_none() {
            false
        } else if self.time_until_next_gift > 0 {
            self.time_until_next_gift -= 1;
            false
        } else {
            true
        }
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.gift_given_during_this_run = false;
        self.time_since_start = timestamp;
        let Some(player) = get_nearest_targetable_hero(level, body) else { return };
        body.brain_mut().set_memory(&INTERACTION_TARGET, EntityRef(player));
        if let Some(p) = level.get_entity(player) {
            look_at_entity(body, p);
        }
    }

    fn can_still_use(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) -> bool {
        get_nearest_targetable_hero(level, body).is_some() && !self.gift_given_during_this_run
    }

    fn tick(&mut self, level: &mut ServerLevel, villager: &mut dyn Entity, timestamp: i64) {
        let Some(player_id) = get_nearest_targetable_hero(level, villager) else { return };
        let Some(player) = level.get_entity(player_id) else { return };
        look_at_entity(villager, player);
        let within = villager.block_position().closer_than(&player.block_position(), THROW_GIFT_AT_DISTANCE);
        if within {
            if timestamp - self.time_since_start > TIME_TO_DELAY_FOR_HEAD_TO_FINISH_TURNING {
                let target_pos = player.position();
                let table = get_loot_table_to_throw(villager);
                drop_from_gift_loot_table(level, villager, &table, |level, v, stack| throw_item(level, v, stack, target_pos));
                self.gift_given_during_this_run = true;
            }
        } else {
            set_walk_and_look_target_memories_entity(villager, player, SPEED_MODIFIER, 5);
        }
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        use crate::world::level::LevelAccessor as _;
        self.time_until_next_gift = MIN_TIME_BETWEEN_GIFTS + level.random().next_int_bounded(6001);
        let brain = body.brain_mut();
        brain.erase_memory(&INTERACTION_TARGET);
        brain.erase_memory(&WALK_TARGET);
        brain.erase_memory(&LOOK_TARGET);
    }
}
