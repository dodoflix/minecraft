//! `SkullBlockEntity`: owner profile, note block sound, custom name and the dragon/piglin head animation counter.

use mc_game_core::block_entity_base;
use mc_game_core::world::block_entity::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec};
use mc_game_core::world::item::ItemStack;
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_data::props;
use mc_dfu::extra::IDENTIFIER;
use mc_nbt::CompoundTag;
use mc_protocol::components::map::DataComponentMapBuilder;
use mc_protocol::components::{CUSTOM_NAME, NOTE_BLOCK_SOUND, PROFILE, ResolvableProfile, component_codec};
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;
use mc_text::Component;

pub struct SkullBlockEntity {
    base: BlockEntityData,
    owner: Option<ResolvableProfile>,
    note_block_sound: Option<Identifier>,
    animation_tick_count: i32,
    is_animating: bool,
    custom_name: Option<Component>,
}

impl SkullBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::SKULL, pos, state), owner: None, note_block_sound: None, animation_tick_count: 0, is_animating: false, custom_name: None }
    }

    pub fn get_owner_profile(&self) -> Option<&ResolvableProfile> {
        self.owner.as_ref()
    }

    pub fn get_note_block_sound(&self) -> Option<&Identifier> {
        self.note_block_sound.as_ref()
    }

    /// `getAnimation(a)`.
    pub fn get_animation(&self, a: f32) -> f32 {
        if self.is_animating { self.animation_tick_count as f32 + a } else { self.animation_tick_count as f32 }
    }

    /// `SkullBlockEntity.animation(level, pos, state, entity)` (a client-side ticker in vanilla, never run by a server).
    pub fn animation(&mut self, state: BlockState) {
        if state.has_property(props::POWERED) && state.get(props::POWERED) {
            self.is_animating = true;
            self.animation_tick_count += 1;
        } else {
            self.is_animating = false;
        }
    }
}

impl BlockEntity for SkullBlockEntity {
    block_entity_base!();

    fn save_additional(&self, tag: &mut CompoundTag) {
        if let Some(owner) = &self.owner
            && let Ok(encoded) = mc_dfu::encode_nbt(&ResolvableProfile::codec(), owner).into_result()
        {
            tag.put("profile", encoded);
        }
        if let Some(sound) = &self.note_block_sound
            && let Ok(encoded) = mc_dfu::encode_nbt(&IDENTIFIER, sound).into_result()
        {
            tag.put("note_block_sound", encoded);
        }
        if let Some(name) = &self.custom_name
            && let Ok(encoded) = mc_dfu::encode_nbt(&component_codec(), name).into_result()
        {
            tag.put("custom_name", encoded);
        }
    }

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.owner = read_codec(tag, "profile", &ResolvableProfile::codec());
        self.note_block_sound = read_codec(tag, "note_block_sound", &IDENTIFIER);
        self.custom_name = read_codec(tag, "custom_name", &component_codec());
    }

    /// `saveCustomOnly`.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket { pos: self.get_block_pos(), r#type: self.get_type(), tag: self.get_update_tag() })
    }

    /// `collectImplicitComponents(components)`.
    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        let mut b = std::mem::take(builder);
        if let Some(owner) = &self.owner {
            b = b.set(PROFILE, owner.clone());
        }
        if let Some(sound) = &self.note_block_sound {
            b = b.set(NOTE_BLOCK_SOUND, sound.clone());
        }
        if let Some(name) = &self.custom_name {
            b = b.set(CUSTOM_NAME, name.clone());
        }
        *builder = b;
    }

    fn get_custom_name(&self) -> Option<Component> {
        self.custom_name.clone()
    }

    /// `applyImplicitComponents`.
    fn apply_components_from_item_stack(&mut self, stack: &ItemStack) {
        self.owner = stack.get(&PROFILE).cloned();
        self.note_block_sound = stack.get(&NOTE_BLOCK_SOUND).cloned();
        self.custom_name = stack.get(&CUSTOM_NAME).cloned();
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static SKULL: BlockEntityRegistration = BlockEntityRegistration { ty: BlockEntityType::SKULL, create: |pos, state| Box::new(SkullBlockEntity::new(pos, state)) };
