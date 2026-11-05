//! `BeaconBlockEntity`: beam section scan (10 blocks per tick), pyramid level scan (`updateBase` every 80 ticks), the
//! primary / secondary effects and their `ContainerData` view for `BeaconMenu`.

use mc_game_core::block_entity_base;
use mc_game_core::world::block_entity::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::inventory::{BEACON_EFFECTS, ContainerData, decode_effect, encode_effect};
use mc_game_core::world::level::block::BlockStateExt;
use mc_core::phys::AABB;
use mc_game_core::world::level::{BlockGetter, LevelReader};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::{BlockEntityType, MobEffect, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{LockCode, component_codec};
use mc_protocol::packets::play::SoundSource;
use mc_text::Component;
use mc_world::heightmap::HeightmapType;

pub const MAX_LEVELS: i32 = 4;
pub const DATA_LEVELS: usize = 0;
pub const DATA_PRIMARY: usize = 1;
pub const DATA_SECONDARY: usize = 2;
pub const NUM_DATA_VALUES: usize = 3;
const BLOCKS_CHECK_PER_TICK: i32 = 10;

/// `BeaconBeamOwner.Section`: one beam segment (`color` is ARGB, `height` in blocks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeamSection {
    pub color: i32,
    pub height: i32,
}

impl BeamSection {
    fn new(color: i32) -> Self {
        Self { color, height: 1 }
    }
}

/// `ARGB.average(lhs, rhs)`: per-channel integer mean.
pub fn argb_average(lhs: i32, rhs: i32) -> i32 {
    let channel = |c: i32, shift: u32| (c >> shift) & 0xFF;
    let mix = |shift: u32| ((channel(lhs, shift) + channel(rhs, shift)) / 2) << shift;
    mix(24) | mix(16) | mix(8) | mix(0)
}

/// `BeaconBeamBlock.getColor().getTextureDiffuseColor()` of a block (`BeaconBlock` = white, the stained glass blocks and
/// panes their dye color); `None` when the block is not a `BeaconBeamBlock`.
pub fn beacon_beam_color(block: Block) -> Option<i32> {
    let data = block.data();
    let dye = if data.classes.contains(&"BeaconBlock") {
        mc_protocol::components::DyeColor::White
    } else if data.classes.contains(&"StainedGlassBlock") || data.classes.contains(&"StainedGlassPaneBlock") {
        let name = data.name.strip_prefix("minecraft:").unwrap_or(data.name);
        let color = name.strip_suffix("_stained_glass_pane").or_else(|| name.strip_suffix("_stained_glass"))?;
        mc_protocol::components::DyeColor::from_name(color)?
    } else {
        return None;
    };
    Some(mc_data::misc::DYE_COLORS[dye.id() as usize].texture_diffuse_color)
}

/// `filterEffect`: only the effects of [`BEACON_EFFECTS`] are kept.
fn filter_effect(effect: Option<MobEffect>) -> Option<MobEffect> {
    effect.filter(|e| BEACON_EFFECTS.iter().any(|level| level.contains(e)))
}

/// `BeaconBlockEntity.playSound(level, pos, event)`.
pub fn play_sound(level: &mut ServerLevel, pos: BlockPos, event: SoundEvent) {
    level.play_sound(None, pos, event, SoundSource::Blocks, 1.0, 1.0);
}

/// `updateBase(level, x, y, z)`: how many pyramid layers (max 4) of beacon base blocks are below.
fn update_base(level: &dyn BlockGetter, x: i32, y: i32, z: i32) -> i32 {
    let mut levels = 0;
    let mut step = 1;
    while step <= MAX_LEVELS {
        let ly = y - step;
        if ly < level.get_min_y() {
            break;
        }
        let mut is_ok = true;
        let mut lx = x - step;
        while lx <= x + step && is_ok {
            for lz in z - step..=z + step {
                if !level.get_block_state(BlockPos::new(lx, ly, lz)).is_tag("beacon_base_blocks") {
                    is_ok = false;
                    break;
                }
            }
            lx += 1;
        }
        if !is_ok {
            break;
        }
        levels = step;
        step += 1;
    }
    levels
}

pub struct BeaconBlockEntity {
    base: BlockEntityData,
    beam_sections: Vec<BeamSection>,
    checking_beam_sections: Vec<BeamSection>,
    pub levels: i32,
    /// `lastCheckY`; `i32::MIN` until the first tick (vanilla sets `minY - 1` in `setLevel`, any value below the block's
    /// y restarts the scan).
    last_check_y: i32,
    pub primary_power: Option<MobEffect>,
    pub secondary_power: Option<MobEffect>,
    pub name: Option<Component>,
    /// `None` = `LockCode.NO_LOCK`.
    pub lock_key: Option<LockCode>,
    /// `dataAccess.set(1, ..)` happened while beams exist: the menu resolver plays `BEACON_POWER_SELECT` (the setter has
    /// no level) through [`BeaconBlockEntity::take_power_select_sound`].
    power_select_pending: bool,
}

impl BeaconBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::BEACON, pos, state),
            beam_sections: Vec::new(),
            checking_beam_sections: Vec::new(),
            levels: 0,
            last_check_y: i32::MIN,
            primary_power: None,
            secondary_power: None,
            name: None,
            lock_key: None,
            power_select_pending: false,
        }
    }

    /// `getBeamSections()`.
    pub fn get_beam_sections(&self) -> &[BeamSection] {
        if self.levels == 0 { &[] } else { &self.beam_sections }
    }

    /// `getName()`.
    pub fn get_name(&self) -> Component {
        self.name.clone().unwrap_or_else(|| Component::translatable("container.beacon", vec![]))
    }

    /// `dataAccess.get(dataId)`.
    pub fn data_get(&self, data_id: usize) -> i32 {
        match data_id {
            DATA_LEVELS => self.levels,
            DATA_PRIMARY => encode_effect(self.primary_power),
            DATA_SECONDARY => encode_effect(self.secondary_power),
            _ => 0,
        }
    }

    /// `dataAccess.set(dataId, value)` (the `BEACON_POWER_SELECT` sound is deferred, see `power_select_pending`).
    pub fn data_set(&mut self, data_id: usize, value: i32) {
        match data_id {
            DATA_LEVELS => self.levels = value,
            DATA_PRIMARY => {
                if !self.beam_sections.is_empty() {
                    self.power_select_pending = true;
                }
                self.primary_power = filter_effect(decode_effect(value));
            }
            DATA_SECONDARY => self.secondary_power = filter_effect(decode_effect(value)),
            _ => {}
        }
    }

    /// `applyEffects(level, pos, levels, primary, secondary)`.
    fn apply_effects(&self, level: &mut ServerLevel) {
        let Some(primary_power) = self.primary_power else { return };
        let levels = self.levels;
        let range = f64::from(levels * 10 + 10);
        let mut base_amp = 0;
        if levels >= 4 && Some(primary_power) == self.secondary_power {
            base_amp = 1;
        }
        let duration_ticks = (9 + levels * 2) * 20;
        let bb = AABB::from_block_pos(&self.get_block_pos()).inflate_scalar(range).expand_towards(0.0, f64::from(level.get_height()), 0.0);
        let players = level.get_entities_of_class(&bb, |e| e.is_player(), |_| true);
        for &id in &players {
            level.with_entity(id, |level, player| {
                player.add_effect(level, MobEffectInstance::with_flags(primary_power, duration_ticks, base_amp, true, true), None);
            });
        }
        if levels >= 4
            && Some(primary_power) != self.secondary_power
            && let Some(secondary) = self.secondary_power
        {
            for &id in &players {
                level.with_entity(id, |level, player| {
                    player.add_effect(level, MobEffectInstance::with_flags(secondary, duration_ticks, 0, true, true), None);
                });
            }
        }
    }

    /// Whether a `BEACON_POWER_SELECT` sound is owed for a primary-effect change; clears the flag.
    pub fn take_power_select_sound(&mut self) -> bool {
        std::mem::take(&mut self.power_select_pending)
    }
}

impl BlockEntity for BeaconBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.primary_power = load_effect(tag, "primary_effect");
        self.secondary_power = load_effect(tag, "secondary_effect");
        self.name = read_codec(tag, "CustomName", &component_codec());
        self.lock_key = read_codec(tag, "lock", &LockCode::codec());
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        store_effect(tag, "primary_effect", self.primary_power);
        store_effect(tag, "secondary_effect", self.secondary_power);
        tag.put_int("Levels", self.levels);
        if let Some(name) = &self.name
            && let Ok(encoded) = mc_dfu::encode_nbt(&component_codec(), name).into_result()
        {
            tag.put("CustomName", encoded);
        }
        if let Some(lock) = &self.lock_key
            && let Ok(encoded) = mc_dfu::encode_nbt(&LockCode::codec(), lock).into_result()
        {
            tag.put("lock", encoded);
        }
    }

    fn container_data(&self) -> Option<&dyn ContainerData> {
        Some(self)
    }

    fn container_data_mut(&mut self) -> Option<&mut dyn ContainerData> {
        Some(self)
    }

    fn container_pending(&self) -> bool {
        self.power_select_pending
    }

    /// The `BEACON_POWER_SELECT` sound of `dataAccess.set(1, ..)` (the setter has no level).
    fn container_accessed(&mut self, level: &mut ServerLevel) {
        if self.take_power_select_sound() {
            play_sound(level, self.get_block_pos(), SoundEvent::BLOCK_BEACON_POWER_SELECT);
        }
    }

    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn get_update_packet(&self) -> Option<mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket> {
        Some(mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket { pos: self.get_block_pos(), r#type: self.get_type(), tag: self.get_update_tag() })
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn set_removed_in_level(&mut self, level: &mut ServerLevel) {
        play_sound(level, self.get_block_pos(), SoundEvent::BLOCK_BEACON_DEACTIVATE);
        self.set_removed();
    }

    /// `BeaconBlockEntity.tick` (server side).
    fn server_tick(&mut self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        let (x, y, z) = (pos.x(), pos.y(), pos.z());
        let mut check_pos;
        if self.last_check_y < y {
            check_pos = pos;
            self.checking_beam_sections = Vec::new();
            self.last_check_y = pos.y() - 1;
        } else {
            check_pos = BlockPos::new(x, self.last_check_y + 1, z);
        }
        // `lastBeamSection`: index into `checking_beam_sections`
        let mut last_beam_section = self.checking_beam_sections.len().checked_sub(1);
        let last_set_block = level.get_height_at(HeightmapType::WorldSurface, x, z);
        let mut i = 0;
        while i < BLOCKS_CHECK_PER_TICK && check_pos.y() <= last_set_block {
            let state = level.get_block_state(check_pos);
            if let Some(color) = beacon_beam_color(state.block()) {
                if self.checking_beam_sections.len() <= 1 {
                    self.checking_beam_sections.push(BeamSection::new(color));
                    last_beam_section = Some(self.checking_beam_sections.len() - 1);
                } else if let Some(last) = last_beam_section {
                    if color == self.checking_beam_sections[last].color {
                        self.checking_beam_sections[last].height += 1;
                    } else {
                        let mixed = argb_average(self.checking_beam_sections[last].color, color);
                        self.checking_beam_sections.push(BeamSection::new(mixed));
                        last_beam_section = Some(self.checking_beam_sections.len() - 1);
                    }
                }
            } else {
                match last_beam_section {
                    Some(last) if !(state.get_light_block() >= 15 && !state.is_block(Block::BEDROCK)) => self.checking_beam_sections[last].height += 1,
                    _ => {
                        self.checking_beam_sections.clear();
                        self.last_check_y = last_set_block;
                        break;
                    }
                }
            }
            check_pos = check_pos.above();
            self.last_check_y += 1;
            i += 1;
        }

        let previous_levels = self.levels;
        if level.get_game_time() % 80 == 0 {
            if !self.beam_sections.is_empty() {
                self.levels = update_base(&*level, x, y, z);
            }
            if self.levels > 0 && !self.beam_sections.is_empty() {
                self.apply_effects(level);
                play_sound(level, pos, SoundEvent::BLOCK_BEACON_AMBIENT);
            }
        }

        if self.last_check_y >= last_set_block {
            self.last_check_y = level.get_min_y() - 1;
            let was_active = previous_levels > 0;
            self.beam_sections = std::mem::take(&mut self.checking_beam_sections);
            let is_active = self.levels > 0;
            if !was_active && is_active {
                play_sound(level, pos, SoundEvent::BLOCK_BEACON_ACTIVATE);
                // CriteriaTriggers.CONSTRUCT_BEACON.trigger(player, levels) for the ServerPlayers in
                // `new AABB(x, y, z, x, y - 4, z).inflate(10.0, 5.0, 10.0)`: wired with the advancement triggers
            } else if was_active && !is_active {
                play_sound(level, pos, SoundEvent::BLOCK_BEACON_DEACTIVATE);
            }
        }
    }
}

/// `storeEffect(output, field, effect)`.
fn store_effect(tag: &mut CompoundTag, field: &str, effect: Option<MobEffect>) {
    if let Some(effect) = effect {
        tag.put_string(field, effect.name());
    }
}

/// `loadEffect(input, field)`: the registered effect, kept only when selectable.
fn load_effect(tag: &CompoundTag, field: &str) -> Option<MobEffect> {
    let name = tag.get_string(field)?;
    filter_effect(MobEffect::by_name(name))
}

impl ContainerData for BeaconBlockEntity {
    fn get(&self, data_id: usize) -> i32 {
        self.data_get(data_id)
    }
    fn set(&mut self, data_id: usize, value: i32) {
        self.data_set(data_id, value);
    }
    fn get_count(&self) -> usize {
        NUM_DATA_VALUES
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static BEACON: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::BEACON, create: |pos, state| Box::new(BeaconBlockEntity::new(pos, state)) };
