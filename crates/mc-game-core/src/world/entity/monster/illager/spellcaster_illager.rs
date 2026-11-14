//! `net.minecraft.world.entity.monster.illager.SpellcasterIllager`: spell state, the casting goal and the shared base of
//! the "use spell" goals of evokers and illusioners. (`SpellcasterIllager.tick`'s particles are client side only.)

use super::abstract_illager::IllagerArmPose;
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use crate::world::entity::mob::{self, navigation};
use crate::world::entity::raid::raider;
use crate::world::entity::synched_data::SynchedEntityDataBuilder;
use mc_data::SoundEvent;
use mc_nbt::CompoundTag;

define_accessor!(pub DATA_SPELL_CASTING_ID: Byte = SpellcasterIllager);

/// `SpellcasterIllager.IllagerSpell`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IllagerSpell {
    None,
    SummonVex,
    Fangs,
    Wololo,
    Disappear,
    Blindness,
}

impl IllagerSpell {
    pub fn id(self) -> i32 {
        self as i32
    }

    /// `IllagerSpell.byId(id)` (`ByIdMap.continuous` with `OutOfBoundsStrategy.ZERO`).
    pub fn by_id(id: i32) -> IllagerSpell {
        [
            IllagerSpell::None,
            IllagerSpell::SummonVex,
            IllagerSpell::Fangs,
            IllagerSpell::Wololo,
            IllagerSpell::Disappear,
            IllagerSpell::Blindness,
        ]
        .get(id as usize)
        .copied()
        .unwrap_or(IllagerSpell::None)
    }

    /// `spellColor` (client particles).
    pub fn spell_color(self) -> [f64; 3] {
        match self {
            IllagerSpell::None => [0.0, 0.0, 0.0],
            IllagerSpell::SummonVex => [0.7, 0.7, 0.8],
            IllagerSpell::Fangs => [0.4, 0.3, 0.35],
            IllagerSpell::Wololo => [0.7, 0.5, 0.2],
            IllagerSpell::Disappear => [0.3, 0.3, 0.8],
            IllagerSpell::Blindness => [0.1, 0.1, 0.2],
        }
    }
}

/// The fields of `SpellcasterIllager`; `casting_sound` is the class's `getCastingSoundEvent()`.
#[derive(Clone, Debug)]
pub struct SpellcasterData {
    pub spell_casting_tick_count: i32,
    pub current_spell: IllagerSpell,
    pub casting_sound: SoundEvent,
    /// `Evoker.wololoTarget` (the sheep the evoker looks at while casting wololo).
    pub wololo_target: Option<crate::world::entity::EntityId>,
}

impl SpellcasterData {
    pub fn new(casting_sound: SoundEvent) -> Self {
        SpellcasterData {
            spell_casting_tick_count: 0,
            current_spell: IllagerSpell::None,
            casting_sound,
            wololo_target: None,
        }
    }
}

/// `SpellcasterIllager.defineSynchedData` (after `Raider`'s).
pub fn define_synched_data(builder: &mut SynchedEntityDataBuilder) {
    builder.define(DATA_SPELL_CASTING_ID, 0);
}

fn data(e: &dyn Entity) -> &SpellcasterData {
    e.spellcaster().expect("spellcaster illager")
}

fn data_mut(e: &mut dyn Entity) -> &mut SpellcasterData {
    e.spellcaster_mut().expect("spellcaster illager")
}

/// `addAdditionalSaveData` of `SpellcasterIllager` (after the `Raider` chain).
pub fn add_additional_save_data(e: &dyn Entity, out: &mut CompoundTag) {
    raider::add_additional_save_data(e, out);
    out.put_int("SpellTicks", data(e).spell_casting_tick_count);
}

/// `readAdditionalSaveData` of `SpellcasterIllager`.
pub fn read_additional_save_data(
    e: &mut dyn Entity,
    level: Option<&ServerLevel>,
    input: &CompoundTag,
) {
    raider::read_additional_save_data(e, level, input);
    data_mut(e).spell_casting_tick_count = input.get_int_or("SpellTicks", 0);
}

/// `getArmPose()`.
pub fn get_arm_pose(e: &dyn Entity) -> IllagerArmPose {
    if is_casting_spell(e) {
        IllagerArmPose::Spellcasting
    } else if raider::is_celebrating(e) {
        IllagerArmPose::Celebrating
    } else {
        IllagerArmPose::Crossed
    }
}

/// `isCastingSpell()` (server side).
pub fn is_casting_spell(e: &dyn Entity) -> bool {
    data(e).spell_casting_tick_count > 0
}

/// `setIsCastingSpell(spell)`.
pub fn set_is_casting_spell(e: &mut dyn Entity, spell: IllagerSpell) {
    data_mut(e).current_spell = spell;
    e.base_mut()
        .entity_data
        .set(DATA_SPELL_CASTING_ID, spell.id() as i8);
}

/// `customServerAiStep(level)` (after `Mob`'s).
pub fn custom_server_ai_step(e: &mut dyn Entity) {
    let d = data_mut(e);
    if d.spell_casting_tick_count > 0 {
        d.spell_casting_tick_count -= 1;
    }
}

/// `SpellcasterIllager.SpellcasterCastingSpellGoal` (`EvokerCastingSpellGoal` also looks at its wololo target).
pub struct SpellcasterCastingSpellGoal {
    look_at_wololo_target: bool,
}

impl SpellcasterCastingSpellGoal {
    pub fn new() -> Self {
        SpellcasterCastingSpellGoal {
            look_at_wololo_target: false,
        }
    }

    /// `Evoker.EvokerCastingSpellGoal`.
    pub fn evoker() -> Self {
        SpellcasterCastingSpellGoal {
            look_at_wololo_target: true,
        }
    }
}

impl Default for SpellcasterCastingSpellGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for SpellcasterCastingSpellGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        data(mob).spell_casting_tick_count > 0
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::stop(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        set_is_casting_spell(mob, IllagerSpell::None);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let (max_y, max_x) = (
            mob.get_max_head_y_rot() as f32,
            mob.get_max_head_x_rot() as f32,
        );
        let target = mob::tick::get_target_checked(mob, level);
        if let Some(t) = target {
            if let Some(t) = level.get_entity(t) {
                mob::look_at_entity_full(mob, t, max_y, max_x);
            }
        } else if self.look_at_wololo_target
            && let Some(wololo) = data(mob).wololo_target
            && let Some(t) = level.get_entity_or_recently_removed(wololo)
        {
            mob::look_at_entity_full(mob, t, max_y, max_x);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
