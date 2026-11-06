//! `ZombifiedPiglin` (a [`Zombie`](super::Zombie) of kind [`ZombieKind::ZombifiedPiglin`](super::ZombieKind::ZombifiedPiglin)):
//! a `NeutralMob` that alerts other zombified piglins.

use super::{Zombie, ZombieKind};
use crate::register_spawn_placement;
use crate::server::level::ServerLevel;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::living_entity::util::lm;
use crate::world::entity::mob::{self, NeutralData, ext};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::block::state::BlockStateExt;
use crate::world::level::pathfinder::PathType;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::ident::Identifier;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{Attribute, EntityType, Item};
use mc_world::level_storage::Difficulty;
use std::sync::LazyLock;

/// `SPEED_MODIFIER_ATTACKING_ID`.
static SPEED_MODIFIER_ATTACKING_ID: LazyLock<Identifier> =
    LazyLock::new(|| Identifier::new("minecraft", "attacking").expect("valid id"));

/// `FIRST_ANGER_SOUND_DELAY` (`TimeUtil.rangeOfSeconds(0, 1)`).
const FIRST_ANGER_SOUND_DELAY: (i32, i32) = (0, 20);
/// `PERSISTENT_ANGER_TIME` (`TimeUtil.rangeOfSeconds(20, 39)`).
const PERSISTENT_ANGER_TIME: (i32, i32) = (400, 780);
/// `ALERT_INTERVAL` (`TimeUtil.rangeOfSeconds(4, 6)`).
const ALERT_INTERVAL: (i32, i32) = (80, 120);

/// `UniformInt.sample(random)`.
fn sample(range: (i32, i32), random: &mut dyn RandomSource) -> i32 {
    mc_core::math::mth::random_between_inclusive(random, range.0, range.1)
}

/// The fields `ZombifiedPiglin` adds to `Zombie` (`persistentAngerEndTime`/`persistentAngerTarget` are the
/// [`NeutralData`] of the mob).
#[derive(Default)]
pub struct ZombifiedPiglinData {
    /// `playFirstAngerSoundIn`.
    pub play_first_anger_sound_in: i32,
    /// `ticksUntilNextAlert`.
    pub ticks_until_next_alert: i32,
}

fn data_mut(zombie: &mut Zombie) -> &mut ZombifiedPiglinData {
    match &mut zombie.kind {
        ZombieKind::ZombifiedPiglin(d) => d,
        _ => unreachable!("not a zombified piglin"),
    }
}

/// The body of the `ZombifiedPiglin` constructor after `Zombie`'s.
pub fn init(zombie: &mut Zombie) {
    zombie.mob.neutral = Some(NeutralData::default());
    zombie.mob.set_pathfinding_malus(PathType::Lava, 8.0);
}

/// `customServerAiStep(level)`.
pub fn custom_server_ai_step(zombie: &mut Zombie, level: &mut ServerLevel) {
    let angry = mob::neutral::is_angry(zombie, level);
    let is_baby = zombie.is_baby();
    if let Some(speed) = lm(zombie)
        .attributes
        .get_instance(Attribute::MOVEMENT_SPEED)
    {
        if angry {
            if !is_baby && !speed.has_modifier(&SPEED_MODIFIER_ATTACKING_ID) {
                speed.add_transient_modifier(AttributeModifier {
                    id: SPEED_MODIFIER_ATTACKING_ID.clone(),
                    amount: 0.05,
                    operation: AttributeModifierOperation::AddValue,
                });
            }
        } else if speed.has_modifier(&SPEED_MODIFIER_ATTACKING_ID) {
            speed.remove_modifier(&SPEED_MODIFIER_ATTACKING_ID);
        }
    }
    if angry {
        maybe_play_first_anger_sound(zombie, level);
    }
    mob::neutral::update_persistent_anger(zombie, level, true);
    if let Some(target) = mob::tick::get_target_checked(zombie, level) {
        maybe_alert_others(zombie, level, target);
    }
}

fn maybe_play_first_anger_sound(zombie: &mut Zombie, level: &mut ServerLevel) {
    let d = data_mut(zombie);
    if d.play_first_anger_sound_in > 0 {
        d.play_first_anger_sound_in -= 1;
        if d.play_first_anger_sound_in == 0 {
            play_anger_sound(zombie, level);
        }
    }
}

fn maybe_alert_others(zombie: &mut Zombie, level: &mut ServerLevel, target: EntityId) {
    if data_mut(zombie).ticks_until_next_alert > 0 {
        data_mut(zombie).ticks_until_next_alert -= 1;
    } else {
        let sees = level
            .get_entity(target)
            .is_some_and(|t| zombie.mob.sensing.has_line_of_sight(level, &*zombie, t));
        if sees {
            alert_others(zombie, level, target);
        }
        let ticks = sample(ALERT_INTERVAL, &mut zombie.base_mut().random);
        data_mut(zombie).ticks_until_next_alert = ticks;
    }
}

fn alert_others(zombie: &mut Zombie, level: &mut ServerLevel, target: EntityId) {
    let within = mob::attribute_value(zombie, Attribute::FOLLOW_RANGE);
    let search_aabb =
        AABB::unit_cube_from_lower_corner(zombie.position()).inflate(within, 10.0, within);
    let others = level.get_entities_of_class(
        &search_aabb,
        |e| e.get_type() == EntityType::ZOMBIFIED_PIGLIN,
        |e| !e.is_spectator(),
    );
    for other in others {
        let eligible = {
            let Some(other_entity) = level.get_entity(other) else {
                continue;
            };
            let Some(target_entity) = level.get_entity(target) else {
                continue;
            };
            mob::tick::get_target_checked(other_entity, level).is_none()
                && !ext::is_allied_to(level, other_entity, target_entity)
        };
        if eligible {
            level.with_entity(other, |level, o| o.set_target(&*level, Some(target)));
        }
    }
}

fn play_anger_sound(zombie: &mut Zombie, level: &mut ServerLevel) {
    let volume = zombie.get_sound_volume() * 2.0;
    let pitch = zombie.get_voice_pitch() * 1.8;
    zombie.play_sound(
        level,
        mc_data::SoundEvent::ENTITY_ZOMBIFIED_PIGLIN_ANGRY,
        volume,
        pitch,
    );
}

/// `setTarget(target)`.
pub fn set_target(zombie: &mut Zombie, level: &ServerLevel, target: Option<EntityId>) {
    if mob::tick::get_target_checked(zombie, level).is_none() && target.is_some() {
        let first = sample(FIRST_ANGER_SOUND_DELAY, &mut zombie.base_mut().random);
        data_mut(zombie).play_first_anger_sound_in = first;
        let alert = sample(ALERT_INTERVAL, &mut zombie.base_mut().random);
        data_mut(zombie).ticks_until_next_alert = alert;
    }
    mob::tick::set_target(zombie, level, target);
}

/// `startPersistentAngerTimer()`.
pub fn start_persistent_anger_timer(zombie: &mut Zombie, level: &mut ServerLevel) {
    let remaining = sample(PERSISTENT_ANGER_TIME, &mut zombie.base_mut().random);
    mob::neutral::set_time_to_remain_angry(zombie, level, i64::from(remaining));
}

/// `populateDefaultEquipmentSlots(random, difficulty)`.
pub fn populate_default_equipment_slots(
    zombie: &mut Zombie,
    random: &mut dyn RandomSource,
    _difficulty: &DifficultyInstance,
) {
    let item = if random.next_int_bounded(20) == 0 {
        Item::GOLDEN_SPEAR
    } else {
        Item::GOLDEN_SWORD
    };
    mob::equipment::set_item_slot_spawn(zombie, EquipmentSlot::MainHand, ItemStack::new(item));
}

/// `ZombifiedPiglin.checkZombifiedPiglinSpawnRules(type, level, spawnReason, pos, random)`.
pub fn check_zombified_piglin_spawn_rules(
    _ty: EntityType,
    level: &dyn ServerLevelAccessor,
    _reason: EntitySpawnReason,
    pos: BlockPos,
    _random: &mut dyn RandomSource,
) -> bool {
    level.get_difficulty() != Difficulty::Peaceful
        && !level
            .get_block_state(pos.below())
            .is_block(Block::NETHER_WART_BLOCK)
}

register_spawn_placement!(
    EntityType::ZOMBIFIED_PIGLIN,
    check_zombified_piglin_spawn_rules
);
