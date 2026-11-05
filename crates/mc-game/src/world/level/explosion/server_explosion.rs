//! `net.minecraft.world.level.ServerExplosion`.

use super::{
    BlockInteraction, DefaultExplosionDamageCalculator, EntityBasedExplosionDamageCalculator, Explosion, ExplosionDamageCalculator,
    get_indirect_source_entity,
};
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::movement::collision_context;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::{ItemStack, ItemStackExt};
use crate::world::level::block::{BlockStateExt, behaviour};
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid};
use crate::world::level::levelgen::feature::tree::java_hash_set::BlockPosHashSet;
use crate::world::level::material::FluidStateExt as _;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::GameEvent;

const MAX_DROPS_PER_COMBINED_STACK: i32 = 16;
const LARGE_EXPLOSION_RADIUS: f32 = 2.0;

/// The calculator of an explosion: the caller's, else the default / entity based one (`makeDamageCalculator`).
enum Calculator<'a> {
    Default(DefaultExplosionDamageCalculator),
    EntityBased(EntityBasedExplosionDamageCalculator<'a>),
    Custom(&'a dyn ExplosionDamageCalculator),
}

impl Calculator<'_> {
    fn get(&self) -> &dyn ExplosionDamageCalculator {
        match self {
            Calculator::Default(c) => c,
            Calculator::EntityBased(c) => c,
            Calculator::Custom(c) => *c,
        }
    }
}

/// `ServerExplosion.StackCollector`.
struct StackCollector {
    pos: BlockPos,
    stack: ItemStack,
}

impl StackCollector {
    fn try_merge(&mut self, input: &mut ItemStack) {
        if crate::world::entity::item::are_mergable(&self.stack, input) {
            self.stack = crate::world::entity::item::merge_stacks(&self.stack, input, MAX_DROPS_PER_COMBINED_STACK);
        }
    }
}

/// `ServerExplosion`.
pub struct ServerExplosion<'a> {
    fire: bool,
    block_interaction: BlockInteraction,
    center: Vec3,
    source: Option<&'a dyn Entity>,
    radius: f32,
    damage_source: DamageSource,
    damage_calculator: Calculator<'a>,
    /// `hitPlayers` (player id -> knockback; only looked up by player).
    hit_players: Vec<(EntityId, Vec3)>,
    indirect_source: Option<EntityId>,
    can_trigger_blocks: bool,
    should_affect_blocklike_entities: bool,
}

impl<'a> ServerExplosion<'a> {
    /// `new ServerExplosion(level, source, damageSource, damageCalculator, center, radius, fire, blockInteraction)`.
    /// `source` is the exploding entity (it is usually taken out of the level while it runs, hence passed explicitly).
    pub fn new(
        level: &ServerLevel,
        source: Option<&'a dyn Entity>,
        damage_source: Option<DamageSource>,
        damage_calculator: Option<&'a dyn ExplosionDamageCalculator>,
        center: Vec3,
        radius: f32,
        fire: bool,
        block_interaction: BlockInteraction,
    ) -> Self {
        let indirect_source = get_indirect_source_entity(level, source);
        let damage_source = damage_source.unwrap_or_else(|| {
            let indirect = indirect_source.and_then(|id| resolve(level, source, id));
            level.damage_sources().explosion(source, indirect)
        });
        let damage_calculator = match (damage_calculator, source) {
            (Some(c), _) => Calculator::Custom(c),
            (None, None) => Calculator::Default(DefaultExplosionDamageCalculator),
            (None, Some(s)) => Calculator::EntityBased(EntityBasedExplosionDamageCalculator { source: s }),
        };
        let is_breeze_wind_charge = source.is_some_and(|s| s.get_type() == EntityType::BREEZE_WIND_CHARGE);
        let is_wind_charge = source.is_some_and(|s| s.get_type() == EntityType::WIND_CHARGE);
        let can_trigger_blocks = if block_interaction != BlockInteraction::TriggerBlock {
            false
        } else if is_breeze_wind_charge {
            level.mob_griefing()
        } else {
            true
        };
        let is_not_wind_charge = !is_breeze_wind_charge && !is_wind_charge;
        let should_affect_blocklike_entities =
            if level.mob_griefing() { is_not_wind_charge } else { block_interaction.should_affect_blocklike_entities() && is_not_wind_charge };
        Self {
            fire,
            block_interaction,
            center,
            source,
            radius,
            damage_source,
            damage_calculator,
            hit_players: Vec::new(),
            indirect_source,
            can_trigger_blocks,
            should_affect_blocklike_entities,
        }
    }

    /// `ServerExplosion.getSeenPercent(center, entity)`.
    pub fn get_seen_percent(level: &ServerLevel, center: Vec3, entity: &dyn Entity) -> f32 {
        let bb = entity.get_bounding_box();
        let xs = 1.0 / ((bb.max_x - bb.min_x) * 2.0 + 1.0);
        let ys = 1.0 / ((bb.max_y - bb.min_y) * 2.0 + 1.0);
        let zs = 1.0 / ((bb.max_z - bb.min_z) * 2.0 + 1.0);
        let x_offset = (1.0 - (1.0 / xs).floor() * xs) / 2.0;
        let z_offset = (1.0 - (1.0 / zs).floor() * zs) / 2.0;
        if !(xs < 0.0) && !(ys < 0.0) && !(zs < 0.0) {
            let mut hits = 0;
            let mut count = 0;
            let context = collision_context(entity);
            let mut xx = 0.0;
            while xx <= 1.0 {
                let mut yy = 0.0;
                while yy <= 1.0 {
                    let mut zz = 0.0;
                    while zz <= 1.0 {
                        let x = mth::lerp_f64(xx, bb.min_x, bb.max_x);
                        let y = mth::lerp_f64(yy, bb.min_y, bb.max_y);
                        let z = mth::lerp_f64(zz, bb.min_z, bb.max_z);
                        let from = Vec3::new(x + x_offset, y, z + z_offset);
                        let clip = ClipContext::new(from, center, ClipBlock::Collider, ClipFluid::None, context.clone());
                        if level.clip(&clip).miss {
                            hits += 1;
                        }
                        count += 1;
                        zz += zs;
                    }
                    yy += ys;
                }
                xx += xs;
            }
            hits as f32 / count as f32
        } else {
            0.0
        }
    }

    /// `calculateExplodedPositions()`: the `HashSet<BlockPos>` of the rays in Java iteration order.
    fn calculate_exploded_positions(&self, level: &mut ServerLevel) -> Vec<BlockPos> {
        let calculator = self.damage_calculator.get();
        let mut to_blow_set = BlockPosHashSet::new();
        for xx in 0..16 {
            for yy in 0..16 {
                for zz in 0..16 {
                    if xx == 0 || xx == 15 || yy == 0 || yy == 15 || zz == 0 || zz == 15 {
                        let mut xd = f64::from(xx as f32 / 15.0 * 2.0 - 1.0);
                        let mut yd = f64::from(yy as f32 / 15.0 * 2.0 - 1.0);
                        let mut zd = f64::from(zz as f32 / 15.0 * 2.0 - 1.0);
                        let d = (xd * xd + yd * yd + zd * zd).sqrt();
                        xd /= d;
                        yd /= d;
                        zd /= d;
                        let mut remaining_power = self.radius * (0.7 + LevelAccessor::random(level).next_float() * 0.6);
                        let mut xp = self.center.x;
                        let mut yp = self.center.y;
                        let mut zp = self.center.z;
                        while remaining_power > 0.0 {
                            let pos = BlockPos::containing(xp, yp, zp);
                            level.ensure_chunk_loaded(pos);
                            let block = level.get_block_state(pos);
                            let fluid = level.get_fluid_state(pos);
                            if !level.is_in_world_bounds(pos) {
                                break;
                            }
                            if let Some(resistance) = calculator.get_block_explosion_resistance(self, level, pos, block, fluid) {
                                remaining_power -= (resistance + 0.3) * 0.3;
                            }
                            if remaining_power > 0.0 && calculator.should_block_explode(self, level, pos, block, remaining_power) {
                                to_blow_set.insert(pos);
                            }
                            xp += xd * f64::from(0.3f32);
                            yp += yd * f64::from(0.3f32);
                            zp += zd * f64::from(0.3f32);
                            remaining_power -= 0.225_000_01;
                        }
                    }
                }
            }
        }
        to_blow_set.iter().collect()
    }

    /// `hurtEntities()`. `participants` are entities that currently live outside the level's storage (the acting
    /// player of a bed/anchor use is taken out while its packet is handled): they take part like level entities, after them.
    fn hurt_entities(&mut self, level: &mut ServerLevel, participants: &mut [&mut dyn Entity]) {
        if self.radius < 1.0E-5 {
            return;
        }
        let double_radius = self.radius * 2.0;
        let dr = f64::from(double_radius);
        let x0 = mth::floor_f64(self.center.x - dr - 1.0);
        let x1 = mth::floor_f64(self.center.x + dr + 1.0);
        let y0 = mth::floor_f64(self.center.y - dr - 1.0);
        let y1 = mth::floor_f64(self.center.y + dr + 1.0);
        let z0 = mth::floor_f64(self.center.z - dr - 1.0);
        let z1 = mth::floor_f64(self.center.z + dr + 1.0);
        let area = AABB::new(f64::from(x0), f64::from(y0), f64::from(z0), f64::from(x1), f64::from(y1), f64::from(z1));
        let mut targets: Vec<Target> = level
            .get_entities(self.source.map(|s| s.id()), &area, |e| !e.is_spectator())
            .into_iter()
            .map(Target::Level)
            .collect();
        for (i, p) in participants.iter().enumerate() {
            if !p.is_spectator() && p.get_bounding_box().intersects_aabb(&area) && self.source.is_none_or(|s| s.id() != p.id()) {
                targets.push(Target::External(i));
            }
        }
        for target in targets {
            match target {
                Target::Level(id) => {
                    let Some(entity) = level.get_entity(id) else { continue };
                    let Some(plan) = self.plan(level, entity, double_radius) else { continue };
                    level.with_entity(id, |level, entity| self.apply(level, entity, plan));
                }
                Target::External(i) => {
                    let entity: &mut dyn Entity = &mut *participants[i];
                    let Some(plan) = self.plan(level, entity, double_radius) else { continue };
                    self.apply(level, entity, plan);
                }
            }
        }
    }

    /// The reads of `hurtEntities` before an entity is hurt: `None` when the entity is not affected at all.
    fn plan(&self, level: &ServerLevel, entity: &dyn Entity, double_radius: f32) -> Option<EntityPlan> {
        if entity.ignore_explosion(self) {
            return None;
        }
        let calculator = self.damage_calculator.get();
        let dist = entity.base().distance_to_sqr_vec(self.center).sqrt() / f64::from(double_radius);
        if dist > 1.0 {
            return None;
        }
        let origin = if entity.get_type() == EntityType::TNT { entity.position() } else { entity.base().get_eye_position() };
        let direction = origin.subtract(self.center).normalize();
        let should_damage_entity = calculator.should_damage_entity(self, entity);
        let knockback_multiplier = calculator.get_knockback_multiplier(entity);
        let exposure = if !should_damage_entity && knockback_multiplier == 0.0 { 0.0 } else { Self::get_seen_percent(level, self.center, entity) };
        let damage = (should_damage_entity && self.source.is_none_or(|s| crate::world::entity::team::do_teams_allow_damage(level, s, entity)))
            .then(|| calculator.get_entity_damage_amount(self, entity, exposure));
        Some(EntityPlan { dist, direction, knockback_multiplier, exposure, damage })
    }

    fn apply(&mut self, level: &mut ServerLevel, entity: &mut dyn Entity, plan: EntityPlan) {
        if let Some(amount) = plan.damage {
            entity.hurt_server(level, &self.damage_source, amount);
        }
        let knockback_resistance = entity.living().map_or(0.0, |l| l.get_attribute_value(mc_data::Attribute::EXPLOSION_KNOCKBACK_RESISTANCE));
        let knockback_power = (1.0 - plan.dist) * f64::from(plan.exposure) * f64::from(plan.knockback_multiplier) * (1.0 - knockback_resistance);
        let knockback = plan.direction.scale(knockback_power);
        entity.push_from_explosion(knockback);
        let redirectable = mc_data::tags::get::<EntityType>("redirectable_projectile").is_some_and(|t| t.contains(entity.get_type()));
        if redirectable && entity.as_projectile().is_some() {
            let owner = self.damage_source.get_entity().and_then(|id| resolve(level, self.source, id)).map(|e| e.uuid());
            if let Some(projectile) = entity.as_projectile_mut() {
                projectile.set_owner_uuid(owner);
            }
        } else if let Some(player) = entity.player()
            && !entity.is_spectator()
            && (!player.abilities.instabuild || !player.abilities.flying)
        {
            self.hit_players.push((entity.id(), knockback));
        }
    }

    /// `interactWithBlocks(targetBlocks)`.
    fn interact_with_blocks(&self, level: &mut ServerLevel, target_blocks: &mut [BlockPos]) {
        let mut stacks: Vec<StackCollector> = Vec::new();
        shuffle(target_blocks, LevelAccessor::random(level));
        for &pos in target_blocks.iter() {
            let state = level.get_block_state(pos);
            behaviour(state.block()).on_explosion_hit(state, level, pos, self, &mut |stack, position| add_or_append_stack(&mut stacks, stack, position));
        }
        for stack in stacks {
            crate::world::level::block::helpers::pop_resource(level, stack.pos, stack.stack);
        }
    }

    /// `createFire(targetBlocks)`.
    fn create_fire(&self, level: &mut ServerLevel, target_blocks: &[BlockPos]) {
        for &pos in target_blocks {
            if LevelAccessor::random(level).next_int_bounded(3) == 0
                && level.get_block_state(pos).is_air()
                && level.get_block_state(pos.below()).is_solid_render()
            {
                let fire = crate::world::level::block::impls::base_fire_block::get_state(&*level, pos);
                level.set_block_and_update(pos, fire);
            }
        }
    }

    /// `explode()`: returns the number of exploded block positions (`blockCount` of the packet).
    pub fn explode(&mut self, level: &mut ServerLevel, participants: &mut [&mut dyn Entity]) -> usize {
        level.game_event_from_entity(self.source.map(|s| s.id()), GameEvent::EXPLODE, self.center);
        let mut to_blow = self.calculate_exploded_positions(level);
        self.hurt_entities(level, participants);
        if self.interacts_with_blocks() {
            self.interact_with_blocks(level, &mut to_blow);
        }
        if self.fire {
            self.create_fire(level, &to_blow);
        }
        to_blow.len()
    }

    fn interacts_with_blocks(&self) -> bool {
        self.block_interaction != BlockInteraction::Keep
    }

    /// `getHitPlayers().get(player)`.
    pub fn get_hit_player(&self, player: EntityId) -> Option<Vec3> {
        self.hit_players.iter().find(|(id, _)| *id == player).map(|(_, v)| *v)
    }

    pub fn get_damage_source(&self) -> &DamageSource {
        &self.damage_source
    }

    /// `isSmall()`.
    pub fn is_small(&self) -> bool {
        self.radius < LARGE_EXPLOSION_RADIUS || !self.interacts_with_blocks()
    }
}

enum Target {
    Level(EntityId),
    External(usize),
}

struct EntityPlan {
    dist: f64,
    direction: Vec3,
    knockback_multiplier: f32,
    exposure: f32,
    /// The damage to deal (`None` = this explosion does not damage the entity).
    damage: Option<f32>,
}

impl Explosion for ServerExplosion<'_> {
    fn get_block_interaction(&self) -> BlockInteraction {
        self.block_interaction
    }
    fn get_indirect_source_entity(&self) -> Option<EntityId> {
        self.indirect_source
    }
    fn get_direct_source_entity(&self) -> Option<EntityId> {
        self.source.map(|s| s.id())
    }
    fn get_direct_source_entity_ref(&self) -> Option<&dyn Entity> {
        self.source
    }
    fn radius(&self) -> f32 {
        self.radius
    }
    fn center(&self) -> Vec3 {
        self.center
    }
    fn can_trigger_blocks(&self) -> bool {
        self.can_trigger_blocks
    }
    fn should_affect_blocklike_entities(&self) -> bool {
        self.should_affect_blocklike_entities
    }
}

/// The entity `id` as a reference: the explicitly passed `source` (which is outside the level) or the level's entity.
fn resolve<'l>(level: &'l ServerLevel, source: Option<&'l dyn Entity>, id: EntityId) -> Option<&'l dyn Entity> {
    match source {
        Some(s) if s.id() == id => Some(s),
        _ => level.get_entity(id),
    }
}

/// `Util.shuffle(list, random)`.
fn shuffle<T>(list: &mut [T], random: &mut dyn RandomSource) {
    let mut i = list.len();
    while i > 1 {
        let swap_to = random.next_int_bounded(i as i32) as usize;
        list.swap(i - 1, swap_to);
        i -= 1;
    }
}

/// `ServerExplosion.addOrAppendStack(stacks, stack, pos)`.
fn add_or_append_stack(stacks: &mut Vec<StackCollector>, mut stack: ItemStack, pos: BlockPos) {
    for collector in stacks.iter_mut() {
        collector.try_merge(&mut stack);
        if stack.is_empty() {
            return;
        }
    }
    stacks.push(StackCollector { pos, stack });
}
