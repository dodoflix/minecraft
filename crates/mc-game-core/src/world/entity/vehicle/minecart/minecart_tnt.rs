//! `net.minecraft.world.entity.vehicle.minecart.MinecartTNT`: a cart that explodes when it is primed (activator rail, fire,
//! explosion, a flaming arrow), falls three blocks or hits something fast.

use super::abstract_minecart::{self, AbstractMinecart, MinecartData};
use crate::minecart_entity_methods;
use crate::server::level::ServerLevel;
use crate::server::level::level_explode::Explode;
use crate::world::damagesource::{DamageSource, DamageTypeTags};
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use crate::world::entity::vehicle::vehicle_entity::{self, VehicleEntity};
use crate::world::entity::{Entity, EntityData, EntityId, RemovalReason};
use crate::world::item::ItemStack;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::explosion::{Explosion, ExplosionInteraction};
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::FluidState;
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;

const DEFAULT_EXPLOSION_POWER_BASE: f32 = 4.0;
const DEFAULT_EXPLOSION_SPEED_FACTOR: f32 = 1.0;

/// The scenario dump fields of a TNT cart: `fuse`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static TNT_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 31,
    applies: |e| e.get_type() == EntityType::TNT_MINECART,
    fields: |e, _level, _ctx| {
        let m = e.as_any().downcast_ref::<MinecartTnt>().expect("applies");
        vec![("fuse".into(), m.fuse.to_string())]
    },
};

pub struct MinecartTnt {
    base: EntityData,
    minecart: MinecartData,
    ignition_source: Option<DamageSource>,
    fuse: i32,
    explosion_power_base: f32,
    explosion_speed_factor: f32,
}

/// What `MinecartTNT` reads from `source.getDirectEntity()` when that is a `Projectile`.
struct DirectProjectile {
    /// `instanceof AbstractArrow`.
    is_arrow: bool,
    /// `isOnFire()`.
    on_fire: bool,
    /// `getDeltaMovement().lengthSqr()`.
    speed_sqr: f64,
}

/// The direct entity of `source` when it is a projectile (the projectile that hit is lent by its own hit).
fn direct_projectile(level: &ServerLevel, source: &DamageSource) -> Option<DirectProjectile> {
    let read = |e: &dyn Entity| {
        e.as_projectile().map(|_| DirectProjectile {
            is_arrow: matches!(
                e.get_type(),
                EntityType::ARROW | EntityType::SPECTRAL_ARROW | EntityType::TRIDENT
            ),
            on_fire: e.is_on_fire(),
            speed_sqr: e.base().delta_movement().length_sqr(),
        })
    };
    read(level.get_entity(source.get_direct_entity()?)?)
}

/// `damageSourceIgnitesTnt(source)`.
fn damage_source_ignites_tnt(level: &ServerLevel, source: &DamageSource) -> bool {
    match direct_projectile(level, source) {
        Some(projectile) => projectile.on_fire,
        None => {
            source.is_tag(level, DamageTypeTags::IS_FIRE)
                || source.is_tag(level, DamageTypeTags::IS_EXPLOSION)
        }
    }
}

/// `damageSources().explosion(this, source.getEntity())`.
fn explosion_source(level: &ServerLevel, cart: &dyn Entity, source: &DamageSource) -> DamageSource {
    let cause = source.get_entity().and_then(|id| level.get_entity(id));
    level.damage_sources().explosion(Some(cart), cause)
}

impl MinecartTnt {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let (base, minecart) = abstract_minecart::new_base(ty, id, 6, |_| {});
        Self {
            base,
            minecart,
            ignition_source: None,
            fuse: -1,
            explosion_power_base: DEFAULT_EXPLOSION_POWER_BASE,
            explosion_speed_factor: DEFAULT_EXPLOSION_SPEED_FACTOR,
        }
    }

    pub fn get_fuse(&self) -> i32 {
        self.fuse
    }

    pub fn is_primed(&self) -> bool {
        self.fuse > -1
    }

    /// `explode(damageSource, speedSqr)`: the cart takes part in nothing of its own explosion and is discarded afterwards.
    fn explode(
        &mut self,
        level: &mut ServerLevel,
        damage_source: Option<DamageSource>,
        speed_sqr: f64,
    ) {
        if level.tnt_explodes() {
            let speed = speed_sqr.sqrt().min(5.0);
            let power = (f64::from(self.explosion_power_base)
                + f64::from(self.explosion_speed_factor)
                    * self.base.random.next_double()
                    * 1.5
                    * speed) as f32;
            let p = self.base.position();
            level.explode(
                Explode::new(
                    Some(self.as_dyn_mut()),
                    p.x,
                    p.y,
                    p.z,
                    power,
                    ExplosionInteraction::Tnt,
                )
                .with_damage_source(damage_source, None),
            );
            self.discard_in_level(level);
        } else if self.is_primed() {
            self.discard_in_level(level);
        }
    }

    /// `discard()` of the running cart: watchers get `remove_entities` now; a cart that is already removed (it explodes twice in
    /// one tick when it hits a wall as its fuse runs out) is left as it is.
    fn discard_in_level(&mut self, level: &mut ServerLevel) {
        if !self.is_removed() {
            level.remove_held_entity(self.as_dyn_mut(), RemovalReason::Discarded);
        }
    }

    /// `primeFuse(source)`.
    pub fn prime_fuse(&mut self, level: &mut ServerLevel, source: Option<&DamageSource>) {
        if level.tnt_explodes() {
            self.fuse = 80;
            if let Some(source) = source
                && self.ignition_source.is_none()
            {
                self.ignition_source = Some(explosion_source(level, &*self, source));
            }
            level.broadcast_entity_event(self.base.id, 70);
            if !self.base.is_silent() {
                let p = self.base.position();
                level.play_sound_at(
                    None,
                    p.x,
                    p.y,
                    p.z,
                    SoundEvent::ENTITY_TNT_PRIMED,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
            }
        }
    }
}

impl AbstractMinecart for MinecartTnt {
    fn minecart(&self) -> &MinecartData {
        &self.minecart
    }
    fn minecart_mut(&mut self) -> &mut MinecartData {
        &mut self.minecart
    }
    fn get_default_display_block_state(&self) -> BlockState {
        Block::TNT.default_state()
    }
    fn activate_minecart(
        &mut self,
        level: &mut ServerLevel,
        _x: i32,
        _y: i32,
        _z: i32,
        state: bool,
    ) {
        if state && self.fuse < 0 {
            self.prime_fuse(level, None);
        }
    }
}

impl VehicleEntity for MinecartTnt {
    fn get_drop_item(&self) -> Item {
        Item::TNT_MINECART
    }

    fn should_source_destroy(&self, level: &ServerLevel, source: &DamageSource) -> bool {
        damage_source_ignites_tnt(level, source)
    }

    fn destroy(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        let speed_sqr = self.base.delta_movement().horizontal_distance_sqr();
        if !damage_source_ignites_tnt(level, source) && !(speed_sqr >= f64::from(0.01f32)) {
            vehicle_entity::destroy_item(self, level, Item::TNT_MINECART);
        } else if self.fuse < 0 {
            self.prime_fuse(level, Some(source));
            self.fuse =
                self.base.random.next_int_bounded(20) + self.base.random.next_int_bounded(20);
        }
    }
}

impl Entity for MinecartTnt {
    minecart_entity_methods!();

    fn tick(&mut self, level: &mut ServerLevel) {
        abstract_minecart::tick(self, level);
        if self.fuse > 0 {
            self.fuse -= 1;
        } else if self.fuse == 0 {
            let speed_sqr = self.base.delta_movement().horizontal_distance_sqr();
            self.explode(level, self.ignition_source.clone(), speed_sqr);
        }
        if self.base.horizontal_collision {
            let speed_sqr = self.base.delta_movement().horizontal_distance_sqr();
            if speed_sqr >= f64::from(0.01f32) {
                self.explode(level, self.ignition_source.clone(), speed_sqr);
            }
        }
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        if let Some(arrow) = direct_projectile(level, source).filter(|p| p.is_arrow && p.on_fire) {
            let damage_source = explosion_source(level, &*self, source);
            self.explode(level, Some(damage_source), arrow.speed_sqr);
        }
        vehicle_entity::hurt_server(self, level, source, amount)
    }

    fn cause_fall_damage(
        &mut self,
        level: &mut ServerLevel,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        if fall_distance >= 3.0 {
            let power = fall_distance / 10.0;
            self.explode(level, self.ignition_source.clone(), power * power);
        }
        crate::world::entity::entity::cause_fall_damage_base(
            self,
            level,
            fall_distance,
            damage_modifier,
            source,
        )
    }

    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::TNT_MINECART))
    }

    fn get_block_explosion_resistance(
        &self,
        _explosion: &dyn Explosion,
        level: &dyn BlockGetter,
        pos: BlockPos,
        block: BlockState,
        _fluid: FluidState,
        resistance: f32,
    ) -> f32 {
        if !self.is_primed()
            || !block.is_tag("rails") && !level.get_block_state(pos.above()).is_tag("rails")
        {
            resistance
        } else {
            0.0
        }
    }

    fn should_block_explode(
        &self,
        _explosion: &dyn Explosion,
        level: &dyn BlockGetter,
        pos: BlockPos,
        state: BlockState,
        _power: f32,
    ) -> bool {
        !self.is_primed()
            || !state.is_tag("rails") && !level.get_block_state(pos.above()).is_tag("rails")
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_minecart::add_additional_save_data(self, out);
        out.put_int("fuse", self.fuse);
        if self.explosion_power_base != DEFAULT_EXPLOSION_POWER_BASE {
            out.put_float("explosion_power", self.explosion_power_base);
        }
        if self.explosion_speed_factor != DEFAULT_EXPLOSION_SPEED_FACTOR {
            out.put_float("explosion_speed_factor", self.explosion_speed_factor);
        }
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_minecart::read_additional_save_data(self, input);
        self.fuse = input.get_int_or("fuse", -1);
        self.explosion_power_base = mth::clamp_f32(
            input.get_float_or("explosion_power", DEFAULT_EXPLOSION_POWER_BASE),
            0.0,
            128.0,
        );
        self.explosion_speed_factor = mth::clamp_f32(
            input.get_float_or("explosion_speed_factor", DEFAULT_EXPLOSION_SPEED_FACTOR),
            0.0,
            128.0,
        );
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static MINECART_TNT_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::TNT_MINECART,
    create: |ty, id, _registries| Box::new(MinecartTnt::new(ty, id)),
};
