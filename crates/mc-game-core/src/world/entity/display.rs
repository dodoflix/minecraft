//! `net.minecraft.world.entity.Display` and its `BlockDisplay`, `ItemDisplay` and `TextDisplay`: invisible entities that
//! only carry synched data the client renders (a transformation, billboard mode, brightness, shadow, and a block state,
//! an item stack or a text). The server never ticks them (no `baseTick`), cannot hurt them and never lets them move or
//! press anything.
//!
//! Not implemented: the client-side render state, culling box and interpolation (`Display.RenderState`, `slerp`), which
//! only the client reads. Loads that have no server access (chunk and structure loads, spawn eggs, spawners) cannot run
//! `ComponentUtils.resolve` for a `TextDisplay` text with selector, score or NBT contents, which only `/summon`,
//! `/execute summon` and `/data merge entity` do ([`resolve_loaded_text`]): there such a text leaves the stored text as it
//! was.

use super::factory::{ENTITY_FACTORIES, EntityRegistration};
use super::{Entity, EntityData, EntityId, entity};
use crate::commands::component_utils::{self, ResolutionContext};
use crate::commands::server_access::enter_server;
use crate::define_accessor;
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::permissions::{LevelBasedPermissionSet, PermissionSet};
use crate::world::block_entity::read_codec;
use crate::world::item::ItemStack;
use crate::world::item::enchantment::value_codecs::block_state_codec;
use crate::world::level::DimensionKey;
use linkme::distributed_slice;
use mc_core::display::{BillboardConstraints, Brightness, ItemDisplayContext};
use mc_core::math::mth;
use mc_core::math::quaternionf::Quaternionf;
use mc_core::math::transformation::Transformation;
use mc_data::enums::PushReaction;
use mc_data::{Block, EntityType};
use mc_dfu::extra::{
    BILLBOARD_CONSTRAINTS, BRIGHTNESS, ITEM_DISPLAY_CONTEXT, TRANSFORMATION_EXTENDED,
};
use mc_nbt::CompoundTag;
use mc_text::Component;
use mc_text::component::{ComponentContents, TranslatableArg};
use mc_text::events::HoverEvent;
use std::any::Any;

define_accessor!(DATA_POS_ROT_INTERPOLATION_DURATION_ID: Int = Display);
define_accessor!(DATA_TRANSFORMATION_INTERPOLATION_START_DELTA_TICKS_ID: Int = Display);
define_accessor!(DATA_TRANSFORMATION_INTERPOLATION_DURATION_ID: Int = Display);
define_accessor!(DATA_TRANSLATION_ID: Vector3 = Display);
define_accessor!(DATA_SCALE_ID: Vector3 = Display);
define_accessor!(DATA_LEFT_ROTATION_ID: Quaternion = Display);
define_accessor!(DATA_RIGHT_ROTATION_ID: Quaternion = Display);
define_accessor!(DATA_BILLBOARD_RENDER_CONSTRAINTS_ID: Byte = Display);
define_accessor!(DATA_BRIGHTNESS_OVERRIDE_ID: Int = Display);
define_accessor!(DATA_VIEW_RANGE_ID: Float = Display);
define_accessor!(DATA_SHADOW_RADIUS_ID: Float = Display);
define_accessor!(DATA_SHADOW_STRENGTH_ID: Float = Display);
define_accessor!(DATA_WIDTH_ID: Float = Display);
define_accessor!(DATA_HEIGHT_ID: Float = Display);
define_accessor!(DATA_GLOW_COLOR_OVERRIDE_ID: Int = Display);

define_accessor!(DATA_BLOCK_STATE_ID: BlockState = BlockDisplay);

define_accessor!(DATA_ITEM_STACK_ID: ItemStack = ItemDisplay);
define_accessor!(DATA_ITEM_DISPLAY_ID: Byte = ItemDisplay);

define_accessor!(DATA_TEXT_ID: Component = TextDisplay);
define_accessor!(DATA_LINE_WIDTH_ID: Int = TextDisplay);
define_accessor!(DATA_BACKGROUND_COLOR_ID: Int = TextDisplay);
define_accessor!(DATA_TEXT_OPACITY_ID: Byte = TextDisplay);
define_accessor!(DATA_STYLE_FLAGS_ID: Byte = TextDisplay);

/// `NO_BRIGHTNESS_OVERRIDE` / `NO_GLOW_COLOR_OVERRIDE`.
const NONE: i32 = -1;
const FLAG_SHADOW: i8 = 1;
const FLAG_SEE_THROUGH: i8 = 2;
const FLAG_USE_DEFAULT_BACKGROUND: i8 = 4;
const FLAG_ALIGN_LEFT: i8 = 8;
const FLAG_ALIGN_RIGHT: i8 = 16;
const INITIAL_BACKGROUND: i32 = 0x4000_0000;
const INITIAL_LINE_WIDTH: i32 = 200;

/// The class of a display entity.
enum Kind {
    Block,
    Item,
    /// `unresolved` = the text of the last load that has selector, score or NBT contents, until [`resolve_loaded_text`]
    /// replaces it by its resolved form (the stored text is not touched before).
    Text {
        unresolved: Option<Box<Component>>,
    },
}

pub struct Display {
    base: EntityData,
    kind: Kind,
}

impl Display {
    pub fn new_block(ty: EntityType, id: EntityId) -> Self {
        Self::new(ty, id, Kind::Block, |b| {
            b.define(DATA_BLOCK_STATE_ID, Block::AIR.default_state());
        })
    }

    pub fn new_item(ty: EntityType, id: EntityId) -> Self {
        Self::new(ty, id, Kind::Item, |b| {
            b.define(DATA_ITEM_STACK_ID, ItemStack::empty());
            b.define(DATA_ITEM_DISPLAY_ID, ItemDisplayContext::None.id() as i8);
        })
    }

    pub fn new_text(ty: EntityType, id: EntityId) -> Self {
        Self::new(ty, id, Kind::Text { unresolved: None }, |b| {
            b.define(DATA_TEXT_ID, Component::empty());
            b.define(DATA_LINE_WIDTH_ID, INITIAL_LINE_WIDTH);
            b.define(DATA_BACKGROUND_COLOR_ID, INITIAL_BACKGROUND);
            b.define(DATA_TEXT_OPACITY_ID, -1);
            b.define(DATA_STYLE_FLAGS_ID, 0);
        })
    }

    /// `Display(type, level)` + `defineSynchedData`.
    fn new(
        ty: EntityType,
        id: EntityId,
        kind: Kind,
        define: impl FnOnce(&mut super::synched_data::SynchedEntityDataBuilder),
    ) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_POS_ROT_INTERPOLATION_DURATION_ID, 0);
            b.define(DATA_TRANSFORMATION_INTERPOLATION_START_DELTA_TICKS_ID, 0);
            b.define(DATA_TRANSFORMATION_INTERPOLATION_DURATION_ID, 0);
            b.define(DATA_TRANSLATION_ID, [0.0; 3]);
            b.define(DATA_SCALE_ID, [1.0; 3]);
            b.define(
                DATA_RIGHT_ROTATION_ID,
                quaternion_value(Quaternionf::identity()),
            );
            b.define(
                DATA_LEFT_ROTATION_ID,
                quaternion_value(Quaternionf::identity()),
            );
            b.define(
                DATA_BILLBOARD_RENDER_CONSTRAINTS_ID,
                BillboardConstraints::Fixed.id() as i8,
            );
            b.define(DATA_BRIGHTNESS_OVERRIDE_ID, NONE);
            b.define(DATA_VIEW_RANGE_ID, 1.0);
            b.define(DATA_SHADOW_RADIUS_ID, 0.0);
            b.define(DATA_SHADOW_STRENGTH_ID, 1.0);
            b.define(DATA_WIDTH_ID, 0.0);
            b.define(DATA_HEIGHT_ID, 0.0);
            b.define(DATA_GLOW_COLOR_OVERRIDE_ID, NONE);
            define(b);
        });
        base.no_physics = true;
        Self { base, kind }
    }

    /// `createTransformation(entityData)`.
    fn transformation(&self) -> Transformation {
        let data = &self.base.entity_data;
        Transformation::new(
            Some(data.get(DATA_TRANSLATION_ID)),
            Some(quaternion_of(data.get(DATA_LEFT_ROTATION_ID))),
            Some(data.get(DATA_SCALE_ID)),
            Some(quaternion_of(data.get(DATA_RIGHT_ROTATION_ID))),
        )
    }

    /// `set(accessor, value)` followed by `onSyncedDataUpdated(accessor)`.
    fn set<S: super::synched_data::Serializer>(
        &mut self,
        accessor: super::synched_data::EntityDataAccessor<S>,
        value: S::Value,
    ) {
        if self.base.entity_data.set(accessor, value) {
            self.on_synced_data_updated(accessor.id());
        }
    }

    /// `setTransformation(transformation)`.
    fn set_transformation(&mut self, transformation: &Transformation) {
        self.set(DATA_TRANSLATION_ID, transformation.translation());
        self.set(
            DATA_LEFT_ROTATION_ID,
            quaternion_value(transformation.left_rotation()),
        );
        self.set(DATA_SCALE_ID, transformation.scale());
        self.set(
            DATA_RIGHT_ROTATION_ID,
            quaternion_value(transformation.right_rotation()),
        );
    }

    fn billboard_constraints(&self) -> BillboardConstraints {
        BillboardConstraints::by_id(i32::from(
            self.base
                .entity_data
                .get(DATA_BILLBOARD_RENDER_CONSTRAINTS_ID),
        ))
    }

    fn brightness_override(&self) -> Option<Brightness> {
        let value = self.base.entity_data.get(DATA_BRIGHTNESS_OVERRIDE_ID);
        (value != NONE).then(|| Brightness::unpack(value))
    }

    fn item_display_context(&self) -> ItemDisplayContext {
        ItemDisplayContext::by_id(i32::from(self.base.entity_data.get(DATA_ITEM_DISPLAY_ID)))
    }

    /// `ItemDisplay.getSlot(0)`: the displayed stack.
    pub fn item_stack(&self) -> ItemStack {
        self.base.entity_data.get(DATA_ITEM_STACK_ID)
    }

    /// `ItemDisplay.setItemStack(item)` (what `SlotAccess` of slot 0 writes).
    pub fn set_item_stack(&mut self, stack: ItemStack) {
        if self.base.entity_data.set(DATA_ITEM_STACK_ID, stack) {
            self.on_synced_data_updated(DATA_ITEM_STACK_ID.id());
        }
    }

    fn text(&self) -> Component {
        self.base.entity_data.get(DATA_TEXT_ID)
    }

    fn add_display_save_data(&self, out: &mut CompoundTag) {
        if let Some(tag) =
            mc_dfu::encode_nbt(&TRANSFORMATION_EXTENDED, &self.transformation()).result()
        {
            out.put("transformation", tag);
        }
        if let Some(tag) =
            mc_dfu::encode_nbt(&BILLBOARD_CONSTRAINTS, &self.billboard_constraints()).result()
        {
            out.put("billboard", tag);
        }
        let data = &self.base.entity_data;
        out.put_int(
            "interpolation_duration",
            data.get(DATA_TRANSFORMATION_INTERPOLATION_DURATION_ID),
        );
        out.put_int(
            "teleport_duration",
            data.get(DATA_POS_ROT_INTERPOLATION_DURATION_ID),
        );
        out.put_float("view_range", data.get(DATA_VIEW_RANGE_ID));
        out.put_float("shadow_radius", data.get(DATA_SHADOW_RADIUS_ID));
        out.put_float("shadow_strength", data.get(DATA_SHADOW_STRENGTH_ID));
        out.put_float("width", data.get(DATA_WIDTH_ID));
        out.put_float("height", data.get(DATA_HEIGHT_ID));
        out.put_int("glow_color_override", data.get(DATA_GLOW_COLOR_OVERRIDE_ID));
        if let Some(brightness) = self.brightness_override()
            && let Some(tag) = mc_dfu::encode_nbt(&BRIGHTNESS, &brightness).result()
        {
            out.put("brightness", tag);
        }
    }

    fn read_display_save_data(&mut self, input: &CompoundTag) {
        let transformation = read_codec(input, "transformation", &TRANSFORMATION_EXTENDED)
            .unwrap_or_else(Transformation::identity);
        self.set_transformation(&transformation);
        self.set(
            DATA_TRANSFORMATION_INTERPOLATION_DURATION_ID,
            input.get_int_or("interpolation_duration", 0),
        );
        // `setTransformationInterpolationDelay`: forced dirty, so the client restarts the interpolation
        let delay = input.get_int_or("start_interpolation", 0);
        if self.base.entity_data.set_force(
            DATA_TRANSFORMATION_INTERPOLATION_START_DELTA_TICKS_ID,
            delay,
            true,
        ) {
            self.on_synced_data_updated(
                DATA_TRANSFORMATION_INTERPOLATION_START_DELTA_TICKS_ID.id(),
            );
        }
        self.set(
            DATA_POS_ROT_INTERPOLATION_DURATION_ID,
            mth::clamp_i32(input.get_int_or("teleport_duration", 0), 0, 59),
        );
        let billboard = read_codec(input, "billboard", &BILLBOARD_CONSTRAINTS)
            .unwrap_or(BillboardConstraints::Fixed);
        self.set(DATA_BILLBOARD_RENDER_CONSTRAINTS_ID, billboard.id() as i8);
        self.set(DATA_VIEW_RANGE_ID, input.get_float_or("view_range", 1.0));
        self.set(
            DATA_SHADOW_RADIUS_ID,
            input.get_float_or("shadow_radius", 0.0),
        );
        self.set(
            DATA_SHADOW_STRENGTH_ID,
            input.get_float_or("shadow_strength", 1.0),
        );
        self.set(DATA_WIDTH_ID, input.get_float_or("width", 0.0));
        self.set(DATA_HEIGHT_ID, input.get_float_or("height", 0.0));
        self.set(
            DATA_GLOW_COLOR_OVERRIDE_ID,
            input.get_int_or("glow_color_override", NONE),
        );
        let brightness = read_codec(input, "brightness", &BRIGHTNESS);
        self.set(
            DATA_BRIGHTNESS_OVERRIDE_ID,
            brightness.map_or(NONE, Brightness::pack),
        );
    }

    fn add_text_save_data(&self, out: &mut CompoundTag) {
        if let Ok(text) = self.text().to_nbt() {
            out.put("text", text);
        }
        let data = &self.base.entity_data;
        out.put_int("line_width", data.get(DATA_LINE_WIDTH_ID));
        out.put_int("background", data.get(DATA_BACKGROUND_COLOR_ID));
        out.put_byte("text_opacity", data.get(DATA_TEXT_OPACITY_ID));
        let flags = data.get(DATA_STYLE_FLAGS_ID);
        out.put_boolean("shadow", flags & FLAG_SHADOW != 0);
        out.put_boolean("see_through", flags & FLAG_SEE_THROUGH != 0);
        out.put_boolean(
            "default_background",
            flags & FLAG_USE_DEFAULT_BACKGROUND != 0,
        );
        out.put_string(
            "alignment",
            if flags & FLAG_ALIGN_LEFT != 0 {
                "left"
            } else if flags & FLAG_ALIGN_RIGHT != 0 {
                "right"
            } else {
                "center"
            },
        );
    }

    fn read_text_save_data(&mut self, input: &CompoundTag) {
        self.set(
            DATA_LINE_WIDTH_ID,
            input.get_int_or("line_width", INITIAL_LINE_WIDTH),
        );
        self.set(DATA_TEXT_OPACITY_ID, input.get_byte_or("text_opacity", -1));
        self.set(
            DATA_BACKGROUND_COLOR_ID,
            input.get_int_or("background", INITIAL_BACKGROUND),
        );
        let mut flags = 0;
        for (name, mask) in [
            ("shadow", FLAG_SHADOW),
            ("see_through", FLAG_SEE_THROUGH),
            ("default_background", FLAG_USE_DEFAULT_BACKGROUND),
        ] {
            if input.get_boolean_or(name, false) {
                flags |= mask;
            }
        }
        // `Align.CODEC`: an unknown name or another tag type reads as absent
        flags |= match input.get_string("alignment") {
            Some("left") => FLAG_ALIGN_LEFT,
            Some("right") => FLAG_ALIGN_RIGHT,
            _ => 0,
        };
        self.set(DATA_STYLE_FLAGS_ID, flags);
        let Kind::Text { unresolved } = &mut self.kind else {
            return;
        };
        *unresolved = None;
        if let Some(text) = input
            .get("text")
            .and_then(|t| match Component::from_nbt_partial(t) {
                Ok(text) => Some(text),
                Err(partial) => partial.value,
            })
        {
            // `ComponentUtils.resolve` leaves a text without selector, score and NBT contents as it is
            if needs_resolution(&text) {
                *unresolved = Some(Box::new(text));
            } else {
                self.set(DATA_TEXT_ID, text);
            }
        }
    }
}

/// Whether `ComponentUtils.resolve` changes `text`: it has selector, score or NBT contents somewhere (also in translation
/// arguments, siblings and the text of a hover event).
fn needs_resolution(text: &Component) -> bool {
    let own = match text.contents() {
        ComponentContents::Selector(_)
        | ComponentContents::Score(_)
        | ComponentContents::Nbt(_) => true,
        ComponentContents::Translatable(t) => t
            .args
            .iter()
            .any(|a| matches!(a, TranslatableArg::Component(c) if needs_resolution(c))),
        ComponentContents::Object(o) => o.fallback.as_deref().is_some_and(needs_resolution),
        _ => false,
    };
    own || text.siblings().iter().any(needs_resolution)
        || matches!(text.style().hover_event(), Some(HoverEvent::ShowText(t)) if needs_resolution(t))
}

/// `Quaternionf` of the wire value `[x, y, z, w]`.
fn quaternion_of([x, y, z, w]: [f32; 4]) -> Quaternionf {
    Quaternionf::new(x, y, z, w)
}

fn quaternion_value(q: Quaternionf) -> [f32; 4] {
    [q.x, q.y, q.z, q.w]
}

impl Entity for Display {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }

    /// `Display.tick()`: only a removed vehicle is left (no `baseTick`).
    fn tick(&mut self, level: &mut ServerLevel) {
        if let Some(vehicle) = self.base.vehicle
            && level
                .entity_manager
                .get_any(vehicle)
                .is_none_or(|v| v.is_removed())
        {
            level.stop_riding(self);
        }
    }

    fn hurt_server(
        &mut self,
        _level: &mut ServerLevel,
        _source: &crate::world::damagesource::DamageSource,
        _amount: f32,
    ) -> bool {
        false
    }

    fn get_piston_push_reaction(&self) -> PushReaction {
        PushReaction::IgnoreEntity
    }

    fn is_ignoring_block_triggers(&self) -> bool {
        true
    }

    /// `ItemDisplay.getSlot(slot)`: slot 0 is the displayed stack.
    fn get_slot(&self, slot: i32) -> Option<ItemStack> {
        (matches!(self.kind, Kind::Item) && slot == 0).then(|| self.item_stack())
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        self.add_display_save_data(out);
        match self.kind {
            Kind::Block => {
                if let Some(tag) = mc_dfu::encode_nbt(
                    &block_state_codec(),
                    &self.base.entity_data.get(DATA_BLOCK_STATE_ID),
                )
                .result()
                {
                    out.put("block_state", tag);
                }
            }
            Kind::Item => {
                let stack = self.item_stack();
                if !stack.is_empty()
                    && let Ok(tag) = stack.to_tag().into_result()
                {
                    out.put("item", tag);
                }
                if let Some(tag) =
                    mc_dfu::encode_nbt(&ITEM_DISPLAY_CONTEXT, &self.item_display_context()).result()
                {
                    out.put("item_display", tag);
                }
            }
            Kind::Text { .. } => self.add_text_save_data(out),
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.read_display_save_data(input);
        match self.kind {
            Kind::Block => {
                let state = read_codec(input, "block_state", &block_state_codec())
                    .unwrap_or_else(|| Block::AIR.default_state());
                self.set(DATA_BLOCK_STATE_ID, state);
            }
            Kind::Item => {
                let stack = input
                    .get("item")
                    .and_then(|t| ItemStack::from_tag(t).result_or_partial())
                    .unwrap_or_else(ItemStack::empty);
                self.set_item_stack(stack);
                let context = read_codec(input, "item_display", &ITEM_DISPLAY_CONTEXT)
                    .unwrap_or(ItemDisplayContext::None);
                self.set(DATA_ITEM_DISPLAY_ID, context.id() as i8);
            }
            Kind::Text { .. } => self.read_text_save_data(input),
        }
    }
}

/// The pending text of a text display that was loaded with selector, score or NBT contents.
fn take_unresolved(entity: &mut dyn Entity) -> Option<Component> {
    match entity.as_any_mut().downcast_mut::<Display>()? {
        Display {
            kind: Kind::Text { unresolved },
            ..
        } => unresolved.take().map(|text| *text),
        _ => None,
    }
}

/// Stores a resolved text; a text that failed to resolve is logged and the stored text stays.
fn store_resolved(entity: &mut dyn Entity, text: &Component, resolved: Result<Component, String>) {
    match resolved {
        Ok(resolved) => {
            if let Some(display) = entity.as_any_mut().downcast_mut::<Display>() {
                display.set(DATA_TEXT_ID, resolved);
            }
        }
        Err(e) => log::warn!("Failed to parse display entity text {text:?}: {e}"),
    }
}

/// The `ComponentUtils.resolve` step of `TextDisplay.readAdditionalSaveData` for an entity that is not in the level yet
/// (`/summon`): the text of the display's last load is resolved with the display as source and replaces the stored text.
/// `entity` is any entity: only a text display with a pending text is touched. While the text resolves the display is the
/// level's loading entity, so `@s` finds it.
pub fn resolve_loaded_text(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    mut entity: Box<dyn Entity>,
) -> Box<dyn Entity> {
    let Some(text) = take_unresolved(entity.as_mut()) else {
        return entity;
    };
    let source = entity
        .create_command_source_stack_for_name_resolution(server, dimension)
        .with_permission(PermissionSet::LevelBased(
            LevelBasedPermissionSet::GAMEMASTER,
        ));
    let Some(level) = server.level_mut(dimension) else {
        return entity;
    };
    level.loading_entity = Some(entity);
    let resolved = enter_server(server, || {
        component_utils::resolve(&ResolutionContext::create(&source), &text)
    })
    .map_err(|e| e.get_message().to_string());
    let mut entity = server
        .level_mut(dimension)
        .and_then(|level| level.loading_entity.take())
        .expect("the loading entity stays in the level while its text resolves");
    store_resolved(entity.as_mut(), &text, resolved);
    entity
}

/// [`resolve_loaded_text`] for an entity that is in the level (`/data merge entity`, which loads it where it stands): the
/// level sees the display like any other entity while its text resolves.
pub fn resolve_loaded_text_in_level(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    id: EntityId,
) {
    let Some(text) = server
        .level_mut(dimension)
        .and_then(|level| level.get_entity_mut(id))
        .and_then(take_unresolved)
    else {
        return;
    };
    let Some(entity) = server
        .level(dimension)
        .and_then(|level| level.get_entity(id))
    else {
        return;
    };
    // the source is built from the entity in the level; `resolve` needs the server for its selectors
    let source = entity
        .create_command_source_stack_for_name_resolution(server, dimension)
        .with_permission(PermissionSet::LevelBased(
            LevelBasedPermissionSet::GAMEMASTER,
        ));
    let resolved = enter_server(server, || {
        component_utils::resolve(&ResolutionContext::create(&source), &text)
    })
    .map_err(|e| e.get_message().to_string());
    if let Some(level) = server.level_mut(dimension) {
        level.with_entity(id, |_, e| store_resolved(e, &text, resolved));
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static BLOCK_DISPLAY_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::BLOCK_DISPLAY,
    create: |ty, id, _registries| Box::new(Display::new_block(ty, id)),
};

#[distributed_slice(ENTITY_FACTORIES)]
static ITEM_DISPLAY_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::ITEM_DISPLAY,
    create: |ty, id, _registries| Box::new(Display::new_item(ty, id)),
};

#[distributed_slice(ENTITY_FACTORIES)]
static TEXT_DISPLAY_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::TEXT_DISPLAY,
    create: |ty, id, _registries| Box::new(Display::new_text(ty, id)),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(display: &Display) -> CompoundTag {
        let mut tag = CompoundTag::new();
        display.add_additional_save_data(&mut tag);
        tag
    }

    #[test]
    fn text_display_codec_round_trips_and_clamps() {
        let mut display = Display::new_text(EntityType::TEXT_DISPLAY, 1);
        let mut tag = CompoundTag::new();
        tag.put_string("text", "hi");
        tag.put_string("alignment", "left");
        tag.put_boolean("shadow", true);
        tag.put_int("teleport_duration", 100);
        tag.put_int("line_width", 10);
        display.read_additional_save_data(&tag);
        let out = saved(&display);
        assert_eq!(out.get_int_or("teleport_duration", 0), 59);
        assert_eq!(out.get_string("alignment"), Some("left"));
        assert!(out.get_boolean_or("shadow", false));
        assert_eq!(out.get_int_or("line_width", 0), 10);
        assert_eq!(display.text(), Component::literal("hi"));
        // the second load of the same saved data changes nothing
        let mut again = Display::new_text(EntityType::TEXT_DISPLAY, 2);
        again.read_additional_save_data(&out);
        assert_eq!(saved(&again), out);
    }

    #[test]
    fn an_invalid_value_falls_back_to_its_default() {
        let mut display = Display::new_item(EntityType::ITEM_DISPLAY, 1);
        let mut tag = CompoundTag::new();
        tag.put_string("billboard", "bogus");
        tag.put_string("item_display", "bogus");
        display.read_additional_save_data(&tag);
        let out = saved(&display);
        assert_eq!(out.get_string("billboard"), Some("fixed"));
        assert_eq!(out.get_string("item_display"), Some("none"));
        assert!(out.get("item").is_none() && out.get("brightness").is_none());
    }

    #[test]
    fn only_an_item_display_has_slot_zero() {
        let mut item = Display::new_item(EntityType::ITEM_DISPLAY, 1);
        item.set_item_stack(ItemStack::new(mc_data::Item::STONE));
        assert!(item.get_slot(0).is_some_and(|s| !s.is_empty()));
        assert!(item.get_slot(1).is_none());
        assert!(
            Display::new_text(EntityType::TEXT_DISPLAY, 2)
                .get_slot(0)
                .is_none()
        );
    }
}
