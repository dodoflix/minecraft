//! `SignBlockEntity` + `HangingSignBlockEntity` (one struct, [`SignKind`] selects the subclass) and the `SignText`
//! helpers (`SignText` itself is the data component value from `mc_protocol`).
//!
//! Vanilla block entities keep their `Level`; here they are taken out of the chunk while they run, so every method
//! that touches the level (`markUpdated`, `setText`, ...) takes it as an argument.

use super::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec,
};
use crate::commands::command_source::null_source;
use crate::commands::command_source_stack::CommandSourceStack;
use crate::server::level::ServerLevel;
use crate::server::network::text_filter::FilteredText;
use crate::server::permissions::{LevelBasedPermissionSet, PermissionSet};
use crate::world::entity::ServerPlayer;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
pub use mc_protocol::components::SignText;
use mc_protocol::components::map::DataComponentMapBuilder;
use mc_protocol::components::{DyeColor, SIGN_TEXT_BACK, SIGN_TEXT_FRONT, WAXED};
pub use mc_protocol::packets::play::SignTextSlot;
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;
use mc_text::{ClickEvent, Component, ComponentContents, Style};
use uuid::Uuid;

/// `SignText.EMPTY`.
pub fn empty_sign_text() -> SignText {
    SignText {
        messages: vec![Component::empty(); 4],
        filtered_messages: vec![Component::empty(); 4],
        color: DyeColor::Black,
        has_glowing_text: false,
    }
}

/// Methods of vanilla's `SignText` (and `SignText.Mutable`) that are not plain field access.
pub trait SignTextExt {
    fn with_glowing_text(&self, has_glowing_text: bool) -> SignText;
    fn with_color(&self, color: DyeColor) -> SignText;
    /// `getMessages(shouldFilter)`.
    fn get_messages(&self, should_filter: bool) -> &[Component];
    /// `hasMessage(shouldFilter)`: some line has non-empty text.
    fn has_message(&self, should_filter: bool) -> bool;
    /// `hasAnyClickCommands(shouldFilter)`: some line has a `run_command` click event.
    fn has_any_click_commands(&self, should_filter: bool) -> bool;
    /// `hasEditableText(shouldFilter)`: every line is empty or plain text.
    fn has_editable_text(&self, should_filter: bool) -> bool;
}

impl SignTextExt for SignText {
    fn with_glowing_text(&self, has_glowing_text: bool) -> SignText {
        SignText {
            has_glowing_text,
            ..self.clone()
        }
    }

    fn with_color(&self, color: DyeColor) -> SignText {
        SignText {
            color,
            ..self.clone()
        }
    }

    fn get_messages(&self, should_filter: bool) -> &[Component] {
        if should_filter {
            &self.filtered_messages
        } else {
            &self.messages
        }
    }

    fn has_message(&self, should_filter: bool) -> bool {
        self.get_messages(should_filter)
            .iter()
            .any(|c| !c.get_string().is_empty())
    }

    fn has_any_click_commands(&self, should_filter: bool) -> bool {
        self.get_messages(should_filter)
            .iter()
            .any(|c| matches!(c.style().click_event(), Some(ClickEvent::RunCommand(_))))
    }

    fn has_editable_text(&self, should_filter: bool) -> bool {
        self.get_messages(should_filter).iter().all(|c| {
            *c == Component::empty() || matches!(c.contents(), ComponentContents::Literal(_))
        })
    }
}

/// Which vanilla class a [`SignBlockEntity`] is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignKind {
    Sign,
    Hanging,
}

pub struct SignBlockEntity {
    base: BlockEntityData,
    kind: SignKind,
    player_who_may_edit: Option<Uuid>,
    front_text: SignText,
    back_text: SignText,
    is_waxed: bool,
    allow_op_features: bool,
}

impl SignBlockEntity {
    /// `new SignBlockEntity(pos, state)`.
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self::with_kind(SignKind::Sign, pos, state)
    }

    /// `new HangingSignBlockEntity(pos, state)`.
    pub fn new_hanging(pos: BlockPos, state: BlockState) -> Self {
        Self::with_kind(SignKind::Hanging, pos, state)
    }

    fn with_kind(kind: SignKind, pos: BlockPos, state: BlockState) -> Self {
        let ty = match kind {
            SignKind::Sign => BlockEntityType::SIGN,
            SignKind::Hanging => BlockEntityType::HANGING_SIGN,
        };
        Self {
            base: BlockEntityData::new(ty, pos, state),
            kind,
            player_who_may_edit: None,
            front_text: empty_sign_text(),
            back_text: empty_sign_text(),
            is_waxed: false,
            allow_op_features: false,
        }
    }

    pub fn get_text(&self, slot: SignTextSlot) -> &SignText {
        match slot {
            SignTextSlot::Front => &self.front_text,
            SignTextSlot::Back => &self.back_text,
        }
    }

    /// `getTextLineHeight()`.
    pub fn get_text_line_height(&self) -> i32 {
        if self.kind == SignKind::Hanging {
            9
        } else {
            10
        }
    }

    /// `getMaxTextLineWidth()`.
    pub fn get_max_text_line_width(&self) -> i32 {
        if self.kind == SignKind::Hanging {
            60
        } else {
            90
        }
    }

    pub fn allow_op_features(&self) -> bool {
        self.allow_op_features
    }

    /// `getSignInteractionFailedSoundEvent()`.
    pub fn get_sign_interaction_failed_sound_event(&self) -> mc_data::SoundEvent {
        if self.kind == SignKind::Hanging {
            mc_data::SoundEvent::BLOCK_HANGING_SIGN_WAXED_INTERACT_FAIL
        } else {
            mc_data::SoundEvent::BLOCK_SIGN_WAXED_INTERACT_FAIL
        }
    }

    /// `getSlotPlayerIsFacing(player)` for a player at `(player_x, player_z)`.
    pub fn get_slot_player_is_facing(&self, player_x: f64, player_z: f64) -> SignTextSlot {
        use crate::world::level::block::impls::sign_block;
        let state = self.get_block_state();
        let Some(sign_y_rot) = sign_block::y_rotation_degrees(state) else {
            return SignTextSlot::Front;
        };
        let offset = sign_block::sign_hitbox_center_position(state);
        let pos = self.get_block_pos();
        let xd = player_x - (pos.x() as f64 + offset.x);
        let zd = player_z - (pos.z() as f64 + offset.z);
        let player_y_rot = (mc_core::math::mth::atan2(zd, xd) * 180.0f32 as f64
            / std::f32::consts::PI as f64) as f32
            - 90.0;
        if mc_core::math::mth::degrees_difference_abs(sign_y_rot, player_y_rot) <= 90.0 {
            SignTextSlot::Front
        } else {
            SignTextSlot::Back
        }
    }

    pub fn is_waxed(&self) -> bool {
        self.is_waxed
    }

    pub fn get_player_who_may_edit(&self) -> Option<Uuid> {
        self.player_who_may_edit
    }

    /// `setAllowedPlayerEditor(uuid)`.
    pub fn set_allowed_player_editor(&mut self, player: Option<Uuid>) {
        self.player_who_may_edit = player;
    }

    /// `markUpdated()`: `setChanged()` + `sendBlockUpdated(pos, state, state, 3)`.
    fn mark_updated(&self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        super::set_changed(level, pos, state);
        level.send_block_updated(pos, state, state, 3);
    }

    /// `setText(text, slot)`.
    pub fn set_text(&mut self, level: &mut ServerLevel, text: SignText, slot: SignTextSlot) {
        match slot {
            SignTextSlot::Front => self.front_text = text,
            SignTextSlot::Back => self.back_text = text,
        }
        self.mark_updated(level);
    }

    /// `updateText(function, slot)`. Vanilla compares object identity (`currentText != newText`); the `SignText`
    /// `with*` helpers return an equal value exactly when vanilla returns the same object, so equality is used here.
    pub fn update_text(
        &mut self,
        level: &mut ServerLevel,
        function: impl FnOnce(&SignText) -> SignText,
        slot: SignTextSlot,
    ) -> bool {
        let new_text = function(self.get_text(slot));
        if *self.get_text(slot) != new_text {
            self.set_text(level, new_text, slot);
            true
        } else {
            false
        }
    }

    /// `setWaxed(isWaxed)`.
    pub fn set_waxed(&mut self, level: &mut ServerLevel, is_waxed: bool) -> bool {
        if self.is_waxed != is_waxed {
            self.is_waxed = is_waxed;
            self.mark_updated(level);
            true
        } else {
            false
        }
    }

    /// `updateSignText(player, slot, lines)`.
    pub fn update_sign_text(
        &mut self,
        level: &mut ServerLevel,
        player: &ServerPlayer,
        slot: SignTextSlot,
        lines: &[FilteredText],
    ) {
        use crate::world::entity::Entity;
        if !self.is_waxed() && Some(player.uuid()) == self.get_player_who_may_edit() {
            let should_filter = player.text_filtering_enabled;
            // `updateMessages` always builds a new SignText, so `updateText` always reports a change
            let new_text = update_messages(lines, self.get_text(slot), should_filter);
            self.set_text(level, new_text, slot);
            self.set_allowed_player_editor(None);
            let state = self.get_block_state();
            level.send_block_updated(self.get_block_pos(), state, state, 3);
        } else {
            log::warn!(
                "Player {} just tried to change non-editable sign",
                player.get_profile().name
            );
        }
    }

    /// `playerIsTooFarAwayToEdit(uuid)`.
    pub fn player_is_too_far_away_to_edit(&self, level: &ServerLevel, player: Uuid) -> bool {
        let editing = level
            .get_player_by_uuid(player)
            .and_then(|id| level.get_entity(id))
            .and_then(|e| e.as_any().downcast_ref::<ServerPlayer>());
        editing.is_none_or(|p| !p.is_within_block_interaction_range(self.get_block_pos(), 4.0))
    }

    /// `canExecuteClickCommands(slot, player)`.
    pub fn can_execute_click_commands(&self, slot: SignTextSlot, player: &ServerPlayer) -> bool {
        self.is_waxed()
            && self
                .get_text(slot)
                .has_any_click_commands(player.text_filtering_enabled)
    }

    /// `executeClickCommandsIfPresent(level, player, pos, slot)`: runs the `run_command` / `show_dialog` / `custom`
    /// click events of the slot's lines (only for signs with `allow_op_features`); whether any line has one.
    pub fn execute_click_commands_if_present(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        pos: BlockPos,
        slot: SignTextSlot,
    ) -> bool {
        let mut has_any_click_command = false;
        for message in self
            .get_text(slot)
            .get_messages(player.text_filtering_enabled)
        {
            match message.style().click_event() {
                Some(ClickEvent::RunCommand(command)) => {
                    if self.allow_op_features {
                        run_click_command(level, player, pos, command);
                    }
                    has_any_click_command = true;
                }
                Some(ClickEvent::ShowDialog(dialog)) => {
                    if self.allow_op_features {
                        open_dialog(player, dialog);
                    }
                    has_any_click_command = true;
                }
                Some(ClickEvent::Custom { id, .. }) => {
                    if self.allow_op_features {
                        // `server.handleCustomClickAction(id, payload)`: no handler is registered in the dedicated server
                        log::debug!("Unhandled custom click action {id}");
                    }
                    has_any_click_command = true;
                }
                _ => {}
            }
        }
        if !self.allow_op_features && has_any_click_command {
            // `player.sendOverlayMessage(CLICK_ACTIONS_DISABLED)`
            if let Some(handle) = &player.handle {
                handle.send_system_message(
                    &Component::translatable("sign.click_actions_disabled", vec![])
                        .with_format(mc_text::ChatFormatting::Red),
                    true,
                );
            }
        }
        has_any_click_command
    }

    /// `applyImplicitComponents(components)`.
    fn apply_implicit_components(&mut self, stack: &crate::world::item::ItemStack) {
        self.front_text = stack
            .get(&SIGN_TEXT_FRONT)
            .cloned()
            .unwrap_or_else(empty_sign_text);
        self.back_text = stack
            .get(&SIGN_TEXT_BACK)
            .cloned()
            .unwrap_or_else(empty_sign_text);
        self.is_waxed = stack.get(&WAXED).is_some();
    }
}

/// `SignBlockEntity.updateMessages(newLines, originalText, shouldFilter)`.
fn update_messages(
    new_lines: &[FilteredText],
    original: &SignText,
    should_filter: bool,
) -> SignText {
    let mut messages: Vec<Component> = original.messages.clone();
    // `Mutable` initialises both arrays from the raw messages
    let mut filtered: Vec<Component> = original.messages.clone();
    let original_lines = original.get_messages(should_filter);
    for (i, line) in new_lines.iter().enumerate() {
        let current_style: Style = original_lines[i].style().clone();
        if should_filter {
            let text = Component::literal(line.filtered_or_empty()).set_style(current_style);
            messages[i] = text.clone();
            filtered[i] = text;
        } else {
            messages[i] = Component::literal(line.raw.clone()).set_style(current_style.clone());
            filtered[i] = Component::literal(line.filtered_or_empty()).set_style(current_style);
        }
    }
    SignText {
        messages,
        filtered_messages: filtered,
        color: original.color,
        has_glowing_text: original.has_glowing_text,
    }
}

/// `createCommandSourceStack(player, level, pos)`: gamemaster permissions, the block centre, `CommandSource.NULL`; the sign
/// itself (`"Sign"`) when no player is involved.
fn create_command_source_stack(
    player: Option<&ServerPlayer>,
    level: &ServerLevel,
    pos: BlockPos,
) -> CommandSourceStack {
    use crate::commands::command_source_stack::CommandEntity;
    use crate::world::entity::Entity;
    use mc_core::phys::{Vec2, Vec3};
    let permissions: PermissionSet = LevelBasedPermissionSet::GAMEMASTER.into();
    let position = Vec3::at_center_of(&pos);
    match player {
        Some(player) => CommandSourceStack::for_entity(
            null_source(),
            position,
            Vec2::ZERO,
            level.dimension.clone(),
            permissions,
            CommandEntity {
                id: player.id(),
                uuid: player.uuid(),
                is_player: true,
                display_name: player.get_display_name(),
                plain_text_name: player.get_plain_text_name(),
                eye_height: player.player.living.entity.eye_height,
                player: player.handle.clone(),
            },
        ),
        None => CommandSourceStack::named(
            null_source(),
            position,
            Vec2::ZERO,
            level.dimension.clone(),
            permissions,
            Component::literal("Sign"),
        ),
    }
}

/// `performPrefixedCommand(createCommandSourceStack(player, level, pos), command)`. Not implemented: the command source is
/// built but the command is not run.
fn run_click_command(level: &mut ServerLevel, player: &ServerPlayer, pos: BlockPos, command: &str) {
    let source = create_command_source_stack(Some(player), level, pos);
    let _ = (source, command);
}

/// `player.openDialog(holder)`: the click event's dialog (registry id or inline definition) as `ClientboundShowDialogPacket`.
fn open_dialog(player: &ServerPlayer, dialog: &mc_nbt::Tag) {
    use mc_protocol::dialog::Dialog;
    let Ok(holder) = mc_dfu::parse_nbt(&Dialog::codec(), dialog).into_result() else {
        return;
    };
    if let Some(wire) = Dialog::wire_holder(&holder) {
        player.send(&mc_protocol::packets::play::ClientboundShowDialogPacket { dialog: wire });
    }
}

impl BlockEntity for SignBlockEntity {
    block_entity_base!();

    fn save_additional(&self, tag: &mut CompoundTag) {
        if self.allow_op_features {
            tag.put_boolean("allow_op_features", true);
        }
        for (key, text) in [
            ("front_text", &self.front_text),
            ("back_text", &self.back_text),
        ] {
            if let Ok(encoded) = mc_dfu::encode_nbt(&SignText::codec(), text).into_result() {
                tag.put(key, encoded);
            }
        }
        tag.put_boolean("is_waxed", self.is_waxed);
    }

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.allow_op_features = tag.get_boolean_or("allow_op_features", false);
        // `resolveLines` needs the block entity's level, which does not exist yet while a chunk loads (vanilla: `this.level`
        // is still null then, so the lines stay unresolved)
        self.front_text =
            read_codec(tag, "front_text", &SignText::codec()).unwrap_or_else(empty_sign_text);
        self.back_text =
            read_codec(tag, "back_text", &SignText::codec()).unwrap_or_else(empty_sign_text);
        self.is_waxed = tag.get_boolean_or("is_waxed", false);
    }

    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket {
            pos: self.get_block_pos(),
            r#type: self.get_type(),
            tag: self.get_update_tag(),
        })
    }

    fn is_ticking(&self) -> bool {
        true
    }

    /// `SignBlockEntity.tick` (`clearInvalidPlayerWhoMayEdit`).
    fn server_tick(&mut self, level: &mut ServerLevel) {
        if let Some(editor) = self.player_who_may_edit
            && self.player_is_too_far_away_to_edit(level, editor)
        {
            self.set_allowed_player_editor(None);
        }
    }

    fn apply_components_from_item_stack(&mut self, stack: &crate::world::item::ItemStack) {
        self.apply_implicit_components(stack);
    }

    /// `collectImplicitComponents(components)`.
    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        let mut b = std::mem::take(builder)
            .set(SIGN_TEXT_FRONT, self.front_text.clone())
            .set(SIGN_TEXT_BACK, self.back_text.clone());
        if self.is_waxed {
            b = b.set(WAXED, ());
        }
        *builder = b;
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static SIGN: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::SIGN,
    create: |pos, state| Box::new(SignBlockEntity::new(pos, state)),
};

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static HANGING_SIGN: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::HANGING_SIGN,
    create: |pos, state| Box::new(SignBlockEntity::new_hanging(pos, state)),
};

/// The sign part of the play listener (`ServerGamePacketListenerImpl.handleSignUpdate` after the lines were stripped of
/// formatting): text filtering, then `SignBlockEntity.updateSignText` for a loaded chunk.
struct SignPlayHandlers;

impl crate::server::network::game_packets::hooks::PlayHandlers for SignPlayHandlers {
    fn update_sign_text(
        &self,
        ctx: &mut crate::server::network::game_packets::PlayCtx,
        packet: &mc_protocol::packets::play::ServerboundSignUpdatePacket,
        lines: &[String],
    ) {
        use crate::server::network::text_filter::{DummyTextFilter, TextFilter};
        use crate::world::level::LevelReader;
        // `filterTextPacket(lines)`: the dummy filter (no `text-filtering-config` backend is ported)
        let filtered = DummyTextFilter.process_message_bundle(lines);
        let pos = packet.pos;
        if ctx.level.has_chunk(pos.x() >> 4, pos.z() >> 4) {
            let player: &ServerPlayer = ctx.player;
            ctx.level.with_block_entity(pos, |level, be| {
                if let Some(sign) = be.as_any_mut().downcast_mut::<SignBlockEntity>() {
                    sign.update_sign_text(level, player, packet.slot, &filtered);
                }
            });
        }
    }
}

#[distributed_slice(crate::server::network::game_packets::hooks::PLAY_HANDLERS)]
static SIGN_PLAY_HANDLERS: &dyn crate::server::network::game_packets::hooks::PlayHandlers =
    &SignPlayHandlers;
