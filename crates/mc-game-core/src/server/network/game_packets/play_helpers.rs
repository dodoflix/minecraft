//! Vanilla-named helpers the play packet listener uses on `ServerPlayer`, the level and the server, expressed over the
//! data the entity, level and server modules expose.

use super::PlayCtx;
use crate::server::level::ServerLevel;
use crate::server::players::name_and_id::NameAndId;
use crate::world::entity::movement::{EntityCollisionQuery, MovementLevel, collision_context};
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::collision::{CollisionContext, CollisionGetter};
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::phys::shapes::{BooleanOp, Shapes, VoxelShape};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::common::{ChatVisibility, ClientInformation};
use mc_protocol::packets::play::{ClientboundSystemChatPacket, Input};
use mc_text::Component;

/// `PlayerModelPart.HAT`'s mask.
const MODEL_PART_HAT: u8 = 1 << 6;

impl ServerPlayer {
    /// `getPlainTextName()`.
    pub fn get_plain_text_name(&self) -> String {
        self.player.game_profile.name.clone()
    }

    /// `getGameProfile()`.
    pub fn get_profile(&self) -> &GameProfile {
        &self.player.game_profile
    }

    /// `nameAndId()`.
    pub fn name_and_id(&self) -> NameAndId {
        NameAndId {
            id: self.player.game_profile.id,
            name: self.player.game_profile.name.clone(),
        }
    }

    /// `getDisplayName()` without the team formatting (that needs the scoreboard: `host::entity_display_name`).
    pub fn get_display_name(&self) -> Component {
        crate::commands::host::decorate_display_name(
            Component::literal(self.player.game_profile.name.clone()),
            self,
        )
    }

    pub fn get_y_rot(&self) -> f32 {
        self.player.living.entity.y_rot()
    }

    pub fn get_x_rot(&self) -> f32 {
        self.player.living.entity.x_rot()
    }

    pub fn is_sleeping(&self) -> bool {
        self.player.living.is_sleeping()
    }

    pub fn is_dead_or_dying(&self) -> bool {
        self.player.living.is_dead_or_dying()
    }

    pub fn is_fall_flying(&self) -> bool {
        self.player.living.is_fall_flying()
    }

    pub fn is_passenger(&self) -> bool {
        self.player.living.entity.is_passenger()
    }

    pub fn get_health(&self) -> f32 {
        self.player.living.get_health()
    }

    /// `Entity.isChangingDimension()`.
    pub fn is_changing_dimension(&self) -> bool {
        self.is_changing_dimension
    }

    pub fn get_chat_visibility(&self) -> ChatVisibility {
        self.chat_visibility
    }

    /// `getLastActionTime()` / `resetLastActionTime()` (`Util.getMillis()`).
    pub fn get_last_action_time(&self) -> i64 {
        self.last_action_time
    }

    pub fn reset_last_action_time(&mut self) {
        self.last_action_time = crate::util::util::get_millis();
    }

    /// `setShiftKeyDown(shift)`.
    pub fn set_shift_key_down(&mut self, shift: bool) {
        self.player
            .living
            .entity
            .set_shared_flag(crate::world::entity::entity::FLAG_SHIFT_KEY_DOWN, shift);
    }

    /// `setKnownMovement(movement)`.
    pub fn set_known_movement(&mut self, movement: Vec3) {
        self.last_known_client_movement = movement;
    }

    /// `setLastClientInput(input)`.
    pub fn set_last_client_input(&mut self, input: Input) {
        self.last_client_input = input;
    }

    /// `isModelPartShown(PlayerModelPart.HAT)`.
    pub fn is_model_part_shown_hat(&self) -> bool {
        self.player.is_model_part_shown(MODEL_PART_HAT)
    }

    /// `clientInformation()` (the model customisation is read back from the synched data like vanilla).
    pub fn client_information(&self) -> ClientInformation {
        let mut info = self.client_information.clone();
        info.model_customisation = self
            .player
            .living
            .entity
            .entity_data
            .get(crate::world::entity::player::DATA_PLAYER_MODE_CUSTOMISATION)
            as u8;
        info.main_hand = self.player.get_main_arm();
        info
    }

    /// `sendSystemMessage(message)`.
    pub fn send_system_message(&self, message: Component) {
        self.send(&ClientboundSystemChatPacket {
            content: message,
            overlay: false,
        });
    }
}

/// `noCollision(entity, aabb)`: block, entity and world-border collisions.
pub(super) fn no_collision(level: &ServerLevel, entity: &dyn Entity, aabb: &AABB) -> bool {
    let context = collision_context(entity);
    level.no_block_collision(Some(&context), aabb)
        && level.entity_collisions(Some(entity), aabb).is_empty()
        && level
            .world_border_collision(entity, aabb)
            .is_none_or(|border| {
                !Shapes::join_is_not_empty(&border, &Shapes::create(*aabb), BooleanOp::AND)
            })
}

/// `getPreMoveCollisions(entity, box, oldPos)`.
pub(super) fn get_pre_move_collisions(
    level: &ServerLevel,
    entity: &dyn Entity,
    aabb: &AABB,
    old_pos: Vec3,
) -> Vec<VoxelShape> {
    let mut shapes = level.entity_collisions(Some(entity), aabb);
    let context = CollisionContext::with_position(
        entity.id(),
        entity.is_descending(),
        old_pos.y,
        entity.held_item_for_collision(),
        entity.can_stand_on_fluid_fn(),
    );
    shapes.extend(level.get_block_collisions_from_context(context, *aabb));
    shapes
}

/// `level.findBlocksIn(box).filterState(isAir).allMatched()`: positions outside the world count as air.
pub(super) fn all_blocks_in_are_air(level: &ServerLevel, aabb: &AABB) -> bool {
    let min = BlockPos::containing_position(&aabb.get_min_position());
    let max = BlockPos::containing_position(&aabb.get_max_position());
    for x in min.x()..=max.x() {
        for y in min.y()..=max.y() {
            for z in min.z()..=max.z() {
                if !level.get_block_state(BlockPos::new(x, y, z)).is_air() {
                    return false;
                }
            }
        }
    }
    true
}

/// `itemStack.isItemEnabled(level.enabledFeatures())`.
pub(super) fn item_enabled(
    level: &ServerLevel,
    stack: &mc_protocol::components::ItemStack,
) -> bool {
    crate::world::item::is_item_enabled(stack, level.enabled_features())
}

impl PlayCtx<'_> {
    /// `player.permissions().hasPermission(Permissions.COMMANDS_GAMEMASTER)`.
    pub(super) fn has_gamemaster_permission(&mut self) -> bool {
        let id = self.player.name_and_id();
        self.server
            .player_list
            .get_profile_permissions(&id)
            .has_permission(&crate::server::permissions::permissions::COMMANDS_GAMEMASTER)
    }

    /// `player.hasInfiniteMaterials()` (creative: `abilities.instabuild`).
    pub(super) fn has_infinite_materials(&self) -> bool {
        self.player.player.abilities.instabuild
    }

    /// `level.getMaxY()` / `getMinY()`.
    pub(super) fn max_y(&self) -> i32 {
        self.level.get_max_y()
    }

    pub(super) fn min_y(&self) -> i32 {
        self.level.get_min_y()
    }

    /// `level.isLoaded`-style chunk presence for `ClientboundBlockUpdatePacket(level, pos)`.
    pub(super) fn block_state_at(&self, pos: BlockPos) -> mc_data::block::BlockState {
        self.level.get_block_state(pos)
    }

    /// `level.tickRateManager().runsNormally()`.
    pub(super) fn runs_normally(&self) -> bool {
        self.server.tick_rate_manager.runs_normally()
    }

    /// `level.getChunkSource().move(player)` after the player's position changed.
    pub(super) fn chunk_source_move(&mut self) {
        self.player.chunk_source_move(self.level);
    }
}
