//! `Animal`: love mode, breeding, food interaction, walk target value, NBT.

use super::ageable;
use super::ext;
use crate::server::level::ServerLevel;
use crate::stats::stats::Stats;
use crate::world::entity::experience_orb::ExperienceOrb;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult};
use crate::world::level::LevelReader;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_nbt::CompoundTag;
use uuid::Uuid;

pub const PARENT_AGE_AFTER_BREEDING: i32 = 6000;

/// Fields of `Animal`.
#[derive(Default, Debug)]
pub struct AnimalData {
    pub in_love: i32,
    /// `loveCause` (`EntityReference<ServerPlayer>`).
    pub love_cause: Option<Uuid>,
}

fn an(e: &dyn Entity) -> &AnimalData {
    e.mob().and_then(|m| m.animal.as_ref()).expect("animal")
}

fn anm(e: &mut dyn Entity) -> &mut AnimalData {
    e.mob_mut().and_then(|m| m.animal.as_mut()).expect("animal")
}

/// `Animal.customServerAiStep(level)` before `super.customServerAiStep`.
pub fn custom_server_ai_step(e: &mut dyn Entity) {
    if ageable::get_age(e) != 0 {
        anm(e).in_love = 0;
    }
}

/// `Animal.aiStep()` after `super.aiStep()`.
pub fn ai_step_after(e: &mut dyn Entity, _level: &mut ServerLevel) {
    if ageable::get_age(e) != 0 {
        anm(e).in_love = 0;
    }
    if an(e).in_love > 0 {
        anm(e).in_love -= 1;
        if an(e).in_love % 10 == 0 {
            // `level.addParticle(HEART, ...)` is a no-op on the server, but the RNG draws happen
            let b = e.base_mut();
            let _xa = b.random.next_gaussian() * 0.02;
            let _ya = b.random.next_gaussian() * 0.02;
            let _za = b.random.next_gaussian() * 0.02;
            let _ = b.get_random_x(1.0);
            let _ = b.get_random_y();
            let _ = b.get_random_z(1.0);
        }
    }
}

/// `Animal.getWalkTargetValue(pos, level)`.
pub fn get_walk_target_value(pos: BlockPos, level: &dyn LevelReader) -> f32 {
    if level.get_block_state(pos.below()).block() == Block::GRASS_BLOCK {
        10.0
    } else {
        level.get_pathfinding_cost_from_light_levels(pos)
    }
}

/// `Animal.getBaseExperienceReward(level)`.
pub fn get_base_experience_reward(e: &mut dyn Entity) -> i32 {
    1 + e.base_mut().random.next_int_bounded(3)
}

/// `canFallInLove()`.
pub fn can_fall_in_love(e: &dyn Entity) -> bool {
    an(e).in_love <= 0
}

/// `setInLove(player)`.
pub fn set_in_love(e: &mut dyn Entity, level: &mut ServerLevel, player: Option<&dyn Entity>) {
    let a = anm(e);
    a.in_love = 600;
    if let Some(p) = player.filter(|p| p.is_player()) {
        a.love_cause = Some(p.uuid());
    }
    level.broadcast_entity_event(e.id(), 18);
}

/// `setInLoveTime(time)`.
pub fn set_in_love_time(e: &mut dyn Entity, time: i32) {
    anm(e).in_love = time;
}

/// `getInLoveTime()`.
pub fn get_in_love_time(e: &dyn Entity) -> i32 {
    an(e).in_love
}

/// `isInLove()`.
pub fn is_in_love(e: &dyn Entity) -> bool {
    an(e).in_love > 0
}

/// `resetLove()`.
pub fn reset_love(e: &mut dyn Entity) {
    anm(e).in_love = 0;
}

/// `getLoveCause()`: the player id when it is still in the level.
pub fn get_love_cause(e: &dyn Entity, level: &ServerLevel) -> Option<EntityId> {
    let uuid = an(e).love_cause?;
    level
        .get_entity_by_uuid(uuid)
        .filter(|p| p.is_player())
        .map(|p| p.id())
}

/// `canMate(partner)`: same class and both in love.
pub fn can_mate(e: &dyn Entity, partner: &dyn Entity) -> bool {
    if partner.id() == e.id() {
        return false;
    }
    if partner.as_any().type_id() != e.as_any().type_id() {
        return false;
    }
    is_in_love(e)
        && partner
            .mob()
            .is_some_and(|m| m.animal.as_ref().is_some_and(|a| a.in_love > 0))
}

/// `Animal.mobInteract(player, hand)`: feeding. `None` when the item is not food / nothing applies (`super.mobInteract`).
pub fn mob_interact(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> Option<InteractionResult> {
    let item_stack = player.get_item_in_hand(hand);
    if e.is_food(&item_stack) {
        let age = ageable::get_age(e);
        if age == 0 && e.can_fall_in_love() {
            e.use_player_item(level, player, hand);
            set_in_love(e, level, Some(&*player));
            e.play_eating_sound(level);
            return Some(InteractionResult::SUCCESS_SERVER);
        }
        if e.can_age_up() {
            e.use_player_item(level, player, hand);
            ageable::age_up(
                e,
                level,
                ageable::get_speed_up_seconds_when_feeding(-age),
                true,
            );
            e.play_eating_sound(level);
            return Some(InteractionResult::SUCCESS);
        }
    }
    None
}

/// `spawnChildFromBreeding(level, partner)`: the virtual call (`Frog` and `Sniffer` override it).
pub fn spawn_child_from_breeding(e: &mut dyn Entity, level: &mut ServerLevel, partner: EntityId) {
    e.spawn_child_from_breeding(level, partner);
}

/// `Animal.spawnChildFromBreeding(level, partner)` (the `super` call of the overrides).
pub fn spawn_child_from_breeding_super(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    partner: EntityId,
) {
    let Some(mut offspring) = e.get_breed_offspring(level, partner) else {
        return;
    };
    ageable::set_baby(&mut *offspring, true);
    let p = e.position();
    offspring.snap_to(p.x, p.y, p.z, 0.0, 0.0);
    finalize_spawn_child_from_breeding(e, level, partner, Some(&mut *offspring));
    level.add_fresh_entity_with_passengers(vec![offspring]);
}

/// `finalizeSpawnChildFromBreeding(level, partner, offspring)`.
pub fn finalize_spawn_child_from_breeding(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    partner: EntityId,
    offspring: Option<&mut dyn Entity>,
) {
    let cause = get_love_cause(e, level).or_else(|| {
        level
            .get_entity(partner)
            .and_then(|p| get_love_cause(p, level))
    });
    if let Some(cause) = cause {
        level.with_player(cause, |level, player| {
            player.award_stat(level, Stats::ANIMALS_BRED, 1);
        });
        ext::trigger_bred_animals(level, cause, e, partner, offspring.as_deref());
    }
    ageable::set_age_in_level(e, level, PARENT_AGE_AFTER_BREEDING);
    level.with_entity(partner, |level, p| {
        ageable::set_age_in_level(p, level, PARENT_AGE_AFTER_BREEDING)
    });
    reset_love(e);
    level.with_entity(partner, |_, p| reset_love(p));
    level.broadcast_entity_event(e.id(), 18);
    if level.mob_drops() {
        let p = e.position();
        let value = e.base_mut().random.next_int_bounded(7) + 1;
        let orb = ExperienceOrb::at(level, p.x, p.y, p.z, value);
        level.add_fresh_entity(Box::new(orb));
    }
}

/// `Animal.addAdditionalSaveData`.
pub fn add_additional_save_data(e: &dyn Entity, out: &mut CompoundTag) {
    out.put_int("InLove", an(e).in_love);
    if let Some(uuid) = an(e).love_cause {
        out.put_int_array(
            "LoveCause",
            mc_core::uuid_util::uuid_to_int_array(&uuid).to_vec(),
        );
    }
}

/// `Animal.readAdditionalSaveData`.
pub fn read_additional_save_data(e: &mut dyn Entity, input: &CompoundTag) {
    let a = anm(e);
    a.in_love = input.get_int_or("InLove", 0);
    a.love_cause = crate::world::entity::entity::read_uuid(input.get("LoveCause"));
}
