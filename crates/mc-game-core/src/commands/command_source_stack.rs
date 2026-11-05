//! `net.minecraft.commands.CommandSourceStack`: the source every command runs with (who, where, with which
//! permissions, and where feedback goes). Immutable value with `with_*` modifiers.
//!
//! Differences from vanilla (see `server_access`): the stack does not hold `MinecraftServer`/`ServerLevel`/`Entity`
//! references. The level is a dimension key, the entity a [`CommandEntity`] handle (id + the facts the stack itself
//! needs); use [`CommandSourceStack::with_server`] to reach the live objects.

use super::arguments::entity_anchor_argument::Anchor;
use super::command_signing_context::CommandSigningContext;
use super::server_access;
use super::{CommandResultCallback, CommandSource, CommandSourceKind};
use crate::server::MinecraftServer;
use crate::server::permissions::{Permission, PermissionSet};
use crate::server::players::player_handle::PlayerHandle;
use crate::world::entity::EntityId;
use crate::world::level::DimensionKey;
use mc_brigadier::{CommandExceptionType, CommandSyntaxException, MessageRef, SimpleCommandExceptionType};
use mc_core::math::mth;
use mc_core::phys::{Vec2, Vec3};
use mc_text::{ChatFormatting, Component};
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

use super::execution::TraceCallbacks;
use super::shared_suggestion_provider::TextCoordinates;
use super::message::{from_message, message};

pub static ERROR_NOT_PLAYER: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| SimpleCommandExceptionType::new(message(crate::translatable!("permissions.requires.player"))));
pub static ERROR_NOT_ENTITY: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| SimpleCommandExceptionType::new(message(crate::translatable!("permissions.requires.entity"))));

/// The entity a stack runs as: its id plus the facts the stack itself consults (vanilla reads them from the live
/// `Entity`; entities are not reachable without the server here).
#[derive(Clone)]
pub struct CommandEntity {
    pub id: EntityId,
    pub uuid: Uuid,
    /// `entity instanceof ServerPlayer`.
    pub is_player: bool,
    /// `Entity.getDisplayName()`.
    pub display_name: Component,
    /// `Entity.getPlainTextName()`.
    pub plain_text_name: String,
    /// `Entity.getEyeHeight()` (for `anchored eyes`).
    pub eye_height: f32,
    /// The connection handle when the entity is a player (`sendSystemMessage` goes to it).
    pub player: Option<PlayerHandle>,
}

impl std::fmt::Debug for CommandEntity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandEntity").field("id", &self.id).field("uuid", &self.uuid).field("is_player", &self.is_player).finish()
    }
}

impl PartialEq for CommandEntity {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.uuid == other.uuid
    }
}

/// `CommandSourceStack.NamesProvider`.
#[derive(Clone, Debug)]
enum NamesProvider {
    ForEntity,
    Constant(Component),
}

#[derive(Clone)]
pub struct CommandSourceStack {
    source: Arc<dyn CommandSource>,
    world_position: Vec3,
    level: DimensionKey,
    permissions: PermissionSet,
    names_provider: NamesProvider,
    silent: bool,
    entity: Option<Arc<CommandEntity>>,
    result_callback: CommandResultCallback,
    anchor: Anchor,
    rotation: Vec2,
    signing_context: CommandSigningContext,
}

impl CommandSourceStack {
    /// `new CommandSourceStack(source, position, rotation, level, permissions, server, entity)`.
    pub fn for_entity(
        source: Arc<dyn CommandSource>,
        position: Vec3,
        rotation: Vec2,
        level: DimensionKey,
        permissions: PermissionSet,
        entity: CommandEntity,
    ) -> Self {
        Self::build(source, position, rotation, level, permissions, NamesProvider::ForEntity, Some(Arc::new(entity)))
    }

    /// `new CommandSourceStack(source, position, rotation, level, permissions, name, server)`: a named, entity-less source.
    pub fn named(
        source: Arc<dyn CommandSource>,
        position: Vec3,
        rotation: Vec2,
        level: DimensionKey,
        permissions: PermissionSet,
        name: Component,
    ) -> Self {
        Self::build(source, position, rotation, level, permissions, NamesProvider::Constant(name), None)
    }

    /// `Commands.createCompilationContext`: a null-source stack used where only the permissions matter (tree pruning,
    /// function compilation). The level is `minecraft:overworld` as a placeholder (vanilla passes `null`).
    pub fn compilation_context(permissions: PermissionSet) -> Self {
        Self::named(
            super::null_source(),
            Vec3::ZERO,
            Vec2::ZERO,
            DimensionKey::parse("minecraft:overworld").expect("valid dimension key"),
            permissions,
            Component::empty(),
        )
    }

    fn build(
        source: Arc<dyn CommandSource>,
        position: Vec3,
        rotation: Vec2,
        level: DimensionKey,
        permissions: PermissionSet,
        names_provider: NamesProvider,
        entity: Option<Arc<CommandEntity>>,
    ) -> Self {
        Self {
            source,
            world_position: position,
            level,
            permissions,
            names_provider,
            silent: false,
            entity,
            result_callback: CommandResultCallback::EMPTY,
            anchor: Anchor::Feet,
            rotation,
            signing_context: CommandSigningContext::Anonymous,
        }
    }

    pub fn with_source(&self, source: Arc<dyn CommandSource>) -> Self {
        if Arc::ptr_eq(&self.source, &source) {
            return self.clone();
        }
        Self { source, ..self.clone() }
    }

    pub fn with_entity(&self, entity: CommandEntity) -> Self {
        if self.entity.as_deref() == Some(&entity) && matches!(self.names_provider, NamesProvider::ForEntity) {
            return self.clone();
        }
        Self { entity: Some(Arc::new(entity)), names_provider: NamesProvider::ForEntity, ..self.clone() }
    }

    pub fn with_position(&self, pos: Vec3) -> Self {
        Self { world_position: pos, ..self.clone() }
    }

    pub fn with_rotation(&self, rotation: Vec2) -> Self {
        Self { rotation, ..self.clone() }
    }

    pub fn with_callback(&self, result_callback: CommandResultCallback) -> Self {
        Self { result_callback, ..self.clone() }
    }

    /// `withCallback(newCallback, combiner)`.
    pub fn with_callback_combined(
        &self,
        new_callback: CommandResultCallback,
        combiner: impl FnOnce(CommandResultCallback, CommandResultCallback) -> CommandResultCallback,
    ) -> Self {
        let composite = combiner(self.result_callback.clone(), new_callback);
        self.with_callback(composite)
    }

    pub fn clear_callbacks(&self) -> Self {
        self.with_callback(CommandResultCallback::EMPTY)
    }

    pub fn with_suppressed_output(&self) -> Self {
        if !self.silent && !self.source.always_accepts() { Self { silent: true, ..self.clone() } } else { self.clone() }
    }

    pub fn with_permission(&self, permissions: PermissionSet) -> Self {
        Self { permissions, ..self.clone() }
    }

    pub fn with_maximum_permission(&self, new_permissions: &PermissionSet) -> Self {
        self.with_permission(self.permissions.union(new_permissions))
    }

    pub fn with_anchor(&self, anchor: Anchor) -> Self {
        Self { anchor, ..self.clone() }
    }

    /// `withLevel(level)` for a level whose coordinate scale relative to the current one is `scale`
    /// (`DimensionType.getTeleportationScale(current, new)`).
    pub fn with_level(&self, level: DimensionKey, scale: f64) -> Self {
        if level == self.level {
            return self.clone();
        }
        let pos = Vec3::new(self.world_position.x() * scale, self.world_position.y(), self.world_position.z() * scale);
        Self { world_position: pos, level, ..self.clone() }
    }

    /// `facing(Vec3 pos)`: rotate to look at `pos` from the anchor position.
    pub fn facing(&self, pos: Vec3) -> Self {
        let from = self.anchor.apply_source(self);
        let xd = pos.x() - from.x();
        let yd = pos.y() - from.y();
        let zd = pos.z() - from.z();
        let sd = (xd * xd + zd * zd).sqrt();
        let pi = f64::from(std::f32::consts::PI);
        let x_rot = mth::wrap_degrees_f32((-(mth::atan2(yd, sd) * 180.0 / pi)) as f32);
        let y_rot = mth::wrap_degrees_f32((mth::atan2(zd, xd) * 180.0 / pi) as f32 - 90.0);
        self.with_rotation(Vec2::new(x_rot, y_rot))
    }

    pub fn with_signing_context(&self, signing_context: CommandSigningContext) -> Self {
        Self { signing_context, ..self.clone() }
    }

    pub fn get_display_name(&self) -> Component {
        match (&self.names_provider, &self.entity) {
            (NamesProvider::Constant(name), _) => name.clone(),
            (NamesProvider::ForEntity, Some(entity)) => {
                // `Entity.getDisplayName()` is live (team prefix/color change while a command runs); the snapshot is the
                // fallback when no server is reachable.
                let live = super::server_access::has_server().then(|| {
                    self.with_server(|server| {
                        let entity_ref = super::arguments::selector::EntityRef { dimension: self.level.clone(), id: entity.id };
                        super::host::get_entity(server, &entity_ref).map(|e| super::host::entity_display_name(server, e))
                    })
                });
                live.flatten().unwrap_or_else(|| entity.display_name.clone())
            }
            (NamesProvider::ForEntity, None) => Component::empty(),
        }
    }

    pub fn get_text_name(&self) -> String {
        match (&self.names_provider, &self.entity) {
            (NamesProvider::Constant(name), _) => name.get_string(),
            (NamesProvider::ForEntity, Some(entity)) => entity.plain_text_name.clone(),
            (NamesProvider::ForEntity, None) => String::new(),
        }
    }

    pub fn permissions(&self) -> &PermissionSet {
        &self.permissions
    }

    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.has_permission(permission)
    }

    pub fn get_position(&self) -> Vec3 {
        self.world_position
    }

    pub fn get_level(&self) -> &DimensionKey {
        &self.level
    }

    pub fn get_entity(&self) -> Option<&CommandEntity> {
        self.entity.as_deref()
    }

    pub fn get_entity_or_exception(&self) -> Result<&CommandEntity, CommandSyntaxException> {
        self.entity.as_deref().ok_or_else(|| ERROR_NOT_ENTITY.create())
    }

    /// `getPlayerOrException`: the player entity this stack runs as.
    pub fn get_player_or_exception(&self) -> Result<&CommandEntity, CommandSyntaxException> {
        self.get_player().ok_or_else(|| ERROR_NOT_PLAYER.create())
    }

    pub fn get_player(&self) -> Option<&CommandEntity> {
        self.entity.as_deref().filter(|e| e.is_player)
    }

    pub fn is_player(&self) -> bool {
        self.get_player().is_some()
    }

    pub fn get_rotation(&self) -> Vec2 {
        self.rotation
    }

    pub fn get_anchor(&self) -> Anchor {
        self.anchor
    }

    pub fn get_signing_context(&self) -> &CommandSigningContext {
        &self.signing_context
    }

    /// The `CommandSource` feedback goes to.
    pub fn source(&self) -> &Arc<dyn CommandSource> {
        &self.source
    }

    pub fn source_kind(&self) -> CommandSourceKind {
        self.source.kind()
    }

    pub fn callback(&self) -> &CommandResultCallback {
        &self.result_callback
    }

    pub fn is_silent(&self) -> bool {
        self.silent
    }

    /// Borrow the server (see `server_access`); panics outside a command entry point.
    pub fn with_server<R>(&self, f: impl FnOnce(&mut MinecraftServer) -> R) -> R {
        server_access::with_server(f)
    }

    /// `getRelevantCoordinates()` (the server has no hit result, so the default `~ ~ ~`).
    pub fn get_relevant_coordinates(&self) -> Vec<TextCoordinates> {
        vec![TextCoordinates::default_global()]
    }

    /// `getAbsoluteCoordinates()`.
    pub fn get_absolute_coordinates(&self) -> Vec<TextCoordinates> {
        vec![TextCoordinates::default_global()]
    }

    /// `customSuggestion(context)`: the server has nothing to add for `ask_server` nodes.
    pub fn custom_suggestion(&self, _context: &mc_brigadier::CommandContext<CommandSourceStack>) -> mc_brigadier::Suggestions {
        mc_brigadier::Suggestions::empty()
    }

    /// `enabledFeatures()` of the source's level.
    pub fn enabled_features(&self) -> mc_world::level_storage::FeatureFlagSet {
        let level = self.level.clone();
        self.with_server(|server| super::host::level_enabled_features(server, &level))
    }

    /// `getOnlinePlayerNames()`.
    pub fn get_online_player_names(&self) -> Vec<String> {
        self.with_server(|server| super::host::online_player_names(server))
    }

    /// `suggestRegistryElements(key, elements, builder, context, filter)`.
    pub fn suggest_registry_elements(
        &self,
        registry_key: &str,
        elements: super::shared_suggestion_provider::ElementSuggestionType,
        mut builder: mc_brigadier::SuggestionsBuilder,
        filter: Option<&dyn Fn(&mc_core::ident::Identifier) -> bool>,
    ) -> mc_brigadier::Suggestions {
        use super::shared_suggestion_provider::{suggest_resource_into, suggest_resource_prefixed_into};
        let Some(view) = self.with_server(|server| super::host::registry_view(server, registry_key)) else {
            return mc_brigadier::Suggestions::empty();
        };
        if elements.should_suggest_tags() {
            suggest_resource_prefixed_into(view.tag_ids(), &mut builder, "#");
        }
        if elements.should_suggest_elements() {
            suggest_resource_into(view.element_ids().into_iter().filter(|id| filter.is_none_or(|f| f(id))), &mut builder);
        }
        builder.build()
    }

    /// `levels()`: the keys of all levels.
    pub fn levels(&self) -> Vec<DimensionKey> {
        self.with_server(|server| server.level_keys())
    }

    pub fn send_system_message(&self, message: &Component) {
        if !self.silent {
            match self.get_player().and_then(|p| p.player.as_ref()) {
                Some(player) => player.send_system_message(message, false),
                None => self.source.send_system_message(message),
            }
        }
    }

    /// `sendSuccess(messageSupplier, broadcast)`.
    pub fn send_success(&self, message_supplier: impl FnOnce() -> Component, broadcast: bool) {
        let should_send_system_message = self.source.accepts_success() && !self.silent;
        let should_broadcast = broadcast && self.source.should_inform_admins() && !self.silent;
        if should_send_system_message || should_broadcast {
            let message = message_supplier();
            if should_send_system_message {
                self.source.send_system_message(&message);
            }
            if should_broadcast {
                self.broadcast_to_admins(&message);
            }
        }
    }

    fn broadcast_to_admins(&self, message: &Component) {
        let broadcast = Component::translatable(
            "chat.type.admin",
            vec![self.get_display_name().into(), message.clone().into()],
        )
        .with_formats(&[ChatFormatting::Gray, ChatFormatting::Italic]);
        let source_kind = self.source.kind();
        let level = self.level.clone();
        self.with_server(|server| super::host::broadcast_to_admins(server, source_kind, &level, &broadcast));
    }

    pub fn send_failure(&self, message: Component) {
        if self.source.accepts_failure() && !self.silent {
            self.source.send_system_message(&Component::empty().append(message).with_format(ChatFormatting::Red));
        }
    }

    /// `ExecutionCommandSource.handleError(CommandExceptionType, Message, forked, tracer)`.
    pub fn handle_error_message(&self, _type: CommandExceptionType, message: &MessageRef, forked: bool, tracer: Option<&mut dyn TraceCallbacks>) {
        if let Some(tracer) = tracer {
            tracer.on_error(&message.get_string());
        }
        if !forked {
            self.send_failure(from_message(message.as_ref()));
        }
    }

    /// `ExecutionCommandSource.handleError(CommandSyntaxException, forked, tracer)`.
    pub fn handle_error(&self, e: &CommandSyntaxException, forked: bool, tracer: Option<&mut dyn TraceCallbacks>) {
        self.handle_error_message(e.get_type(), e.get_raw_message(), forked, tracer);
    }
}
