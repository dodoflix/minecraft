//! `net.minecraft.world.level.{Explosion, ExplosionDamageCalculator, EntityBasedExplosionDamageCalculator,
//! SimpleExplosionDamageCalculator}` and `Level.ExplosionInteraction`; the explosion itself is [`server_explosion`].
//!
//! Deviations from the Java shapes (forced by ownership, no observable effect): `Explosion` never hands out the level
//! (callers already hold it), entities are referenced by [`EntityId`], and the explosion's indirect source /
//! "can trigger blocks" / "affect blocklike entities" answers are computed once when the explosion is created.

pub mod java_hash_set;
pub mod server_explosion;

use crate::world::entity::{Entity, EntityId};
use crate::world::level::block::BlockStateExt;
use crate::world::level::material::FluidStateExt;
use crate::world::level::{BlockGetter, LevelAccessor as _};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::FluidState;

pub use server_explosion::ServerExplosion;

/// `Explosion.BlockInteraction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockInteraction {
    Keep,
    Destroy,
    DestroyWithDecay,
    TriggerBlock,
}

impl BlockInteraction {
    pub fn should_affect_blocklike_entities(self) -> bool {
        matches!(
            self,
            BlockInteraction::Destroy | BlockInteraction::DestroyWithDecay
        )
    }
}

/// `Level.ExplosionInteraction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExplosionInteraction {
    None,
    Block,
    Mob,
    Tnt,
    Trigger,
}

/// `Explosion` (the interface blocks, entities and calculators see).
pub trait Explosion {
    fn get_block_interaction(&self) -> BlockInteraction;
    /// `getIndirectSourceEntity()`: the living entity responsible (see [`get_indirect_source_entity`]).
    fn get_indirect_source_entity(&self) -> Option<EntityId>;
    /// `getDirectSourceEntity()`.
    fn get_direct_source_entity(&self) -> Option<EntityId>;
    /// `getDirectSourceEntity()` as the entity itself (it usually lives outside the level while it runs).
    fn get_direct_source_entity_ref(&self) -> Option<&dyn Entity>;
    fn radius(&self) -> f32;
    fn center(&self) -> Vec3;
    fn can_trigger_blocks(&self) -> bool;
    fn should_affect_blocklike_entities(&self) -> bool;
    /// `explosion instanceof ServerExplosion serverExplosion ? serverExplosion.getDamageSource() : null`.
    fn server_damage_source(&self) -> Option<&crate::world::damagesource::DamageSource> {
        None
    }
}

/// `Explosion.getIndirectSourceEntity(source)`: a primed TNT's owner, a living source itself, a projectile's living owner.
pub fn get_indirect_source_entity(
    level: &crate::server::level::ServerLevel,
    source: Option<&dyn Entity>,
) -> Option<EntityId> {
    source?.explosion_indirect_source(level)
}

/// `ExplosionDamageCalculator` (the base class is [`DefaultExplosionDamageCalculator`]).
pub trait ExplosionDamageCalculator {
    /// `getBlockExplosionResistance(explosion, level, pos, block, fluid)`.
    fn get_block_explosion_resistance(
        &self,
        _explosion: &dyn Explosion,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        block: BlockState,
        fluid: FluidState,
    ) -> Option<f32> {
        default_block_explosion_resistance(block, fluid)
    }

    /// `shouldBlockExplode(explosion, level, pos, state, power)`.
    fn should_block_explode(
        &self,
        _explosion: &dyn Explosion,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        _state: BlockState,
        _power: f32,
    ) -> bool {
        true
    }

    /// `shouldDamageEntity(explosion, entity)`.
    fn should_damage_entity(&self, _explosion: &dyn Explosion, _entity: &dyn Entity) -> bool {
        true
    }

    /// `getKnockbackMultiplier(entity)`.
    fn get_knockback_multiplier(&self, _entity: &dyn Entity) -> f32 {
        1.0
    }

    /// `getEntityDamageAmount(explosion, entity, exposure)`.
    fn get_entity_damage_amount(
        &self,
        explosion: &dyn Explosion,
        entity: &dyn Entity,
        exposure: f32,
    ) -> f32 {
        let double_radius = explosion.radius() * 2.0;
        let center = explosion.center();
        let dist = entity.base().distance_to_sqr_vec(center).sqrt() / f64::from(double_radius);
        let pow = (1.0 - dist) * f64::from(exposure);
        ((pow * pow + pow) / 2.0 * 7.0 * f64::from(double_radius) + 1.0) as f32
    }
}

/// `ExplosionDamageCalculator.getBlockExplosionResistance` (the base behavior the subclasses call as `super`).
pub fn default_block_explosion_resistance(block: BlockState, fluid: FluidState) -> Option<f32> {
    if block.is_air() && fluid.is_empty() {
        None
    } else {
        Some(java_max_f32(
            block.block().data().explosion_resistance,
            fluid.get_explosion_resistance(),
        ))
    }
}

/// `Math.max(float, float)` (NaN wins, `-0.0 < 0.0`).
fn java_max_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() { b } else { a }
    } else if a >= b {
        a
    } else {
        b
    }
}

/// `new ExplosionDamageCalculator()` (`ServerExplosion.EXPLOSION_DAMAGE_CALCULATOR`).
pub struct DefaultExplosionDamageCalculator;

impl ExplosionDamageCalculator for DefaultExplosionDamageCalculator {}

/// `EntityBasedExplosionDamageCalculator(source)`.
pub struct EntityBasedExplosionDamageCalculator<'a> {
    pub source: &'a dyn Entity,
}

impl ExplosionDamageCalculator for EntityBasedExplosionDamageCalculator<'_> {
    fn get_block_explosion_resistance(
        &self,
        explosion: &dyn Explosion,
        level: &dyn BlockGetter,
        pos: BlockPos,
        block: BlockState,
        fluid: FluidState,
    ) -> Option<f32> {
        default_block_explosion_resistance(block, fluid).map(|resistance| {
            self.source
                .get_block_explosion_resistance(explosion, level, pos, block, fluid, resistance)
        })
    }

    fn should_block_explode(
        &self,
        explosion: &dyn Explosion,
        level: &dyn BlockGetter,
        pos: BlockPos,
        state: BlockState,
        power: f32,
    ) -> bool {
        self.source
            .should_block_explode(explosion, level, pos, state, power)
    }
}

/// `HolderSet<Block>` of `SimpleExplosionDamageCalculator.immuneBlocks`.
#[derive(Clone, Debug)]
pub enum ImmuneBlocks {
    Tag(&'static str),
    List(Vec<Block>),
}

impl ImmuneBlocks {
    fn contains(&self, state: BlockState) -> bool {
        match self {
            ImmuneBlocks::Tag(tag) => state.is_tag(tag),
            ImmuneBlocks::List(blocks) => blocks.contains(&state.block()),
        }
    }
}

/// `SimpleExplosionDamageCalculator(explodesBlocks, damagesEntities, knockbackMultiplier, immuneBlocks)`.
#[derive(Clone, Debug)]
pub struct SimpleExplosionDamageCalculator {
    pub explodes_blocks: bool,
    pub damages_entities: bool,
    pub knockback_multiplier: Option<f32>,
    pub immune_blocks: Option<ImmuneBlocks>,
}

impl ExplosionDamageCalculator for SimpleExplosionDamageCalculator {
    fn get_block_explosion_resistance(
        &self,
        _explosion: &dyn Explosion,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        block: BlockState,
        fluid: FluidState,
    ) -> Option<f32> {
        match &self.immune_blocks {
            Some(immune) => immune.contains(block).then_some(3_600_000.0),
            None => default_block_explosion_resistance(block, fluid),
        }
    }

    fn should_block_explode(
        &self,
        _explosion: &dyn Explosion,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        _state: BlockState,
        _power: f32,
    ) -> bool {
        self.explodes_blocks
    }

    fn should_damage_entity(&self, _explosion: &dyn Explosion, _entity: &dyn Entity) -> bool {
        self.damages_entities
    }

    fn get_knockback_multiplier(&self, entity: &dyn Entity) -> f32 {
        let creative_flying = entity.player().is_some_and(|p| p.abilities.flying);
        if creative_flying {
            0.0
        } else {
            self.knockback_multiplier.unwrap_or(1.0)
        }
    }
}

/// `BlockBehaviour.onExplosionHit` (the base behavior overrides call as `super`): drops go to `on_hit`, the block is
/// removed and `wasExploded` runs.
pub fn default_on_explosion_hit(
    state: BlockState,
    level: &mut crate::server::level::ServerLevel,
    pos: BlockPos,
    explosion: &dyn Explosion,
    on_hit: &mut dyn FnMut(crate::world::item::ItemStack, BlockPos),
) {
    use crate::world::level::block::behaviour;
    if state.is_air() || explosion.get_block_interaction() == BlockInteraction::TriggerBlock {
        return;
    }
    let block = state.block();
    let do_drop_experience_hack = explosion
        .get_indirect_source_entity()
        .is_some_and(|id| level.players.contains(&id));
    if behaviour(block).drop_from_explosion(explosion) {
        // `spawnAfterBreak` only pops experience: reading the block entity after it sees the same data as vanilla's params
        behaviour(block).spawn_after_break(
            state,
            level,
            pos,
            &crate::world::item::ItemStack::empty(),
            do_drop_experience_hack,
        );
        let radius = (explosion.get_block_interaction() == BlockInteraction::DestroyWithDecay)
            .then(|| explosion.radius());
        behaviour(block).before_get_drops(
            state,
            level,
            pos,
            explosion
                .get_direct_source_entity_ref()
                .map(|e| e.get_type()),
        );
        let block_entity = if state.has_block_entity() {
            level.get_block_entity(pos)
        } else {
            None
        };
        let drops = crate::world::level::storage::loot::block_drops::block_drops_explosion(
            level,
            state,
            pos,
            block_entity,
            explosion.get_direct_source_entity_ref(),
            radius,
        );
        for stack in drops {
            on_hit(stack, pos);
        }
    }
    level.set_block_and_update(pos, mc_data::block::Block::AIR.default_state());
    behaviour(block).was_exploded(level, pos, explosion);
}
