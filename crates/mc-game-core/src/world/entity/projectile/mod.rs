//! `net.minecraft.world.entity.projectile.Projectile`: the base class of arrows, fireballs, thrown items, ...
//!
//! Data inheritance: a concrete projectile embeds a [`ProjectileData`] next to its `EntityData` and implements
//! [`Projectile`] (the overridable hooks, defaults = vanilla `Projectile`) besides `Entity`, returning itself from
//! `Entity::as_projectile{,_mut}` (use `impl_projectile_entity!`). Its `Entity::tick` calls [`tick`] (= `Projectile.tick`,
//! which ends in `Entity.tick`), its hit logic [`hit_target_or_deflect_self`] after [`util::get_hit_result_on_move_vector`].
//! The owner is an `EntityReference` = a UUID resolved through the level.

pub mod arrow;
pub mod deflection;
pub mod evoker_fangs;
pub mod firework_rocket;
pub mod hurting;
pub mod llama_spit;
pub mod shulker_bullet;
pub mod spawn;
pub mod throwable;
pub mod throwableitemprojectile;
pub mod util;

use super::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use super::{Entity, EntityId};
use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use crate::world::level::gameevent::GameEventContext;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::GameEvent;
use mc_protocol::components::AdventureModePredicate;
use uuid::Uuid;

pub use deflection::{Deflector, ProjectileDeflection};

/// The vanilla bytecode multiplies the angle by `(double)(180.0F / (float)Math.PI)` (a float constant), not by 180 and
/// then divides by `(float)Math.PI` as the decompiled source reads (oracle: axis-aligned shots give 17.999998, not 18).
pub const RAD_TO_DEG: f64 = (180.0f32 / std::f32::consts::PI) as f64;

/// `EntityHitResult`.
#[derive(Clone, Copy, Debug)]
pub struct EntityHitResult {
    pub entity: EntityId,
    pub location: Vec3,
}

/// `HitResult` (`BlockHitResult` incl. the `MISS` type, or `EntityHitResult`).
#[derive(Clone, Copy, Debug)]
pub enum HitResult {
    Block(BlockHitResult),
    Entity(EntityHitResult),
}

/// `HitResult.Type`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitType {
    Miss,
    Block,
    Entity,
}

impl HitResult {
    pub fn get_type(&self) -> HitType {
        match self {
            HitResult::Block(b) if b.miss => HitType::Miss,
            HitResult::Block(_) => HitType::Block,
            HitResult::Entity(_) => HitType::Entity,
        }
    }

    pub fn get_location(&self) -> Vec3 {
        match self {
            HitResult::Block(b) => b.location,
            HitResult::Entity(e) => e.location,
        }
    }
}

/// The fields `Projectile` adds to `Entity`.
#[derive(Default)]
pub struct ProjectileData {
    /// `owner` (`EntityReference<Entity>`).
    pub owner: Option<Uuid>,
    /// Entity id of the owner when it was set from a live entity: what `getAddEntityPacket` writes as the packet data
    /// (that method has no level to resolve `owner` with).
    pub owner_id: Option<EntityId>,
    pub can_break: Option<AdventureModePredicate>,
    pub left_owner: bool,
    pub left_owner_checked: bool,
    pub has_been_shot: bool,
    /// `lastDeflectedBy`.
    pub last_deflected_by: Option<EntityId>,
}

/// `Projectile` and its overridable hooks.
pub trait Projectile: Entity {
    fn projectile_data(&self) -> &ProjectileData;
    fn projectile_data_mut(&mut self) -> &mut ProjectileData;

    /// `getOwner()`.
    fn get_owner<'l>(&self, level: &'l ServerLevel) -> Option<&'l dyn Entity> {
        level.get_entity_by_uuid(self.projectile_data().owner?)
    }

    /// `setOwner(entity)`.
    fn set_owner(&mut self, owner: Option<&dyn Entity>) {
        let d = self.projectile_data_mut();
        d.owner = owner.map(|o| o.uuid());
        d.owner_id = owner.map(|o| o.id());
    }

    /// `setOwner(EntityReference)`.
    fn set_owner_uuid(&mut self, owner: Option<Uuid>) {
        let d = self.projectile_data_mut();
        d.owner = owner;
        d.owner_id = None;
    }

    /// `getEffectSource()`: the owner, else this projectile.
    fn get_effect_source(&self, level: &ServerLevel) -> EntityId {
        self.get_owner(level).map_or(self.id(), |o| o.id())
    }

    /// `ownedBy(entity)`.
    fn owned_by(&self, entity: &dyn Entity) -> bool {
        self.projectile_data().owner == Some(entity.uuid())
    }

    /// `shouldBounceOnWorldBorder()`.
    fn should_bounce_on_world_border(&self) -> bool {
        false
    }

    /// `onDeflection(byAttack)`.
    fn on_deflection(&mut self, _level: &mut ServerLevel, _by_attack: bool) {}

    /// `onItemBreak(item)`.
    fn on_item_break(&mut self, _item: &crate::world::item::ItemStack) {}

    /// `shoot(xd, yd, zd, pow, uncertainty)` (overridable; default [`shoot_base`]).
    fn shoot(&mut self, xd: f64, yd: f64, zd: f64, pow: f32, uncertainty: f32) {
        if let Some(this) = self.as_projectile_mut() {
            shoot_base(this, xd, yd, zd, pow, uncertainty);
        }
    }

    /// `onHit(hitResult)` (overridable; default [`on_hit_base`]).
    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        if let Some(this) = self.as_projectile_mut() {
            on_hit_base(this, level, hit);
        }
    }

    /// `deflect(deflection, deflectingEntity, newOwner, byAttack, power)` (overridable; default [`deflect_base`]).
    fn deflect(
        &mut self,
        level: &mut ServerLevel,
        deflection: ProjectileDeflection,
        deflecting_entity: Option<EntityId>,
        new_owner: Option<Uuid>,
        by_attack: bool,
        power: Vec3,
    ) -> bool {
        match self.as_projectile_mut() {
            Some(this) => deflect_base(
                this,
                level,
                deflection,
                deflecting_entity,
                new_owner,
                by_attack,
                power,
            ),
            None => false,
        }
    }

    /// `onHitEntity(hitResult)`.
    fn on_hit_entity(&mut self, _level: &mut ServerLevel, _hit: &EntityHitResult) {}

    /// `onHitBlock(hitResult)`: `state.onProjectileHit(level, state, hitResult, this)`.
    fn on_hit_block(&mut self, level: &mut ServerLevel, hit: &BlockHitResult) {
        let state = level.get_block_state(hit.block_pos);
        if let Some(this) = self.as_projectile_mut() {
            crate::world::level::block::behaviour(state.block())
                .on_projectile_hit(level, state, hit, this);
        }
    }

    /// `onRedirectProjectile(hitProjectile)`: the redirectable projectile that was hit is aimed along the owner's look.
    fn on_redirect_projectile(&mut self, level: &mut ServerLevel, hit_projectile: EntityId) {
        let owner = self.projectile_data().owner;
        let owner_id = self.get_owner(level).map(|o| o.id());
        level.with_entity(hit_projectile, |level, e| {
            if let Some(p) = e.as_projectile_mut() {
                p.deflect(
                    level,
                    ProjectileDeflection::AimDeflect,
                    owner_id,
                    owner,
                    true,
                    Vec3::new(1.0, 1.0, 1.0),
                );
            }
        });
    }

    /// `canHitEntity(entity)`.
    fn can_hit_entity(&self, level: &ServerLevel, entity: &dyn Entity) -> bool {
        if !entity.can_be_hit_by_projectile() {
            return false;
        }
        let owner = self.get_owner(level);
        owner.is_none_or(|o| {
            self.projectile_data().left_owner || !level.is_passenger_of_same_vehicle(o, entity)
        })
    }

    /// `getMovementToShoot(xd, yd, zd, pow, uncertainty)`.
    fn get_movement_to_shoot(
        &mut self,
        xd: f64,
        yd: f64,
        zd: f64,
        pow: f32,
        uncertainty: f32,
    ) -> Vec3 {
        let spread = 0.0172275 * f64::from(uncertainty);
        let normalized = Vec3::new(xd, yd, zd).normalize();
        let random = &mut self.base_mut().random;
        let dx = random.triangle(0.0, spread);
        let dy = random.triangle(0.0, spread);
        let dz = random.triangle(0.0, spread);
        normalized.add_xyz(dx, dy, dz).scale(f64::from(pow))
    }

    /// `canBreakBlockInAdventureMode(level, blockInWorld)` at `pos`.
    fn can_break_block_in_adventure_mode(&self, level: &ServerLevel, pos: BlockPos) -> bool {
        let owner = self.get_owner(level);
        let world_game_mode_is_adventure =
            level.level_data.get_game_type() == mc_world::level_storage::GameType::Adventure;
        let owner_not_adventure_player = owner.is_some_and(|o| {
            o.as_any()
                .downcast_ref::<crate::world::entity::ServerPlayer>()
                .is_some_and(|p| {
                    p.game_mode.game_mode_for_player != mc_data::enums::GameType::Adventure
                })
        });
        if (owner.is_some() || world_game_mode_is_adventure) && !owner_not_adventure_player {
            self.projectile_data()
                .can_break
                .as_ref()
                .is_some_and(|p| crate::world::item::adventure_predicate_test(p, level, pos))
        } else {
            true
        }
    }

    /// `mayInteract(level, pos)`.
    fn may_interact_projectile(&self, level: &ServerLevel, pos: BlockPos) -> bool {
        match self.get_owner(level) {
            Some(owner) if owner.is_player() => owner.may_interact(level, pos),
            owner => owner.is_none() || level.mob_griefing(),
        }
    }

    /// `mayBreak(level, pos)`.
    fn may_break(&self, level: &ServerLevel, pos: BlockPos) -> bool {
        mc_data::tags::get::<mc_data::EntityType>("impact_projectiles")
            .is_some_and(|t| t.contains(self.get_type()))
            && level.projectiles_can_break_blocks()
            && self.can_break_block_in_adventure_mode(level, pos)
    }

    /// `calculateHorizontalHurtKnockbackDirection(hurtEntity, damageSource)`: the projectile's own horizontal motion.
    fn calculate_horizontal_hurt_knockback_direction(&self) -> (f64, f64) {
        let m = self.base().delta_movement();
        (m.x, m.z)
    }
}

/// `Entity` overrides shared by every projectile (`isPickable`, `getPickRadius`, `getMoveSimulationType`,
/// `getAddEntityPacket`, `mayInteract`, `hurtServer`'s `markHurt`) plus the downcasts.
/// Put inside `impl Entity for T` of a concrete projectile type that implements [`Projectile`].
#[macro_export]
macro_rules! impl_projectile_entity {
    () => {
        $crate::impl_projectile_entity!(@base);
        $crate::impl_projectile_entity!(@hurt);
        fn is_pickable(&self) -> bool {
            $crate::world::entity::projectile::is_pickable(self.get_type())
        }
        fn on_above_bubble_column(&mut self, level: &mut $crate::server::level::ServerLevel, drag_down: bool, pos: mc_core::pos::BlockPos) {
            $crate::world::entity::projectile::on_above_bubble_column(self, level, drag_down, pos);
        }
        fn on_inside_bubble_column(&mut self, _level: &mut $crate::server::level::ServerLevel, drag_down: bool) {
            $crate::world::entity::projectile::on_inside_bubble_column(self, drag_down);
        }
    };
    // Projectiles that override `isPickable` and the bubble column behavior themselves (arrows).
    (arrow) => {
        $crate::impl_projectile_entity!(@base);
        $crate::impl_projectile_entity!(@hurt);
    };
    // Projectiles that override `hurtServer` themselves (hurting projectiles).
    (hurting) => {
        $crate::impl_projectile_entity!(@base);
        fn is_pickable(&self) -> bool {
            $crate::world::entity::projectile::is_pickable(self.get_type())
        }
        fn on_above_bubble_column(&mut self, level: &mut $crate::server::level::ServerLevel, drag_down: bool, pos: mc_core::pos::BlockPos) {
            $crate::world::entity::projectile::on_above_bubble_column(self, level, drag_down, pos);
        }
        fn on_inside_bubble_column(&mut self, _level: &mut $crate::server::level::ServerLevel, drag_down: bool) {
            $crate::world::entity::projectile::on_inside_bubble_column(self, drag_down);
        }
    };
    (@hurt) => {
        fn hurt_server(&mut self, level: &mut $crate::server::level::ServerLevel, source: &$crate::world::damagesource::DamageSource, _amount: f32) -> bool {
            if !self.is_invulnerable_to_base(level, source) {
                self.mark_hurt();
            }
            false
        }
    };
    (@base) => {
        $crate::impl_projectile_entity!(@core);
        fn hurt_knockback_direction(&self) -> Option<$crate::world::damagesource::HurtKnockbackDirection> {
            let (x, z) = $crate::world::entity::projectile::Projectile::calculate_horizontal_hurt_knockback_direction(self);
            Some($crate::world::damagesource::HurtKnockbackDirection::DeltaMovement { x, z })
        }
    };
    // Projectiles whose knockback points from the projectile to the hurt entity (`AbstractThrownPotion`,
    // `FireworkRocketEntity`).
    (away) => {
        $crate::impl_projectile_entity!(@core);
        $crate::impl_projectile_entity!(@hurt);
        fn is_pickable(&self) -> bool {
            $crate::world::entity::projectile::is_pickable(self.get_type())
        }
        fn on_above_bubble_column(&mut self, level: &mut $crate::server::level::ServerLevel, drag_down: bool, pos: mc_core::pos::BlockPos) {
            $crate::world::entity::projectile::on_above_bubble_column(self, level, drag_down, pos);
        }
        fn on_inside_bubble_column(&mut self, _level: &mut $crate::server::level::ServerLevel, drag_down: bool) {
            $crate::world::entity::projectile::on_inside_bubble_column(self, drag_down);
        }
        fn hurt_knockback_direction(&self) -> Option<$crate::world::damagesource::HurtKnockbackDirection> {
            Some($crate::world::damagesource::HurtKnockbackDirection::AwayFromProjectile)
        }
    };
    (@core) => {
        fn as_projectile(&self) -> Option<&dyn $crate::world::entity::projectile::Projectile> {
            Some(self)
        }
        fn as_projectile_mut(&mut self) -> Option<&mut dyn $crate::world::entity::projectile::Projectile> {
            Some(self)
        }
        fn get_pick_radius(&self) -> f32 {
            if self.is_pickable() { 1.0 } else { 0.0 }
        }
        fn move_simulation_type(&self) -> $crate::world::entity::MoveSimulationType {
            $crate::world::entity::MoveSimulationType::ServerAndClient
        }
        fn get_add_entity_packet(
            &self,
            server_entity: &$crate::server::level::ServerEntity,
        ) -> mc_protocol::packets::play::ClientboundAddEntityPacket {
            let owner = $crate::world::entity::projectile::Projectile::projectile_data(self).owner_id.unwrap_or(0);
            server_entity.add_entity_packet(self.id(), self.uuid(), self.get_type(), owner)
        }
        fn may_interact(&self, level: &$crate::server::level::ServerLevel, pos: mc_core::pos::BlockPos) -> bool {
            $crate::world::entity::projectile::Projectile::may_interact_projectile(self, level, pos)
        }
        fn get_dimension_changing_delay(&self, _level: &$crate::server::level::ServerLevel) -> i32 {
            2
        }
    };
}

/// `Projectile.isPickable()`: `is(EntityTypeTags.REDIRECTABLE_PROJECTILE)`.
pub fn is_pickable(ty: mc_data::EntityType) -> bool {
    mc_data::tags::get::<mc_data::EntityType>("redirectable_projectile")
        .is_some_and(|t| t.contains(ty))
}

/// `Projectile.onAboveBubbleColumn(dragDown, pos)`.
pub fn on_above_bubble_column(
    projectile: &mut dyn Entity,
    level: &mut ServerLevel,
    drag_down: bool,
    pos: BlockPos,
) {
    let yd = if drag_down { -0.03 } else { 0.1 };
    let b = projectile.base_mut();
    b.set_delta_movement(b.delta_movement().add_xyz(0.0, yd, 0.0));
    super::entity::send_bubble_column_particles(level, pos);
}

/// `Projectile.onInsideBubbleColumn(dragDown)`.
pub fn on_inside_bubble_column(projectile: &mut dyn Entity, drag_down: bool) {
    let yd = if drag_down { -0.03 } else { 0.06 };
    let b = projectile.base_mut();
    b.set_delta_movement(b.delta_movement().add_xyz(0.0, yd, 0.0));
    projectile.reset_fall_distance();
}

/// `Projectile.tick()`: the shoot game event, the owner check, then `Entity.tick()`.
pub fn tick(projectile: &mut dyn Projectile, level: &mut ServerLevel) {
    if !projectile.projectile_data().has_been_shot {
        let owner = projectile.get_owner(level).map(|o| o.id());
        let position = projectile.position();
        level.game_event_from_entity(owner, GameEvent::PROJECTILE_SHOOT, position);
        projectile.projectile_data_mut().has_been_shot = true;
    }
    check_left_owner(projectile, level);
    super::entity::base_tick(projectile, level);
    projectile.projectile_data_mut().left_owner_checked = false;
}

/// `checkLeftOwner()`.
pub fn check_left_owner(projectile: &mut dyn Projectile, level: &ServerLevel) {
    let d = projectile.projectile_data();
    if !d.left_owner && !d.left_owner_checked {
        let outside = is_outside_owner_collision_range(projectile, level);
        let d = projectile.projectile_data_mut();
        d.left_owner = outside;
        d.left_owner_checked = true;
    }
}

fn is_outside_owner_collision_range(projectile: &dyn Projectile, level: &ServerLevel) -> bool {
    let Some(owner) = projectile.get_owner(level) else {
        return true;
    };
    let aabb = projectile
        .get_bounding_box()
        .expand_towards_vec3(projectile.base().delta_movement())
        .inflate_scalar(1.0);
    let root = owner.get_root_vehicle(level);
    let members = match level.get_entity(root) {
        Some(root) => root.get_self_and_passengers(level),
        None => vec![owner.id()],
    };
    !members
        .into_iter()
        .filter_map(|id| level.get_entity(id).or((id == owner.id()).then_some(owner)))
        .any(|e| e.is_pickable() && aabb.intersects_aabb(&e.get_bounding_box()))
}

/// `shoot(xd, yd, zd, pow, uncertainty)`: the (overridable) `Projectile.shoot`.
pub fn shoot(
    projectile: &mut dyn Projectile,
    xd: f64,
    yd: f64,
    zd: f64,
    pow: f32,
    uncertainty: f32,
) {
    projectile.shoot(xd, yd, zd, pow, uncertainty);
}

/// `Projectile.shoot` itself.
pub fn shoot_base(
    projectile: &mut dyn Projectile,
    xd: f64,
    yd: f64,
    zd: f64,
    pow: f32,
    uncertainty: f32,
) {
    let movement = projectile.get_movement_to_shoot(xd, yd, zd, pow, uncertainty);
    let b = projectile.base_mut();
    b.set_delta_movement(movement);
    b.needs_sync = true;
    let sd = movement.horizontal_distance();
    b.set_y_rot((mth::atan2(movement.x, movement.z) * RAD_TO_DEG) as f32);
    b.set_x_rot((mth::atan2(movement.y, sd) * RAD_TO_DEG) as f32);
    b.y_rot_o = b.y_rot();
    b.x_rot_o = b.x_rot();
}

/// `shootFromRotation(source, xRot, yRot, yOffset, pow, uncertainty)`; `source_movement` is `source.getKnownMovement()`
/// and `source_on_ground` its `onGround()`.
pub fn shoot_from_rotation(
    projectile: &mut dyn Projectile,
    source_movement: Vec3,
    source_on_ground: bool,
    x_rot: f32,
    y_rot: f32,
    y_offset: f32,
    pow: f32,
    uncertainty: f32,
) {
    let deg_to_rad = (std::f64::consts::PI / 180.0) as f32;
    let xd = -mth::sin(f64::from(y_rot * deg_to_rad)) * mth::cos(f64::from(x_rot * deg_to_rad));
    let yd = -mth::sin(f64::from((x_rot + y_offset) * deg_to_rad));
    let zd = mth::cos(f64::from(y_rot * deg_to_rad)) * mth::cos(f64::from(x_rot * deg_to_rad));
    shoot(
        projectile,
        f64::from(xd),
        f64::from(yd),
        f64::from(zd),
        pow,
        uncertainty,
    );
    let b = projectile.base_mut();
    b.set_delta_movement(b.delta_movement().add_xyz(
        source_movement.x,
        if source_on_ground {
            0.0
        } else {
            source_movement.y
        },
        source_movement.z,
    ));
}

/// `deflect(deflection, deflectingEntity, newOwner, byAttack, power)`: `Projectile`'s implementation.
pub fn deflect_base(
    projectile: &mut dyn Projectile,
    level: &mut ServerLevel,
    deflection: ProjectileDeflection,
    deflecting_entity: Option<EntityId>,
    new_owner: Option<Uuid>,
    by_attack: bool,
    power: Vec3,
) -> bool {
    let deflector = deflecting_entity
        .and_then(|id| level.get_entity(id))
        .map(|e| Deflector {
            look_angle: e.get_look_angle(),
            known_speed: e.get_known_speed(level),
        });
    deflect_by(
        projectile, level, deflection, deflector, new_owner, by_attack, power,
    )
}

/// `deflect(...)` for a deflecting entity that is out of the level (the attacking player): its view and speed are passed in.
pub fn deflect_by(
    projectile: &mut dyn Projectile,
    level: &mut ServerLevel,
    deflection: ProjectileDeflection,
    deflector: Option<Deflector>,
    new_owner: Option<Uuid>,
    by_attack: bool,
    power: Vec3,
) -> bool {
    deflection.deflect(projectile, deflector.as_ref(), power);
    projectile.set_owner_uuid(new_owner);
    projectile.on_deflection(level, by_attack);
    true
}

/// `hitTargetOrDeflectSelf(hitResult)`.
pub fn hit_target_or_deflect_self(
    projectile: &mut dyn Projectile,
    level: &mut ServerLevel,
    hit: &HitResult,
) -> ProjectileDeflection {
    match hit {
        HitResult::Entity(entity_hit) => {
            let id = entity_hit.entity;
            let deflection = level
                .get_entity(id)
                .map_or(ProjectileDeflection::None, |e| e.deflection(&*projectile));
            if deflection != ProjectileDeflection::None {
                let owner = projectile.projectile_data().owner;
                if projectile.projectile_data().last_deflected_by != Some(id)
                    && projectile.deflect(
                        level,
                        deflection,
                        Some(id),
                        owner,
                        false,
                        Vec3::new(1.0, 1.0, 1.0),
                    )
                {
                    projectile.projectile_data_mut().last_deflected_by = Some(id);
                }
                return deflection;
            }
        }
        HitResult::Block(block_hit) => {
            if !block_hit.world_border_hit {
                if level.get_block_state(block_hit.block_pos).is_air() {
                    return ProjectileDeflection::None;
                }
            } else if projectile.should_bounce_on_world_border() {
                let deflection = ProjectileDeflection::Reverse;
                let owner = projectile.projectile_data().owner;
                if projectile.deflect(
                    level,
                    deflection,
                    None,
                    owner,
                    false,
                    Vec3::new(0.2, 0.2, 0.2),
                ) {
                    return deflection;
                }
            }
        }
    }
    projectile.on_hit(level, hit);
    ProjectileDeflection::None
}

/// `onHit(hitResult)`: `Projectile`'s implementation.
pub fn on_hit_base(projectile: &mut dyn Projectile, level: &mut ServerLevel, hit: &HitResult) {
    match hit {
        HitResult::Entity(entity_hit) => {
            let id = entity_hit.entity;
            let redirectable = level.get_entity(id).is_some_and(|e| {
                e.as_projectile().is_some()
                    && mc_data::tags::get::<mc_data::EntityType>("redirectable_projectile")
                        .is_some_and(|t| t.contains(e.get_type()))
            });
            if redirectable {
                projectile.on_redirect_projectile(level, id);
            }
            projectile.on_hit_entity(level, entity_hit);
            let context = GameEventContext::of_entity_ref(level, &*projectile, None);
            level.post_game_event(GameEvent::PROJECTILE_LAND, entity_hit.location, &context);
        }
        HitResult::Block(block_hit) if !block_hit.miss => {
            projectile.on_hit_block(level, block_hit);
            let target = block_hit.block_pos;
            let state = level.get_block_state(target);
            let context = GameEventContext::of_entity_ref(level, &*projectile, Some(state));
            level.game_event_at_block(GameEvent::PROJECTILE_LAND, target, &context);
        }
        HitResult::Block(_) => {}
    }
}

/// `Projectile.onHitBlock(hitResult)`: `state.onProjectileHit(level, state, hitResult, this)`.
pub fn on_hit_block_base(
    projectile: &mut dyn Projectile,
    level: &mut ServerLevel,
    hit: &BlockHitResult,
) {
    let state = level.get_block_state(hit.block_pos);
    crate::world::level::block::behaviour(state.block())
        .on_projectile_hit(level, state, hit, projectile);
}

/// `updateRotation()`.
pub fn update_rotation(projectile: &mut dyn Projectile) {
    let b = projectile.base_mut();
    let movement = b.delta_movement();
    let sd = movement.horizontal_distance();
    let x = lerp_rotation(b.x_rot_o, (mth::atan2(movement.y, sd) * RAD_TO_DEG) as f32);
    let y = lerp_rotation(
        b.y_rot_o,
        (mth::atan2(movement.x, movement.z) * RAD_TO_DEG) as f32,
    );
    b.set_x_rot(x);
    b.set_y_rot(y);
}

/// `Projectile.lerpRotation(rotO, rot)`.
pub fn lerp_rotation(mut rot_o: f32, rot: f32) -> f32 {
    while rot - rot_o < -180.0 {
        rot_o -= 360.0;
    }
    while rot - rot_o >= 180.0 {
        rot_o += 360.0;
    }
    mth::lerp_f32(0.2, rot_o, rot)
}

/// `Projectile.addAdditionalSaveData`.
pub fn add_additional_save_data(projectile: &dyn Projectile, out: &mut mc_nbt::CompoundTag) {
    let d = projectile.projectile_data();
    if let Some(owner) = d.owner {
        out.put_int_array(
            "Owner",
            mc_core::uuid_util::uuid_to_int_array(&owner).to_vec(),
        );
    }
    if d.left_owner {
        out.put_boolean("LeftOwner", true);
    }
    out.put_boolean("HasBeenShot", d.has_been_shot);
    if let Some(can_break) = &d.can_break
        && let Ok(tag) =
            mc_dfu::encode_nbt(&AdventureModePredicate::codec(), can_break).into_result()
    {
        out.put("can_break", tag);
    }
}

/// `Projectile.readAdditionalSaveData`.
pub fn read_additional_save_data(projectile: &mut dyn Projectile, input: &mc_nbt::CompoundTag) {
    let d = projectile.projectile_data_mut();
    d.owner = super::entity::read_uuid(input.get("Owner"));
    d.owner_id = None;
    d.left_owner = input.get_boolean_or("LeftOwner", false);
    d.has_been_shot = input.get_boolean_or("HasBeenShot", false);
    d.can_break = input.get("can_break").and_then(|t| {
        mc_dfu::parse_nbt(&AdventureModePredicate::codec(), t)
            .into_result()
            .ok()
    });
}

/// `Projectile` fields of the rich scenario dumps (`owner leftowner shot canbreak`).
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static PROJECTILE_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 20,
    applies: |e| e.as_projectile().is_some(),
    fields: |e, _level, ctx| {
        let d = e.as_projectile().expect("projectile").projectile_data();
        vec![
            ("owner".into(), ctx.name_uuid(d.owner)),
            ("leftowner".into(), u8::from(d.left_owner).to_string()),
            ("shot".into(), u8::from(d.has_been_shot).to_string()),
            (
                "canbreak".into(),
                u8::from(d.can_break.is_some()).to_string(),
            ),
        ]
    },
};
