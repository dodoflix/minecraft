//! `JukeboxBlockEntity` + `JukeboxSongPlayer` (`world.item`): one record, song playing state, ticking while `has_record`.

use mc_game_core::block_entity_base;
use mc_game_core::world::block_entity::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::gameevent::GameEventContext;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, GameEvent, ParticleType, props};
use mc_nbt::CompoundTag;
use mc_protocol::components::{Holder, JUKEBOX_PLAYABLE, JukeboxSong};
use mc_protocol::packets::play::ParticleOptions;

/// Resolved `JukeboxSong` values of the data pack entry (`length_in_seconds`, `comparator_output`, registry id).
#[derive(Clone, Copy, Debug)]
pub struct SongInfo {
    pub id: i32,
    pub length_in_ticks: i32,
    pub comparator_output: i32,
}

impl SongInfo {
    /// `hasFinished(ticksElapsed)`.
    pub fn has_finished(&self, ticks_elapsed: i64) -> bool {
        ticks_elapsed >= i64::from(self.length_in_ticks) + 20
    }
}

/// `JukeboxSong.fromStack(stack)` resolved through the level's `jukebox_song` registry.
pub fn song_of_stack(level: &ServerLevel, stack: &ItemStack) -> Option<(Holder<JukeboxSong>, SongInfo)> {
    let holder = stack.get(&JUKEBOX_PLAYABLE)?.song.clone();
    let info = song_info(level, &holder)?;
    Some((holder, info))
}

pub fn song_info(level: &ServerLevel, holder: &Holder<JukeboxSong>) -> Option<SongInfo> {
    match holder {
        Holder::Reference(name) => {
            let registry = level.registries.registry("jukebox_song")?;
            let entry = registry.entry_by_key(name)?;
            let seconds = entry.json.get("length_in_seconds")?.as_f64()? as f32;
            let comparator_output = entry.json.get("comparator_output")?.as_i64()? as i32;
            Some(SongInfo { id: registry.id_of(name)? as i32, length_in_ticks: mc_core::math::mth::ceil_f32(seconds * 20.0), comparator_output })
        }
        Holder::Direct(song) => Some(SongInfo { id: -1, length_in_ticks: mc_core::math::mth::ceil_f32(song.length_in_seconds * 20.0), comparator_output: song.comparator_output }),
    }
}

/// `JukeboxSongPlayer`.
#[derive(Clone, Debug, Default)]
pub struct JukeboxSongPlayer {
    ticks_since_song_started: i64,
    song: Option<(Holder<JukeboxSong>, SongInfo)>,
}

impl JukeboxSongPlayer {
    pub fn is_playing(&self) -> bool {
        self.song.is_some()
    }

    pub fn get_ticks_since_song_started(&self) -> i64 {
        self.ticks_since_song_started
    }

    /// `setSongWithoutPlaying(song, ticksSinceSongStarted)`.
    pub fn set_song_without_playing(&mut self, song: (Holder<JukeboxSong>, SongInfo), ticks_since_song_started: i64) {
        if !song.1.has_finished(ticks_since_song_started) {
            self.song = Some(song);
            self.ticks_since_song_started = ticks_since_song_started;
        }
    }
}

pub struct JukeboxBlockEntity {
    base: BlockEntityData,
    item: ItemStack,
    song_player: JukeboxSongPlayer,
    /// `ticks_since_song_started` was loaded; the song is resolved against the registry on the first tick.
    pending_song: bool,
}

impl JukeboxBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::JUKEBOX, pos, state), item: ItemStack::empty(), song_player: JukeboxSongPlayer::default(), pending_song: false }
    }

    pub fn is_playing(&self) -> bool {
        self.song_player.is_playing()
    }

    pub fn get_the_item(&self) -> &ItemStack {
        &self.item
    }

    /// `getComparatorOutput()`.
    pub fn comparator_output(&self, level: &ServerLevel) -> i32 {
        song_of_stack(level, &self.item).map_or(0, |(_, info)| info.comparator_output)
    }

    /// `onSongChanged()`.
    fn on_song_changed(&self, level: &mut ServerLevel) {
        level.update_neighbors_at(self.base.pos, self.base.block_state.block());
        mc_game_core::world::block_entity::set_changed(level, self.base.pos, self.base.block_state);
    }

    /// `JukeboxSongPlayer.play(level, song)`.
    fn play(&mut self, level: &mut ServerLevel, song: (Holder<JukeboxSong>, SongInfo)) {
        let id = song.1.id;
        self.song_player.song = Some(song);
        self.song_player.ticks_since_song_started = 0;
        level.level_event(None, 1010, self.base.pos, id);
        self.on_song_changed(level);
    }

    /// `JukeboxSongPlayer.stop(level, blockState)`.
    fn stop(&mut self, level: &mut ServerLevel, state: BlockState) {
        if self.song_player.song.is_some() {
            self.song_player.song = None;
            self.song_player.ticks_since_song_started = 0;
            level.game_event_at_block(GameEvent::JUKEBOX_STOP_PLAY, self.base.pos, &GameEventContext::of_state(Some(state)));
            level.level_event(None, 1011, self.base.pos, 0);
            self.on_song_changed(level);
        }
    }

    /// `notifyItemChangedInJukebox(wasInserted)`.
    fn notify_item_changed(&mut self, level: &mut ServerLevel, was_inserted: bool) {
        let pos = self.base.pos;
        if mc_game_core::world::level::BlockGetter::get_block_state(level, pos) == self.base.block_state {
            let new_state = self.base.block_state.set(props::HAS_RECORD, was_inserted);
            level.set_block(pos, new_state, 2, 512);
            self.base.block_state = new_state;
            level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::of_state(Some(new_state)));
        }
    }

    /// `setTheItem(itemStack)`.
    pub fn set_the_item(&mut self, level: &mut ServerLevel, stack: ItemStack) {
        self.item = stack;
        let inserted = !self.item.is_empty();
        let maybe_song = song_of_stack(level, &self.item);
        self.notify_item_changed(level, inserted);
        match maybe_song {
            Some(song) if inserted => self.play(level, song),
            _ => {
                let state = self.base.block_state;
                self.stop(level, state);
            }
        }
    }

    /// `popOutTheItem()`.
    pub fn pop_out_the_item(&mut self, level: &mut ServerLevel) {
        let pos = self.base.pos;
        if self.item.is_empty() {
            return;
        }
        let before = std::mem::take(&mut self.item);
        self.set_the_item(level, ItemStack::empty());
        let item_pos = Vec3::at_lower_corner_with_offset(&pos, 0.5, 1.01, 0.5).offset_random_xz(LevelAccessor::random(level), 0.7);
        let mut entity = ItemEntity::with_stack(level, item_pos.x, item_pos.y, item_pos.z, before.copy());
        entity.set_default_pick_up_delay();
        level.add_fresh_entity(Box::new(entity));
        self.on_song_changed(level);
    }
}

impl BlockEntity for JukeboxBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        let new_item = tag.get("RecordItem").and_then(|t| ItemStack::from_tag(t).into_result().ok()).unwrap_or_else(ItemStack::empty);
        if !self.item.is_empty() && !ItemStack::is_same_item_same_components(&new_item, &self.item) {
            self.song_player.song = None;
            self.song_player.ticks_since_song_started = 0;
        }
        self.item = new_item;
        if let Some(ticks) = tag.get_long("ticks_since_song_started") {
            // resolved lazily: the registry is not reachable here, the song is re-resolved by `server_tick`
            self.song_player.ticks_since_song_started = ticks;
            self.pending_song = true;
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if !self.item.is_empty()
            && let Ok(t) = self.item.to_tag().into_result()
        {
            tag.put("RecordItem", t);
        }
        if self.song_player.song.is_some() {
            tag.put_long("ticks_since_song_started", self.song_player.ticks_since_song_started);
        }
    }

    fn is_ticking(&self) -> bool {
        self.base.block_state.get(props::HAS_RECORD)
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        let state = self.base.block_state;
        let pos = self.base.pos;
        if self.pending_song {
            self.pending_song = false;
            if let Some(song) = song_of_stack(level, &self.item) {
                let ticks = self.song_player.ticks_since_song_started;
                self.song_player.ticks_since_song_started = 0;
                self.song_player.set_song_without_playing(song, ticks);
            }
        }
        let Some((_, info)) = self.song_player.song.clone() else { return };
        if info.has_finished(self.song_player.ticks_since_song_started) {
            self.stop(level, state);
        } else {
            if self.song_player.ticks_since_song_started % 20 == 0 {
                level.game_event_at_block(GameEvent::JUKEBOX_PLAY, pos, &GameEventContext::of_state(Some(state)));
                let p = Vec3::at_bottom_center_of(&pos).add_xyz(0.0, f64::from(1.2f32), 0.0);
                let random_color = f64::from(LevelAccessor::random(level).next_int_bounded(4) as f32 / 24.0);
                level.send_particles(ParticleOptions::Simple(ParticleType::NOTE), p.x, p.y, p.z, 0, random_color, 0.0, 0.0, 1.0);
            }
            self.song_player.ticks_since_song_started += 1;
        }
    }

    fn pre_remove_side_effects(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: BlockState) {
        self.pop_out_the_item(level);
    }

    fn set_removed_in_level(&mut self, level: &mut ServerLevel) {
        self.set_removed();
        level.game_event_at_block(GameEvent::JUKEBOX_STOP_PLAY, self.base.pos, &GameEventContext::of_state(Some(self.base.block_state)));
        level.level_event(None, 1011, self.base.pos, 0);
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static JUKEBOX: BlockEntityRegistration = BlockEntityRegistration { ty: BlockEntityType::JUKEBOX, create: |pos, state| Box::new(JukeboxBlockEntity::new(pos, state)) };
