//! `net.minecraft.world.entity.projectile.throwableitemprojectile.AbstractThrownPotion`: the hit logic shared by the
//! splash and the lingering potion. The concrete class passes its `onHitAsPotion` to [`on_hit`].

use super::super::throwable;
use super::super::{HitResult, Projectile, on_hit_base, on_hit_block_base};
use crate::server::level::ServerLevel;
use crate::world::effect::potion_contents;
use crate::world::item::ItemStack;
use crate::world::level::block::impls::{
    abstract_candle_block,
    campfire_block::{self, CampfireBlock},
};
use crate::world::level::block::state::BlockStateExt;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::phys::BlockHitResult;
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_protocol::components::{POTION_CONTENTS, PotionContents};

/// `AbstractThrownPotion.SPLASH_RANGE_SQ`.
const SPLASH_RANGE_SQ: f64 = 16.0;

/// `onHitAsPotion(level, potionItem, hitResult)` of the concrete class.
pub type OnHitAsPotion = fn(&mut dyn Projectile, &mut ServerLevel, &ItemStack, &HitResult);

/// `potionItemStack.getOrDefault(DataComponents.POTION_CONTENTS, PotionContents.EMPTY)`.
pub fn contents_of(stack: &ItemStack) -> PotionContents {
    stack
        .get(&POTION_CONTENTS)
        .cloned()
        .unwrap_or_else(potion_contents::empty_contents)
}

/// `Entity.thrown_potion_hurts_water_sensitive` of both potion classes.
pub fn hurts_water_sensitive(p: &dyn Projectile) -> bool {
    potion_contents::is_in_tag(
        &contents_of(&throwable::get_item(p.base())),
        "hurts_water_sensitive_entities",
    )
}

/// `onHitBlock(hitResult)`: `Projectile.onHitBlock`, then dousing the fire around the hit face for a `DOUSES_FIRE` potion.
pub fn on_hit_block(p: &mut dyn Projectile, level: &mut ServerLevel, hit: &BlockHitResult) {
    on_hit_block_base(p, level, hit);
    let contents = contents_of(&throwable::get_item(p.base()));
    if potion_contents::is_in_tag(&contents, "douses_fire") {
        let direction = hit.direction;
        let block_effect_pos = hit.block_pos.relative(direction);
        douse_fire(p, level, block_effect_pos);
        douse_fire(
            p,
            level,
            block_effect_pos.relative(direction.get_opposite()),
        );
        for d in Direction::HORIZONTAL {
            douse_fire(p, level, block_effect_pos.relative(d));
        }
    }
}

/// `onHit(hitResult)`.
pub fn on_hit(
    p: &mut dyn Projectile,
    level: &mut ServerLevel,
    hit: &HitResult,
    on_hit_as_potion: OnHitAsPotion,
) {
    on_hit_base(p, level, hit);
    let stack = throwable::get_item(p.base());
    let contents = contents_of(&stack);
    affect_entities_around(p, level, &contents);
    if potion_contents::has_effects(&contents) {
        on_hit_as_potion(p, level, &stack, hit);
    }
    let pos = p.block_position();
    let color = potion_contents::get_color(&contents);
    let silent = p.base().is_silent();
    if contents
        .potion
        .is_some_and(potion_contents::has_instant_effects)
    {
        level.level_event(None, 2007, pos, color);
        if !silent {
            level.level_event(None, 1054, pos, 0);
        }
    } else {
        level.level_event(None, 2002, pos, color);
        if !silent {
            level.level_event(None, 1053, pos, 0);
        }
    }
    p.discard();
}

/// `affectEntitiesAround(level, potion)`: water bottles hurt water-sensitive mobs, put out burning ones and rehydrate
/// axolotls within 4 blocks.
fn affect_entities_around(
    p: &mut dyn Projectile,
    level: &mut ServerLevel,
    contents: &PotionContents,
) {
    let hurts_water_sensitive =
        potion_contents::is_in_tag(contents, "hurts_water_sensitive_entities");
    let extinguishes = potion_contents::is_in_tag(contents, "extinguishes_entities");
    let aabb = p.get_bounding_box().inflate(4.0, 2.0, 4.0);
    if hurts_water_sensitive || extinguishes {
        let victims = level.get_entities_of_class(
            &aabb,
            |e| e.living().is_some(),
            |e| e.is_sensitive_to_water() || e.is_on_fire(),
        );
        for id in victims {
            let Some(entity) = level.get_entity(id) else {
                continue;
            };
            if p.base().distance_to_sqr_vec(entity.position()) < SPLASH_RANGE_SQ {
                if hurts_water_sensitive && entity.is_sensitive_to_water() {
                    let source = level
                        .damage_sources()
                        .indirect_magic(p.as_dyn(), p.get_owner(level));
                    level.with_entity(id, |level, e| e.hurt_server(level, &source, 1.0));
                }
                if extinguishes {
                    level.with_entity(id, |level, e| {
                        if e.is_on_fire() && e.is_alive() {
                            e.extinguish_fire(level);
                        }
                    });
                }
            }
        }
    }
    if potion_contents::is_in_tag(contents, "rehydrates_axolotls") {
        for id in
            level.get_entities_of_class(&aabb, |e| e.get_type() == EntityType::AXOLOTL, |_| true)
        {
            level.with_entity(id, |_, e| e.rehydrate());
        }
    }
}

/// `douseFire(pos)`: fire is destroyed, candles and campfires are put out.
fn douse_fire(p: &mut dyn Projectile, level: &mut ServerLevel, pos: BlockPos) {
    let state = level.get_block_state(pos);
    if state.is_tag("fire") {
        let id = p.id();
        level.lend(p.as_dyn_mut(), |level| {
            level.destroy_block(pos, false, Some(id), 512)
        });
    } else if abstract_candle_block::is_lit(state) {
        abstract_candle_block::extinguish(None, state, level, pos);
    } else if CampfireBlock::is_lit_campfire(state) {
        level.level_event(None, 1009, pos, 0);
        let owner = p.get_owner(level).map(|o| o.id());
        campfire_block::douse(owner, level, pos);
        level.set_block_and_update(pos, state.set(mc_data::props::LIT, false));
    }
}
