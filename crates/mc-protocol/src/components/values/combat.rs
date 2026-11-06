//! Combat-flavoured components: tools, weapons, blocking, swing animations, equipment.

use super::effects::{
    dec_opt, dec_sound_opt, enc_opt, enc_sound_opt, feq, float_field_default, ident,
};
use crate::codec::EncodeResult;
use crate::codec::{DecodeResult, read_varint, write_varint};
use crate::components::SoundEvent;
use crate::components::common::RegistrySet;
use crate::components::component::{ComponentDef, ComponentKey};
use crate::components::holder::{
    self, Holder, HolderSet, NoDirect, holder_fixed_codec, holder_set_codec,
};
use crate::packets::play::{EquipmentSlot, SwingAnimation, SwingAnimationType};
use crate::{Decode, Encode};
use mc_core::ident::Identifier;
use mc_data::{Block, DataComponentType, EntityType};
use mc_dfu::extra::{
    IDENTIFIER, NON_NEGATIVE_FLOAT, NON_NEGATIVE_INT, POSITIVE_FLOAT, extra_float_range,
};
use mc_dfu::{BOOL, Codec, FLOAT, float_range, string_enum_codec};

pub const TOOL: ComponentKey<Tool> = ComponentKey::new(DataComponentType::TOOL);
pub const WEAPON: ComponentKey<Weapon> = ComponentKey::new(DataComponentType::WEAPON);
pub const ATTACK_RANGE: ComponentKey<AttackRange> =
    ComponentKey::new(DataComponentType::ATTACK_RANGE);
pub const BLOCKS_ATTACKS: ComponentKey<BlocksAttacks> =
    ComponentKey::new(DataComponentType::BLOCKS_ATTACKS);
pub const KINETIC_WEAPON: ComponentKey<KineticWeapon> =
    ComponentKey::new(DataComponentType::KINETIC_WEAPON);
pub const PIERCING_WEAPON: ComponentKey<PiercingWeapon> =
    ComponentKey::new(DataComponentType::PIERCING_WEAPON);
pub const ATTACK_ANIMATION: ComponentKey<SwingAnimation> =
    ComponentKey::new(DataComponentType::ATTACK_ANIMATION);
pub const INTERACT_ANIMATION: ComponentKey<SwingAnimation> =
    ComponentKey::new(DataComponentType::INTERACT_ANIMATION);
pub const DAMAGE_RESISTANT: ComponentKey<DamageResistant> =
    ComponentKey::new(DataComponentType::DAMAGE_RESISTANT);
pub const EQUIPPABLE: ComponentKey<Equippable> = ComponentKey::new(DataComponentType::EQUIPPABLE);

const DAMAGE_TYPE: &str = "minecraft:damage_type";

/// `RegistryCodecs.holderSet(Registries.DAMAGE_TYPE)`.
fn damage_type_set_codec() -> Codec<HolderSet<NoDirect>> {
    holder_set_codec(
        DAMAGE_TYPE,
        &holder_fixed_codec::<NoDirect>(DAMAGE_TYPE),
        false,
    )
}
fn enc_damage_types(set: &HolderSet<NoDirect>, out: &mut Vec<u8>) -> EncodeResult<()> {
    holder::encode_holder_set(DAMAGE_TYPE, set, out)
}
fn dec_damage_types(input: &mut &[u8]) -> DecodeResult<HolderSet<NoDirect>> {
    holder::decode_holder_set(DAMAGE_TYPE, input)
}

// ---- Tool ---------------------------------------------------------------------------------------------------------------

/// `Tool.Rule`.
#[derive(Clone, Debug)]
pub struct ToolRule {
    pub blocks: RegistrySet<Block>,
    pub speed: Option<f32>,
    pub correct_for_drops: Option<bool>,
}
impl PartialEq for ToolRule {
    fn eq(&self, o: &Self) -> bool {
        self.blocks == o.blocks
            && match (self.speed, o.speed) {
                (Some(a), Some(b)) => feq(a, b),
                (a, b) => a.is_none() && b.is_none(),
            }
            && self.correct_for_drops == o.correct_for_drops
    }
}
impl ToolRule {
    pub fn codec() -> Codec<ToolRule> {
        mc_dfu::record!(ToolRule {
            blocks: RegistrySet::<Block>::codec().field_of("blocks"),
            speed: POSITIVE_FLOAT.optional_field_of("speed"),
            correct_for_drops: BOOL.optional_field_of("correct_for_drops"),
        })
        .codec()
    }
}
impl Encode for ToolRule {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.blocks.encode_stream(out)?;
        self.speed.encode(out)?;
        self.correct_for_drops.encode(out)?;
        Ok(())
    }
}
impl Decode for ToolRule {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(ToolRule {
            blocks: RegistrySet::decode_stream(input)?,
            speed: Decode::decode(input)?,
            correct_for_drops: Decode::decode(input)?,
        })
    }
}

/// `Tool`.
#[derive(Clone, Debug)]
pub struct Tool {
    pub rules: Vec<ToolRule>,
    pub default_mining_speed: f32,
    pub damage_per_block: i32,
    pub can_destroy_blocks_in_creative: bool,
}
impl PartialEq for Tool {
    fn eq(&self, o: &Self) -> bool {
        self.rules == o.rules
            && feq(self.default_mining_speed, o.default_mining_speed)
            && self.damage_per_block == o.damage_per_block
            && self.can_destroy_blocks_in_creative == o.can_destroy_blocks_in_creative
    }
}
impl Tool {
    pub fn codec() -> Codec<Tool> {
        mc_dfu::record!(Tool {
            rules: ToolRule::codec().list_of().field_of("rules"),
            default_mining_speed: float_field_default(&FLOAT, "default_mining_speed", 1.0),
            damage_per_block: NON_NEGATIVE_INT.optional_field_of_default("damage_per_block", 1),
            can_destroy_blocks_in_creative: BOOL
                .optional_field_of_default("can_destroy_blocks_in_creative", true),
        })
        .codec()
    }
}
impl Encode for Tool {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.rules.encode(out)?;
        self.default_mining_speed.encode(out)?;
        write_varint(out, self.damage_per_block);
        self.can_destroy_blocks_in_creative.encode(out)?;
        Ok(())
    }
}
impl Decode for Tool {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Tool {
            rules: Vec::decode(input)?,
            default_mining_speed: f32::decode(input)?,
            damage_per_block: read_varint(input)?,
            can_destroy_blocks_in_creative: bool::decode(input)?,
        })
    }
}

// ---- Weapon / AttackRange -------------------------------------------------------------------------------------------------

/// `Weapon`.
#[derive(Clone, Debug, Encode, Decode)]
pub struct Weapon {
    #[mc(var)]
    pub item_damage_per_attack: i32,
    pub disable_blocking_for_seconds: f32,
}
impl PartialEq for Weapon {
    fn eq(&self, o: &Self) -> bool {
        self.item_damage_per_attack == o.item_damage_per_attack
            && feq(
                self.disable_blocking_for_seconds,
                o.disable_blocking_for_seconds,
            )
    }
}
impl Weapon {
    pub fn codec() -> Codec<Weapon> {
        mc_dfu::record!(Weapon {
            item_damage_per_attack: NON_NEGATIVE_INT
                .optional_field_of_default("item_damage_per_attack", 1),
            disable_blocking_for_seconds: float_field_default(
                &NON_NEGATIVE_FLOAT,
                "disable_blocking_for_seconds",
                0.0
            ),
        })
        .codec()
    }
}

/// `AttackRange`.
#[derive(Clone, Debug, Encode, Decode)]
pub struct AttackRange {
    pub min_reach: f32,
    pub max_reach: f32,
    pub min_creative_reach: f32,
    pub max_creative_reach: f32,
    pub hitbox_margin: f32,
    pub mob_factor: f32,
}
impl PartialEq for AttackRange {
    fn eq(&self, o: &Self) -> bool {
        feq(self.min_reach, o.min_reach)
            && feq(self.max_reach, o.max_reach)
            && feq(self.min_creative_reach, o.min_creative_reach)
            && feq(self.max_creative_reach, o.max_creative_reach)
            && feq(self.hitbox_margin, o.hitbox_margin)
            && feq(self.mob_factor, o.mob_factor)
    }
}
impl AttackRange {
    pub fn codec() -> Codec<AttackRange> {
        let reach = extra_float_range(0.0, 64.0);
        mc_dfu::record!(AttackRange {
            min_reach: float_field_default(&reach, "min_reach", 0.0),
            max_reach: float_field_default(&reach, "max_reach", 3.0),
            min_creative_reach: float_field_default(&reach, "min_creative_reach", 0.0),
            max_creative_reach: float_field_default(&reach, "max_creative_reach", 5.0),
            hitbox_margin: float_field_default(&extra_float_range(0.0, 1.0), "hitbox_margin", 0.3),
            mob_factor: float_field_default(&float_range(0.0, 2.0), "mob_factor", 1.0),
        })
        .codec()
    }
}

// ---- BlocksAttacks --------------------------------------------------------------------------------------------------------

/// `BlocksAttacks.DamageReduction`.
#[derive(Clone, Debug)]
pub struct BlocksAttacksDamageReduction {
    pub horizontal_blocking_angle: f32,
    pub r#type: Option<HolderSet<NoDirect>>,
    pub base: f32,
    pub factor: f32,
}
impl PartialEq for BlocksAttacksDamageReduction {
    fn eq(&self, o: &Self) -> bool {
        feq(self.horizontal_blocking_angle, o.horizontal_blocking_angle)
            && self.r#type == o.r#type
            && feq(self.base, o.base)
            && feq(self.factor, o.factor)
    }
}
impl BlocksAttacksDamageReduction {
    pub fn codec() -> Codec<BlocksAttacksDamageReduction> {
        mc_dfu::record!(BlocksAttacksDamageReduction {
            horizontal_blocking_angle: float_field_default(
                &POSITIVE_FLOAT,
                "horizontal_blocking_angle",
                90.0
            ),
            r#type: damage_type_set_codec().optional_field_of("type"),
            base: FLOAT.field_of("base"),
            factor: FLOAT.field_of("factor"),
        })
        .codec()
    }
}
impl Encode for BlocksAttacksDamageReduction {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.horizontal_blocking_angle.encode(out)?;
        enc_opt(out, &self.r#type, |s, o| enc_damage_types(s, o))?;
        self.base.encode(out)?;
        self.factor.encode(out)?;
        Ok(())
    }
}
impl Decode for BlocksAttacksDamageReduction {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(BlocksAttacksDamageReduction {
            horizontal_blocking_angle: f32::decode(input)?,
            r#type: dec_opt(input, dec_damage_types)?,
            base: f32::decode(input)?,
            factor: f32::decode(input)?,
        })
    }
}

/// `BlocksAttacks.ItemDamageFunction`.
#[derive(Clone, Debug, Encode, Decode)]
pub struct BlocksAttacksItemDamageFunction {
    pub threshold: f32,
    pub base: f32,
    pub factor: f32,
}
impl PartialEq for BlocksAttacksItemDamageFunction {
    fn eq(&self, o: &Self) -> bool {
        feq(self.threshold, o.threshold) && feq(self.base, o.base) && feq(self.factor, o.factor)
    }
}
impl BlocksAttacksItemDamageFunction {
    pub const DEFAULT: BlocksAttacksItemDamageFunction = BlocksAttacksItemDamageFunction {
        threshold: 1.0,
        base: 0.0,
        factor: 1.0,
    };
    pub fn codec() -> Codec<BlocksAttacksItemDamageFunction> {
        mc_dfu::record!(BlocksAttacksItemDamageFunction {
            threshold: NON_NEGATIVE_FLOAT.field_of("threshold"),
            base: FLOAT.field_of("base"),
            factor: FLOAT.field_of("factor"),
        })
        .codec()
    }
}

/// `BlocksAttacks`.
#[derive(Clone, Debug)]
pub struct BlocksAttacks {
    pub block_delay_seconds: f32,
    pub disable_cooldown_scale: f32,
    pub damage_reductions: Vec<BlocksAttacksDamageReduction>,
    pub item_damage: BlocksAttacksItemDamageFunction,
    pub bypassed_by: Option<HolderSet<NoDirect>>,
    pub block_sound: Option<Holder<SoundEvent>>,
    pub disable_sound: Option<Holder<SoundEvent>>,
}
impl PartialEq for BlocksAttacks {
    fn eq(&self, o: &Self) -> bool {
        feq(self.block_delay_seconds, o.block_delay_seconds)
            && feq(self.disable_cooldown_scale, o.disable_cooldown_scale)
            && self.damage_reductions == o.damage_reductions
            && self.item_damage == o.item_damage
            && self.bypassed_by == o.bypassed_by
            && self.block_sound == o.block_sound
            && self.disable_sound == o.disable_sound
    }
}
impl BlocksAttacks {
    pub fn codec() -> Codec<BlocksAttacks> {
        let default_reductions = vec![BlocksAttacksDamageReduction {
            horizontal_blocking_angle: 90.0,
            r#type: None,
            base: 0.0,
            factor: 1.0,
        }];
        mc_dfu::record!(BlocksAttacks {
            block_delay_seconds: float_field_default(
                &NON_NEGATIVE_FLOAT,
                "block_delay_seconds",
                0.0
            ),
            disable_cooldown_scale: float_field_default(
                &NON_NEGATIVE_FLOAT,
                "disable_cooldown_scale",
                1.0
            ),
            damage_reductions: BlocksAttacksDamageReduction::codec()
                .list_of()
                .optional_field_of_default("damage_reductions", default_reductions),
            item_damage: BlocksAttacksItemDamageFunction::codec()
                .optional_field_of_default("item_damage", BlocksAttacksItemDamageFunction::DEFAULT),
            bypassed_by: damage_type_set_codec().optional_field_of("bypassed_by"),
            block_sound: SoundEvent::holder_codec().optional_field_of("block_sound"),
            disable_sound: SoundEvent::holder_codec().optional_field_of("disabled_sound"),
        })
        .codec()
    }
}
impl Encode for BlocksAttacks {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.block_delay_seconds.encode(out)?;
        self.disable_cooldown_scale.encode(out)?;
        self.damage_reductions.encode(out)?;
        self.item_damage.encode(out)?;
        enc_opt(out, &self.bypassed_by, |s, o| enc_damage_types(s, o))?;
        enc_sound_opt(&self.block_sound, out)?;
        enc_sound_opt(&self.disable_sound, out)?;
        Ok(())
    }
}
impl Decode for BlocksAttacks {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(BlocksAttacks {
            block_delay_seconds: f32::decode(input)?,
            disable_cooldown_scale: f32::decode(input)?,
            damage_reductions: Vec::decode(input)?,
            item_damage: BlocksAttacksItemDamageFunction::decode(input)?,
            bypassed_by: dec_opt(input, dec_damage_types)?,
            block_sound: dec_sound_opt(input)?,
            disable_sound: dec_sound_opt(input)?,
        })
    }
}

// ---- KineticWeapon / PiercingWeapon -----------------------------------------------------------------------------------------

/// `KineticWeapon.Condition`.
#[derive(Clone, Debug)]
pub struct KineticWeaponCondition {
    pub max_duration_ticks: i32,
    pub min_speed: f32,
    pub min_relative_speed: f32,
}
impl PartialEq for KineticWeaponCondition {
    fn eq(&self, o: &Self) -> bool {
        self.max_duration_ticks == o.max_duration_ticks
            && feq(self.min_speed, o.min_speed)
            && feq(self.min_relative_speed, o.min_relative_speed)
    }
}
impl KineticWeaponCondition {
    pub fn codec() -> Codec<KineticWeaponCondition> {
        mc_dfu::record!(KineticWeaponCondition {
            max_duration_ticks: NON_NEGATIVE_INT.field_of("max_duration_ticks"),
            min_speed: float_field_default(&FLOAT, "min_speed", 0.0),
            min_relative_speed: float_field_default(&FLOAT, "min_relative_speed", 0.0),
        })
        .codec()
    }
}
impl Encode for KineticWeaponCondition {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.max_duration_ticks);
        self.min_speed.encode(out)?;
        self.min_relative_speed.encode(out)?;
        Ok(())
    }
}
impl Decode for KineticWeaponCondition {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(KineticWeaponCondition {
            max_duration_ticks: read_varint(input)?,
            min_speed: f32::decode(input)?,
            min_relative_speed: f32::decode(input)?,
        })
    }
}

/// `KineticWeapon`.
#[derive(Clone, Debug)]
pub struct KineticWeapon {
    pub contact_cooldown_ticks: i32,
    pub delay_ticks: i32,
    pub dismount_conditions: Option<KineticWeaponCondition>,
    pub knockback_conditions: Option<KineticWeaponCondition>,
    pub damage_conditions: Option<KineticWeaponCondition>,
    pub forward_movement: f32,
    pub damage_multiplier: f32,
    pub sound: Option<Holder<SoundEvent>>,
    pub hit_sound: Option<Holder<SoundEvent>>,
}
impl PartialEq for KineticWeapon {
    fn eq(&self, o: &Self) -> bool {
        self.contact_cooldown_ticks == o.contact_cooldown_ticks
            && self.delay_ticks == o.delay_ticks
            && self.dismount_conditions == o.dismount_conditions
            && self.knockback_conditions == o.knockback_conditions
            && self.damage_conditions == o.damage_conditions
            && feq(self.forward_movement, o.forward_movement)
            && feq(self.damage_multiplier, o.damage_multiplier)
            && self.sound == o.sound
            && self.hit_sound == o.hit_sound
    }
}
impl KineticWeapon {
    pub fn codec() -> Codec<KineticWeapon> {
        let condition = KineticWeaponCondition::codec();
        mc_dfu::record!(KineticWeapon {
            contact_cooldown_ticks: NON_NEGATIVE_INT
                .optional_field_of_default("contact_cooldown_ticks", 10),
            delay_ticks: NON_NEGATIVE_INT.optional_field_of_default("delay_ticks", 0),
            dismount_conditions: condition.optional_field_of("dismount_conditions"),
            knockback_conditions: condition.optional_field_of("knockback_conditions"),
            damage_conditions: condition.optional_field_of("damage_conditions"),
            forward_movement: float_field_default(&FLOAT, "forward_movement", 0.0),
            damage_multiplier: float_field_default(&FLOAT, "damage_multiplier", 1.0),
            sound: SoundEvent::holder_codec().optional_field_of("sound"),
            hit_sound: SoundEvent::holder_codec().optional_field_of("hit_sound"),
        })
        .codec()
    }
}
impl Encode for KineticWeapon {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.contact_cooldown_ticks);
        write_varint(out, self.delay_ticks);
        self.dismount_conditions.encode(out)?;
        self.knockback_conditions.encode(out)?;
        self.damage_conditions.encode(out)?;
        self.forward_movement.encode(out)?;
        self.damage_multiplier.encode(out)?;
        enc_sound_opt(&self.sound, out)?;
        enc_sound_opt(&self.hit_sound, out)?;
        Ok(())
    }
}
impl Decode for KineticWeapon {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(KineticWeapon {
            contact_cooldown_ticks: read_varint(input)?,
            delay_ticks: read_varint(input)?,
            dismount_conditions: Decode::decode(input)?,
            knockback_conditions: Decode::decode(input)?,
            damage_conditions: Decode::decode(input)?,
            forward_movement: f32::decode(input)?,
            damage_multiplier: f32::decode(input)?,
            sound: dec_sound_opt(input)?,
            hit_sound: dec_sound_opt(input)?,
        })
    }
}

/// `PiercingWeapon`.
#[derive(Clone, Debug, PartialEq)]
pub struct PiercingWeapon {
    pub deals_knockback: bool,
    pub dismounts: bool,
    pub sound: Option<Holder<SoundEvent>>,
    pub hit_sound: Option<Holder<SoundEvent>>,
}
impl PiercingWeapon {
    pub fn codec() -> Codec<PiercingWeapon> {
        mc_dfu::record!(PiercingWeapon {
            deals_knockback: BOOL.optional_field_of_default("deals_knockback", true),
            dismounts: BOOL.optional_field_of_default("dismounts", false),
            sound: SoundEvent::holder_codec().optional_field_of("sound"),
            hit_sound: SoundEvent::holder_codec().optional_field_of("hit_sound"),
        })
        .codec()
    }
}
impl Encode for PiercingWeapon {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.deals_knockback.encode(out)?;
        self.dismounts.encode(out)?;
        enc_sound_opt(&self.sound, out)?;
        enc_sound_opt(&self.hit_sound, out)?;
        Ok(())
    }
}
impl Decode for PiercingWeapon {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(PiercingWeapon {
            deals_knockback: bool::decode(input)?,
            dismounts: bool::decode(input)?,
            sound: dec_sound_opt(input)?,
            hit_sound: dec_sound_opt(input)?,
        })
    }
}

// ---- SwingAnimation ------------------------------------------------------------------------------------------------------------

static SWING_TYPES: [(SwingAnimationType, &str); 3] = [
    (SwingAnimationType::None, "none"),
    (SwingAnimationType::Whack, "whack"),
    (SwingAnimationType::Stab, "stab"),
];

impl SwingAnimation {
    /// `SwingAnimation.CODEC`.
    pub fn codec() -> Codec<SwingAnimation> {
        mc_dfu::record!(SwingAnimation {
            kind: string_enum_codec(&SWING_TYPES)
                .optional_field_of_default("type", SwingAnimationType::Whack),
            duration: NON_NEGATIVE_INT.optional_field_of_default("duration", 6),
        })
        .codec()
    }
}

// ---- DamageResistant ---------------------------------------------------------------------------------------------------------

/// `DamageResistant`.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageResistant {
    pub types: HolderSet<NoDirect>,
}
impl DamageResistant {
    pub fn codec() -> Codec<DamageResistant> {
        mc_dfu::record!(DamageResistant {
            types: damage_type_set_codec().field_of("types")
        })
        .codec()
    }
}
impl Encode for DamageResistant {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        enc_damage_types(&self.types, out)?;
        Ok(())
    }
}
impl Decode for DamageResistant {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(DamageResistant {
            types: dec_damage_types(input)?,
        })
    }
}

// ---- Equippable ------------------------------------------------------------------------------------------------------------------

static EQUIPMENT_SLOTS: [(EquipmentSlot, &str); 8] = [
    (EquipmentSlot::MainHand, "mainhand"),
    (EquipmentSlot::OffHand, "offhand"),
    (EquipmentSlot::Feet, "feet"),
    (EquipmentSlot::Legs, "legs"),
    (EquipmentSlot::Chest, "chest"),
    (EquipmentSlot::Head, "head"),
    (EquipmentSlot::Body, "body"),
    (EquipmentSlot::Saddle, "saddle"),
];

/// `EquipmentSlot.CODEC`.
pub fn equipment_slot_codec() -> Codec<EquipmentSlot> {
    string_enum_codec(&EQUIPMENT_SLOTS)
}

/// `Equippable`.
#[derive(Clone, Debug, PartialEq)]
pub struct Equippable {
    pub slot: EquipmentSlot,
    pub equip_sound: Holder<SoundEvent>,
    pub asset_id: Option<Identifier>,
    pub camera_overlay: Option<Identifier>,
    pub allowed_entities: Option<RegistrySet<EntityType>>,
    pub dispensable: bool,
    pub swappable: bool,
    pub damage_on_hurt: bool,
    pub equip_on_interact: bool,
    pub can_be_sheared: bool,
    pub shearing_sound: Holder<SoundEvent>,
}
impl Equippable {
    pub fn codec() -> Codec<Equippable> {
        mc_dfu::record!(Equippable {
            slot: equipment_slot_codec().field_of("slot"),
            equip_sound: SoundEvent::holder_codec().optional_field_of_default(
                "equip_sound",
                Holder::Reference(ident("minecraft:item.armor.equip_generic"))
            ),
            asset_id: IDENTIFIER.optional_field_of("asset_id"),
            camera_overlay: IDENTIFIER.optional_field_of("camera_overlay"),
            allowed_entities: RegistrySet::<EntityType>::codec()
                .optional_field_of("allowed_entities"),
            dispensable: BOOL.optional_field_of_default("dispensable", true),
            swappable: BOOL.optional_field_of_default("swappable", true),
            damage_on_hurt: BOOL.optional_field_of_default("damage_on_hurt", true),
            equip_on_interact: BOOL.optional_field_of_default("equip_on_interact", false),
            can_be_sheared: BOOL.optional_field_of_default("can_be_sheared", false),
            shearing_sound: SoundEvent::holder_codec().optional_field_of_default(
                "shearing_sound",
                Holder::Reference(ident("minecraft:item.shears.snip"))
            ),
        })
        .codec()
    }
}
impl Encode for Equippable {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.slot.encode(out)?;
        SoundEvent::encode_holder(&self.equip_sound, out)?;
        self.asset_id.encode(out)?;
        self.camera_overlay.encode(out)?;
        enc_opt(out, &self.allowed_entities, |s, o| s.encode_stream(o))?;
        self.dispensable.encode(out)?;
        self.swappable.encode(out)?;
        self.damage_on_hurt.encode(out)?;
        self.equip_on_interact.encode(out)?;
        self.can_be_sheared.encode(out)?;
        SoundEvent::encode_holder(&self.shearing_sound, out)?;
        Ok(())
    }
}
impl Decode for Equippable {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Equippable {
            slot: EquipmentSlot::decode(input)?,
            equip_sound: SoundEvent::decode_holder(input)?,
            asset_id: Decode::decode(input)?,
            camera_overlay: Decode::decode(input)?,
            allowed_entities: dec_opt(input, RegistrySet::decode_stream)?,
            dispensable: bool::decode(input)?,
            swappable: bool::decode(input)?,
            damage_on_hurt: bool::decode(input)?,
            equip_on_interact: bool::decode(input)?,
            can_be_sheared: bool::decode(input)?,
            shearing_sound: SoundEvent::decode_holder(input)?,
        })
    }
}

pub(crate) fn defs(out: &mut Vec<ComponentDef>) {
    out.push(
        ComponentDef::builder(TOOL)
            .persistent(Tool::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(WEAPON)
            .persistent(Weapon::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(ATTACK_RANGE)
            .persistent(AttackRange::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(BLOCKS_ATTACKS)
            .persistent(BlocksAttacks::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(KINETIC_WEAPON)
            .persistent(KineticWeapon::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(PIERCING_WEAPON)
            .persistent(PiercingWeapon::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(ATTACK_ANIMATION)
            .persistent(SwingAnimation::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(INTERACT_ANIMATION)
            .persistent(SwingAnimation::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(DAMAGE_RESISTANT)
            .persistent(DamageResistant::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(EQUIPPABLE)
            .persistent(Equippable::codec())
            .network_synchronized()
            .build(),
    );
}
