//! The player part of the play-packet dispatcher's `PlayerSystems` (vanilla's `this.player.xxx(...)` calls of
//! `ServerGamePacketListenerImpl`). Exactly one implementor is used (`PLAYER_SYSTEMS.first()`), so every module that owns
//! one of these methods adds its delegation here.

use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAYER_SYSTEMS, PlayerSystems};
use linkme::distributed_slice;

struct ServerPlayerSystems;

impl PlayerSystems for ServerPlayerSystems {
    /// `ServerPlayer.jumpFromGround()` (jump power, sprint boost, `Stats.JUMP`, food exhaustion).
    fn jump_from_ground(&self, ctx: &mut PlayCtx) {
        crate::world::entity::Entity::jump_from_ground(&mut *ctx.player, &*ctx.level);
    }

    /// `LivingEntity.onClimbable()` of the player (records `lastClimbablePos`).
    fn on_climbable(&self, ctx: &mut PlayCtx) -> bool {
        crate::world::entity::living_entity::travel::climbable(&mut *ctx.player, &*ctx.level)
    }

    /// `handlePaddleBoat`: `player.getControlledVehicle() instanceof AbstractBoat boat` -> `boat.setPaddleState(left, right)`.
    fn set_controlled_boat_paddle_state(&self, ctx: &mut PlayCtx, left: bool, right: bool) {
        use crate::world::entity::Entity;
        use crate::world::entity::vehicle::boat::abstract_boat::AbstractBoat;
        let player_id = ctx.player.base_id();
        let Some(vehicle) = ctx.player.base().vehicle else {
            return;
        };
        ctx.level.with_entity(vehicle, |_, e| {
            if e.get_controlling_passenger() == Some(player_id)
                && let Some(boat) = e.as_any_mut().downcast_mut::<AbstractBoat>()
            {
                boat.set_paddle_state(left, right);
            }
        });
    }
    /// `CriteriaTriggers.ANY_BLOCK_USE.trigger(player, pos, itemStack)`: the stack is read after the use (vanilla holds the
    /// in-hand object, which the use mutates in place).
    fn trigger_any_block_use(
        &self,
        ctx: &mut PlayCtx,
        pos: mc_core::pos::BlockPos,
        hand: crate::world::item::InteractionHand,
    ) {
        let stack = ctx.player.get_item_in_hand(hand);
        crate::advancements::triggers::CriteriaTriggers::ANY_BLOCK_USE
            .trigger(ctx.level, ctx.player, pos, &stack);
    }

    /// `CriteriaTriggers.PLAYER_INTERACTED_WITH_ENTITY.trigger(player, awardedForStack, target)`.
    fn trigger_player_interacted_with_entity(
        &self,
        ctx: &mut PlayCtx,
        awarded_for_stack: crate::world::item::ItemStack,
        target: crate::world::entity::EntityId,
    ) {
        let Some(entity) = ctx.level.get_entity(target) else {
            return;
        };
        let matches =
            crate::advancements::triggers::CriteriaTriggers::PLAYER_INTERACTED_WITH_ENTITY.matches(
                &*ctx.level,
                &*ctx.player,
                &awarded_for_stack,
                entity,
            );
        matches.award(ctx.level, ctx.player);
    }

    /// `OPEN_INVENTORY`: `player.getVehicle() instanceof HasCustomInventoryScreen` (chest boats) -> `openCustomInventoryScreen(player)`.
    fn open_vehicle_inventory(&self, ctx: &mut PlayCtx) {
        use crate::world::entity::Entity;
        use crate::world::entity::vehicle::boat::abstract_boat::AbstractBoat;
        use crate::world::entity::vehicle::container_entity::ChestVehicleMenuProvider;
        let Some(vehicle) = ctx.player.base().vehicle else {
            return;
        };
        if ctx
            .level
            .get_entity(vehicle)
            .is_some_and(|e| e.has_custom_inventory_screen())
        {
            ctx.level.with_entity(vehicle, |level, e| {
                e.open_custom_inventory_screen(level, ctx.player)
            });
            return;
        }
        let Some((title, position)) = ctx
            .level
            .get_entity(vehicle)
            .filter(|e| {
                e.as_any()
                    .downcast_ref::<AbstractBoat>()
                    .is_some_and(AbstractBoat::is_chest_boat)
            })
            .map(|e| {
                (
                    crate::world::damagesource::display_name::entity_display_name(&*ctx.level, e),
                    e.position(),
                )
            })
        else {
            return;
        };
        ctx.player
            .open_menu(ctx.level, &ChestVehicleMenuProvider::new(vehicle, title));
        ctx.level.game_event_from_entity_ref(
            &*ctx.player,
            mc_data::GameEvent::CONTAINER_OPEN,
            position,
        );
        crate::server::level::block_break::anger_nearby_piglins(ctx.level, &*ctx.player, true);
    }

    /// `Player.tryToStartFallFlying()`.
    fn try_to_start_fall_flying(&self, ctx: &mut PlayCtx) -> bool {
        ctx.player.try_to_start_fall_flying()
    }

    /// `LivingEntity.stopFallFlying()`.
    fn stop_fall_flying(&self, ctx: &mut PlayCtx) {
        crate::world::entity::living_entity::fall_flying::stop_fall_flying(&mut *ctx.player);
    }

    /// `ServerPlayer.sendBuildLimitMessage(isTooHigh, limit)`.
    fn send_build_limit_message(&self, ctx: &mut PlayCtx, upper: bool, limit: i32) {
        ctx.player.send_build_limit_message(upper, limit);
    }

    /// `ServerPlayer.sendSpawnProtectionMessage(pos)`.
    fn send_spawn_protection_message(&self, ctx: &mut PlayCtx, pos: mc_core::pos::BlockPos) {
        ctx.player.send_spawn_protection_message(pos);
    }

    /// `ServerPlayer.setCamera(target)`.
    fn set_camera(&self, ctx: &mut PlayCtx, target: crate::world::entity::EntityId) {
        ctx.player.set_camera(ctx.level, Some(target));
    }

    /// `ServerPlayer.teleportTo(level, x, y, z, Set.of(), yRot, xRot, resetCamera)` (spectator
    /// `ServerboundTeleportToEntityPacket`). Another dimension is a `teleport(TeleportTransition)`, which needs the
    /// player in its level: it runs as a deferred server task once the player is put back.
    fn teleport_to(
        &self,
        ctx: &mut PlayCtx,
        dimension: &crate::world::level::DimensionKey,
        x: f64,
        y: f64,
        z: f64,
        y_rot: f32,
        x_rot: f32,
        set_camera: bool,
    ) {
        if set_camera {
            ctx.player.set_camera(ctx.level, None);
        }
        if *dimension != ctx.level.dimension {
            let (old, id, target) = (
                ctx.level.dimension.clone(),
                ctx.player.base_id(),
                dimension.clone(),
            );
            ctx.lis().deferred_tasks.push(Box::new(move |server| {
                let transition = crate::server::players::dimension_change::TeleportTransition::new(
                    target,
                    mc_core::phys::Vec3::new(x, y, z),
                    mc_core::phys::Vec3::ZERO,
                    y_rot,
                    x_rot,
                    crate::server::players::dimension_change::PostTeleportTransition::DO_NOTHING,
                );
                crate::server::players::dimension_change::teleport_player(
                    server,
                    &old,
                    id,
                    &transition,
                );
            }));
            return;
        }
        ctx.player.connection_teleport(x, y, z, y_rot, x_rot);
        crate::world::entity::Entity::set_y_head_rot(&mut *ctx.player, y_rot);
        if let Some(l) = ctx.player.listener.as_mut() {
            l.reset_flying_ticks();
        }
    }

    /// `ServerPlayer.interactOn(target, hand, location)`.
    fn interact_on(
        &self,
        ctx: &mut PlayCtx,
        target: crate::world::entity::EntityId,
        hand: crate::world::item::InteractionHand,
        location: mc_core::phys::Vec3,
    ) -> crate::world::item::InteractionResult {
        ctx.player.interact_on(ctx.level, target, hand, location)
    }

    /// `ServerPlayer.stopSleepInBed(forcefulWakeUp, updateLevelList)`.
    fn stop_sleep_in_bed(
        &self,
        ctx: &mut PlayCtx,
        forceful_wake_up: bool,
        update_level_list: bool,
    ) {
        ctx.player
            .stop_sleep_in_bed_impl(ctx.level, forceful_wake_up, update_level_list);
    }

    /// `getStats().sendStats(player)`.
    fn send_stats(&self, ctx: &mut PlayCtx) {
        let packet = ctx.player.stats.lock().stats_packet();
        ctx.send(&packet);
    }

    /// `ServerPlayer.checkMovementStatistics(dx, dy, dz)`.
    fn check_movement_statistics(&self, ctx: &mut PlayCtx, dx: f64, dy: f64, dz: f64) {
        ctx.player.check_movement_statistics(ctx.level, dx, dy, dz);
    }

    /// `PiercingWeapon.attack(player, MAINHAND)` of the main hand item (`STAB` action).
    fn stab_with_piercing_weapon(&self, ctx: &mut PlayCtx) {
        let stack = ctx
            .player
            .get_item_in_hand(crate::world::item::InteractionHand::MainHand);
        if let Some(piercing) = stack.get(&mc_protocol::components::PIERCING_WEAPON) {
            crate::world::item::piercing_weapon::attack(
                piercing,
                ctx.level,
                ctx.player,
                crate::world::entity::equipment::EquipmentSlot::MainHand,
            );
        }
    }

    /// `Player.cannotAttackWithItem(mainHandItem, tolerance)`.
    fn cannot_attack_with_main_hand(&self, ctx: &mut PlayCtx, tolerance_ticks: i32) -> bool {
        let main_hand = ctx
            .player
            .get_item_in_hand(crate::world::item::InteractionHand::MainHand);
        ctx.player
            .cannot_attack_with_item(&main_hand, tolerance_ticks)
    }

    /// `Player.resetAttackStrengthTicker()`.
    fn reset_attack_strength_ticker(&self, ctx: &mut PlayCtx) {
        ctx.player.reset_attack_strength_ticker();
    }

    /// `Player.isWithinAttackRange(mainHandItem, bounds, buffer)`.
    fn is_within_attack_range(
        &self,
        ctx: &mut PlayCtx,
        target: &mc_core::phys::AABB,
        buffer: f64,
    ) -> bool {
        let main_hand = ctx
            .player
            .get_item_in_hand(crate::world::item::InteractionHand::MainHand);
        ctx.player
            .is_within_attack_range(&main_hand, target, buffer)
    }

    /// `mainHandItem.has(DataComponents.PIERCING_WEAPON)`.
    fn main_hand_has_piercing_weapon(&self, ctx: &mut PlayCtx) -> bool {
        ctx.player.main_hand_has_piercing_weapon()
    }

    /// `Player.attack(target)`.
    fn attack(&self, ctx: &mut PlayCtx, target: crate::world::entity::EntityId) {
        ctx.player.attack(ctx.level, target);
    }
}

#[distributed_slice(PLAYER_SYSTEMS)]
static SERVER_PLAYER_SYSTEMS: &dyn PlayerSystems = &ServerPlayerSystems;

impl crate::world::entity::ServerPlayer {
    /// `Player.tryToStartFallFlying()`: start gliding unless already gliding, unable to glide or in a liquid.
    pub fn try_to_start_fall_flying(&mut self) -> bool {
        use crate::world::entity::Entity;
        if !self.player.living.is_fall_flying() && self.can_glide() && !self.is_in_liquid() {
            // `startFallFlying()`
            self.base_mut()
                .set_shared_flag(crate::world::entity::entity::FLAG_FALL_FLYING, true);
            true
        } else {
            false
        }
    }
}
