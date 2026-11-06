//! `SkeletonTrapGoal`: the lightning trap of a skeleton horse (a skeleton rider plus three more skeleton horsemen).

use super::skeleton_horse::SkeletonHorse;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{self, EntitySpawnRequest};
use mc_game_core::world::entity::lightning_bolt::LightningBolt;
use mc_game_core::world::entity::mob::{self, ageable};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_protocol::components::ENCHANTMENTS;

const LIGHTNING_INVULNERABLE_TICKS: i32 = 60;

/// `SkeletonTrapGoal`. Vanilla adds/removes the goal with the horse's trap flag; here it stays registered and is inert while
/// the flag is off (the removal would have to run while the selector is ticking this very goal).
pub struct SkeletonTrapGoal;

impl SkeletonTrapGoal {
    /// `createHorse(difficulty)`.
    fn create_horse(
        &self,
        horse: &mut dyn Entity,
        level: &mut ServerLevel,
        difficulty: DifficultyInstance,
    ) -> Option<Box<dyn Entity>> {
        let mut other = factory::create(
            EntityType::SKELETON_HORSE,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Triggered,
                ignore_checks: false,
            },
        )?;
        other.finalize_spawn(level, difficulty, EntitySpawnReason::Triggered, None);
        let p = horse.position();
        other.set_pos(p.x, p.y, p.z);
        other.base_mut().invulnerable_time = LIGHTNING_INVULNERABLE_TICKS;
        other.mob_mut()?.persistence_required = true;
        if let Some(h) = other.as_abstract_horse_mut() {
            h.set_tamed(true);
        }
        ageable::set_age_in_level(other.as_mut(), level, 0);
        Some(other)
    }

    /// `createSkeleton(difficulty, horse)`.
    fn create_skeleton(
        &self,
        level: &mut ServerLevel,
        difficulty: DifficultyInstance,
        horse: &dyn Entity,
    ) -> Option<Box<dyn Entity>> {
        let mut skeleton = factory::create(
            EntityType::SKELETON,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Triggered,
                ignore_checks: false,
            },
        )?;
        skeleton.finalize_spawn(level, difficulty, EntitySpawnReason::Triggered, None);
        let p = horse.position();
        skeleton.set_pos(p.x, p.y, p.z);
        skeleton.base_mut().invulnerable_time = LIGHTNING_INVULNERABLE_TICKS;
        skeleton.mob_mut()?.persistence_required = true;
        if skeleton.get_item_by_slot(EquipmentSlot::Head).is_empty() {
            mob::equipment::set_item_slot_spawn(
                skeleton.as_mut(),
                EquipmentSlot::Head,
                ItemStack::new(Item::IRON_HELMET),
            );
        }
        self.enchant(
            skeleton.as_mut(),
            level,
            EquipmentSlot::MainHand,
            &difficulty,
        );
        self.enchant(skeleton.as_mut(), level, EquipmentSlot::Head, &difficulty);
        Some(skeleton)
    }

    /// `enchant(skeleton, slot, difficulty)`.
    fn enchant(
        &self,
        skeleton: &mut dyn Entity,
        level: &ServerLevel,
        slot: EquipmentSlot,
        difficulty: &DifficultyInstance,
    ) {
        let mut stack = skeleton.get_item_by_slot(slot);
        stack.set(ENCHANTMENTS, Default::default());
        let registries = level.registries.clone();
        mc_game_core::world::item::enchantment_helper::enchant_item_from_provider(
            &mut stack,
            &registries,
            &mc_core::ident::Identifier::new("minecraft", "mob_spawn_equipment").expect("valid id"),
            difficulty,
            &mut skeleton.base_mut().random,
        );
        mob::equipment::set_item_slot_spawn(skeleton, slot, stack);
    }
}

impl Goal for SkeletonTrapGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !mob
            .as_any()
            .downcast_ref::<SkeletonHorse>()
            .is_some_and(SkeletonHorse::is_trap)
        {
            return false;
        }
        let p = mob.position();
        level
            .get_nearest_player(p.x, p.y, p.z, 10.0, |player| {
                !player.is_spectator() && player.is_alive()
            })
            .is_some()
    }

    fn tick(&mut self, horse: &mut dyn Entity, level: &mut ServerLevel) {
        let difficulty =
            ServerLevelAccessor::get_current_difficulty_at(&*level, horse.block_position());
        if let Some(h) = horse.as_any_mut().downcast_mut::<SkeletonHorse>() {
            h.set_trap(false);
        }
        if let Some(h) = horse.as_abstract_horse_mut() {
            h.set_tamed(true);
        }
        ageable::set_age_in_level(horse, level, 0);
        let Some(mut bolt) = factory::create(
            EntityType::LIGHTNING_BOLT,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Triggered,
                ignore_checks: false,
            },
        ) else {
            return;
        };
        let p = horse.position();
        bolt.snap_to(p.x, p.y, p.z, 0.0, 0.0);
        if let Some(b) = bolt.as_any_mut().downcast_mut::<LightningBolt>() {
            b.set_visual_only(true);
        }
        level.add_fresh_entity(bolt);
        let Some(mut skeleton) = self.create_skeleton(level, difficulty, horse) else {
            return;
        };
        level.start_riding_held(skeleton.as_mut(), horse, false, true);
        level.add_fresh_entity_with_passengers(vec![skeleton]);
        for _ in 0..3 {
            let Some(mut other_horse) = self.create_horse(horse, level, difficulty) else {
                continue;
            };
            let Some(mut other_skeleton) =
                self.create_skeleton(level, difficulty, other_horse.as_ref())
            else {
                continue;
            };
            level.start_riding_held(other_skeleton.as_mut(), other_horse.as_mut(), false, true);
            let dx = horse.base_mut().random.triangle(0.0, 1.1485);
            let dz = horse.base_mut().random.triangle(0.0, 1.1485);
            other_horse.push_xyz(dx, 0.0, dz);
            level.add_fresh_entity_with_passengers(vec![other_horse, other_skeleton]);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::default()
    }
}
