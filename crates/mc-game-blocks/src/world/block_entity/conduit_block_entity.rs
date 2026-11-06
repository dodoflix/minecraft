//! `net.minecraft.world.level.block.entity.ConduitBlockEntity` (server side: the client tick only animates).

use linkme::distributed_slice;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, MobEffect, SoundEvent};
use mc_game_core::block_entity_base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec,
};
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::level::block::{BlockStateExt, helpers};
use mc_game_core::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_nbt::CompoundTag;
use mc_protocol::components::uuid_codec;
use mc_protocol::packets::play::level::{ClientboundBlockEntityDataPacket, SoundSource};
use uuid::Uuid;

const MIN_ACTIVE_SIZE: usize = 16;
const MIN_KILL_SIZE: usize = 42;
const KILL_RANGE: f64 = 8.0;

pub struct ConduitBlockEntity {
    base: BlockEntityData,
    pub tick_count: i32,
    is_active: bool,
    is_hunting: bool,
    effect_blocks: Vec<BlockPos>,
    /// `destroyTarget` (an `EntityReference<LivingEntity>`: the target's UUID).
    destroy_target: Option<Uuid>,
    next_ambient_sound_activation: i64,
}

impl ConduitBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::CONDUIT, pos, state),
            tick_count: 0,
            is_active: false,
            is_hunting: false,
            effect_blocks: Vec::new(),
            destroy_target: None,
            next_ambient_sound_activation: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn is_hunting(&self) -> bool {
        self.is_hunting
    }
}

/// `updateShape(level, worldPosition, effectBlocks)`.
fn update_shape(
    level: &ServerLevel,
    world_position: BlockPos,
    effect_blocks: &mut Vec<BlockPos>,
) -> bool {
    effect_blocks.clear();
    for ox in -1..=1 {
        for oy in -1..=1 {
            for oz in -1..=1 {
                if !helpers::is_water_at(
                    level as &dyn LevelReader,
                    world_position.offset(ox, oy, oz),
                ) {
                    return false;
                }
            }
        }
    }
    for ox in -2i32..=2 {
        for oy in -2i32..=2 {
            for oz in -2i32..=2 {
                let (ax, ay, az) = (ox.abs(), oy.abs(), oz.abs());
                if (ax > 1 || ay > 1 || az > 1)
                    && ((ox == 0 && (ay == 2 || az == 2))
                        || (oy == 0 && (ax == 2 || az == 2))
                        || (oz == 0 && (ax == 2 || ay == 2)))
                {
                    let test_pos = world_position.offset(ox, oy, oz);
                    if level
                        .get_block_state(test_pos)
                        .is_tag("conduit_effect_block")
                    {
                        effect_blocks.push(test_pos);
                    }
                }
            }
        }
    }
    effect_blocks.len() >= MIN_ACTIVE_SIZE
}

/// `applyEffects(level, worldPosition, effectBlocks)`.
fn apply_effects(level: &mut ServerLevel, world_position: BlockPos, active_size: usize) {
    let effect_range = (active_size / 7 * 16) as i32;
    let (x, y, z) = (
        f64::from(world_position.x()),
        f64::from(world_position.y()),
        f64::from(world_position.z()),
    );
    let bb = AABB::new(x, y, z, x + 1.0, y + 1.0, z + 1.0)
        .inflate_scalar(f64::from(effect_range))
        .expand_towards(0.0, f64::from(level.get_height()), 0.0);
    // `getEntitiesOfClass(Player.class, bb)`: `EntitySelector.NO_SPECTATORS`
    let players = level.get_entities_of_class(&bb, |e| e.is_player(), |e| !e.is_spectator());
    for id in players {
        let in_range = level.get_entity(id).is_some_and(|player| {
            world_position.closer_than(&player.block_position(), f64::from(effect_range))
                && player.is_in_water_or_rain(level)
        });
        if in_range {
            level.with_entity(id, |level, player| {
                player.add_effect(
                    level,
                    MobEffectInstance::new(
                        MobEffect::CONDUIT_POWER,
                        260,
                        0,
                        true,
                        true,
                        true,
                        None,
                    ),
                    None,
                );
            });
        }
    }
}

/// `EntityReference.getLivingEntity(reference, level)`.
fn get_living_entity(level: &ServerLevel, target: Option<Uuid>) -> Option<EntityId> {
    let entity = level.get_entity_by_uuid(target?)?;
    entity.living().is_some().then(|| entity.id())
}

/// `selectNewTarget(level, pos)`.
fn select_new_target(level: &mut ServerLevel, pos: BlockPos) -> Option<Uuid> {
    let bb = AABB::from_block_pos(&pos).inflate_scalar(KILL_RANGE);
    let candidates = level.get_entities_of_class(
        &bb,
        |e| e.living().is_some(),
        |e| e.is_enemy() && e.is_in_water_or_rain(level),
    );
    if candidates.is_empty() {
        None
    } else {
        // `Util.getRandom(candidates, random)`
        let index = level.random().next_int_bounded(candidates.len() as i32) as usize;
        level.get_entity(candidates[index]).map(|e| e.base().uuid)
    }
}

/// `updateDestroyTarget(target, level, pos, isActive)`.
fn update_destroy_target(
    target: Option<Uuid>,
    level: &mut ServerLevel,
    pos: BlockPos,
    is_active: bool,
) -> Option<Uuid> {
    if !is_active {
        None
    } else if target.is_none() {
        select_new_target(level, pos)
    } else {
        let alive_in_range = get_living_entity(level, target)
            .and_then(|id| level.get_entity(id))
            .is_some_and(|e| e.is_alive() && pos.closer_than(&e.block_position(), KILL_RANGE));
        if alive_in_range { target } else { None }
    }
}

impl BlockEntity for ConduitBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.destroy_target = read_codec(tag, "Target", &uuid_codec());
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if let Some(uuid) = &self.destroy_target
            && let Ok(encoded) = mc_dfu::encode_nbt(&uuid_codec(), uuid).into_result()
        {
            tag.put("Target", encoded);
        }
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket {
            pos: self.get_block_pos(),
            r#type: self.get_type(),
            tag: self.get_update_tag(),
        })
    }

    /// `getUpdateTag`: `saveCustomOnly`.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn is_ticking(&self) -> bool {
        true
    }

    /// `ConduitBlockEntity.serverTick`.
    fn server_tick(&mut self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        self.tick_count = self.tick_count.wrapping_add(1);
        let game_time = level.game_time();
        if game_time % 40 == 0 {
            let active = update_shape(level, pos, &mut self.effect_blocks);
            if active != self.is_active {
                let event = if active {
                    SoundEvent::BLOCK_CONDUIT_ACTIVATE
                } else {
                    SoundEvent::BLOCK_CONDUIT_DEACTIVATE
                };
                level.play_sound(None, pos, event, SoundSource::Blocks, 1.0, 1.0);
            }
            self.is_active = active;
            self.is_hunting = self.effect_blocks.len() >= MIN_KILL_SIZE;
            if active {
                apply_effects(level, pos, self.effect_blocks.len());
                let is_hunting = self.effect_blocks.len() >= MIN_KILL_SIZE;
                let new_destroy_target =
                    update_destroy_target(self.destroy_target, level, pos, is_hunting);
                if let Some(target) = get_living_entity(level, new_destroy_target) {
                    let (tx, ty, tz) = {
                        let target = level
                            .get_entity(target)
                            .expect("the target was just resolved");
                        {
                            let p = target.position();
                            (p.x, p.y, p.z)
                        }
                    };
                    level.play_sound_at(
                        None,
                        tx,
                        ty,
                        tz,
                        SoundEvent::BLOCK_CONDUIT_ATTACK_TARGET,
                        SoundSource::Blocks,
                        1.0,
                        1.0,
                    );
                    let source = level.damage_sources().magic();
                    level.with_entity(target, |level, entity| {
                        entity.hurt_server(level, &source, 4.0);
                    });
                }
                if new_destroy_target != self.destroy_target {
                    self.destroy_target = new_destroy_target;
                    let state = self.get_block_state();
                    level.send_block_updated(pos, state, state, 2);
                }
            }
        }
        if self.is_active {
            if game_time % 80 == 0 {
                level.play_sound(
                    None,
                    pos,
                    SoundEvent::BLOCK_CONDUIT_AMBIENT,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
            }
            if game_time > self.next_ambient_sound_activation {
                self.next_ambient_sound_activation =
                    game_time + 60 + i64::from(level.random().next_int_bounded(40));
                level.play_sound(
                    None,
                    pos,
                    SoundEvent::BLOCK_CONDUIT_AMBIENT_SHORT,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
            }
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static CONDUIT: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::CONDUIT,
    create: |pos, state| Box::new(ConduitBlockEntity::new(pos, state)),
};
