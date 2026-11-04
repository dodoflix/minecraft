//! Code-defined data of the small registries: attributes, mob effects, potions, game events, sound events, particle
//! types, menu types, data component types, block entity types.

use crate::enums::{AttributeModifierOperation, AttributeSentiment, MobEffectCategory};
use crate::ids::{
    Attribute, Block, BlockEntityType, DataComponentType, GameEvent, MenuType, MobEffect, ParticleType, Potion, SoundEvent,
};

#[derive(Debug)]
pub struct AttributeData {
    pub description_id: &'static str,
    pub default_value: f64,
    /// Only `RangedAttribute`s.
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub syncable: bool,
    pub sentiment: AttributeSentiment,
}

#[derive(Debug)]
pub struct AttributeModifierTemplate {
    pub attribute: Attribute,
    pub id: &'static str,
    pub amount: f64,
    pub operation: AttributeModifierOperation,
}

#[derive(Debug)]
pub struct MobEffectData {
    pub class: &'static str,
    pub description_id: &'static str,
    pub category: MobEffectCategory,
    pub color: i32,
    pub instantaneous: bool,
    pub blend_in: i32,
    pub blend_out: i32,
    pub blend_out_advance: i32,
    pub sound_on_added: Option<SoundEvent>,
    pub required_features: &'static [&'static str],
    pub attribute_modifiers: &'static [AttributeModifierTemplate],
}

#[derive(Debug)]
pub struct MobEffectInstanceData {
    pub effect: MobEffect,
    pub duration: i32,
    pub amplifier: i32,
    pub ambient: bool,
    pub visible: bool,
    pub show_icon: bool,
}

#[derive(Debug)]
pub struct PotionData {
    pub base_name: &'static str,
    pub required_features: &'static [&'static str],
    pub effects: &'static [MobEffectInstanceData],
}

#[derive(Debug)]
pub struct GameEventData {
    pub notification_radius: i32,
}

#[derive(Debug)]
pub struct SoundEventData {
    pub fixed_range: Option<f32>,
}

#[derive(Debug)]
pub struct ParticleTypeData {
    pub class: &'static str,
    pub override_limiter: bool,
}

#[derive(Debug)]
pub struct MenuTypeData {
    pub required_features: &'static [&'static str],
}

#[derive(Debug)]
pub struct DataComponentTypeData {
    /// Name of the `DataComponents` constant (`MAX_STACK_SIZE`).
    pub const_name: Option<&'static str>,
    /// Has a codec (is saved to disk / NBT).
    pub persistent: bool,
    pub network_synchronized: bool,
    pub ignore_swap_animation: bool,
}

#[derive(Debug)]
pub struct BlockEntityTypeData {
    pub valid_blocks: &'static [Block],
    pub only_op_can_set_nbt: bool,
}

include!(concat!(env!("OUT_DIR"), "/registry_data.rs"));

macro_rules! data_accessor {
    ($ty:ident, $data:ident, $table:ident) => {
        impl $ty {
            pub fn data(self) -> &'static $data {
                &$table[self.0 as usize]
            }
        }
    };
}

data_accessor!(Attribute, AttributeData, ATTRIBUTE_DATA);
data_accessor!(MobEffect, MobEffectData, MOB_EFFECT_DATA);
data_accessor!(Potion, PotionData, POTION_DATA);
data_accessor!(GameEvent, GameEventData, GAME_EVENT_DATA);
data_accessor!(SoundEvent, SoundEventData, SOUND_EVENT_DATA);
data_accessor!(ParticleType, ParticleTypeData, PARTICLE_TYPE_DATA);
data_accessor!(MenuType, MenuTypeData, MENU_TYPE_DATA);
data_accessor!(DataComponentType, DataComponentTypeData, DATA_COMPONENT_TYPE_DATA);
data_accessor!(BlockEntityType, BlockEntityTypeData, BLOCK_ENTITY_TYPE_DATA);
