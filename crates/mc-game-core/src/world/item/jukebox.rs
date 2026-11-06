//! `JukeboxSong` (the `jukebox_song` registry), `JukeboxPlayable` lookups and `JukeboxSongPlayer`
//! (the state machine `JukeboxBlockEntity` drives).

use super::ItemStack;
use super::instrument::StageLookup;
use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::{RegistryAccess, keys};
use crate::server::level::ServerLevel;
use crate::world::level::LevelAccessor;
use crate::world::level::gameevent::GameEventContext;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{GameEvent, ParticleType};
use mc_dfu::JSON;
use mc_protocol::components::common::serde_to_dyn;
use mc_protocol::components::{Holder, JUKEBOX_PLAYABLE, JukeboxSong};
use mc_protocol::packets::play::ParticleOptions;
use mc_protocol::registry_context::with_scoped_lookup;
use serde_json::Value;

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let input = serde_to_dyn(json);
    let song = with_scoped_lookup(&StageLookup(ctx), || {
        JukeboxSong::direct_codec().parse(&JSON, &input)
    })
    .into_result()?;
    Ok(DecodedEntry::new(song))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::JUKEBOX_SONG,
    decode,
};

/// `JukeboxSong.fromStack(stack)`: the `jukebox_playable` song of the stack.
pub fn jukebox_song_from_stack(stack: &ItemStack) -> Option<Holder<JukeboxSong>> {
    stack.get(&JUKEBOX_PLAYABLE).map(|p| p.song.clone())
}

/// `holder.value()` of a song holder.
fn song_value(registries: &RegistryAccess, song: &Holder<JukeboxSong>) -> Option<JukeboxSong> {
    match song {
        Holder::Reference(id) => registries
            .get::<JukeboxSong>(keys::JUKEBOX_SONG, id)
            .cloned(),
        Holder::Direct(song) => Some(song.clone()),
    }
}

/// `JukeboxSong.lengthInTicks()`.
pub fn length_in_ticks(song: &JukeboxSong) -> i64 {
    i64::from(mth::ceil_f32(song.length_in_seconds * 20.0))
}

/// `JukeboxSong.hasFinished(ticksElapsed)` (`SONG_END_PADDING_TICKS` = 20).
pub fn has_finished(song: &JukeboxSong, ticks_elapsed: i64) -> bool {
    ticks_elapsed >= length_in_ticks(song) + 20
}

/// `JukeboxSongPlayer`. Where vanilla calls its `OnSongChanged` callback the mutating methods return `true`
/// (the owner then runs `notifyChange`: neighbor update + `setChanged`).
pub struct JukeboxSongPlayer {
    ticks_since_song_started: i64,
    song: Option<Holder<JukeboxSong>>,
    block_pos: BlockPos,
}

impl JukeboxSongPlayer {
    pub const PLAY_EVENT_INTERVAL_TICKS: i64 = 20;

    pub fn new(block_pos: BlockPos) -> Self {
        Self {
            ticks_since_song_started: 0,
            song: None,
            block_pos,
        }
    }

    pub fn is_playing(&self) -> bool {
        self.song.is_some()
    }

    /// `getSong()`.
    pub fn get_song(&self, registries: &RegistryAccess) -> Option<JukeboxSong> {
        self.song.as_ref().and_then(|s| song_value(registries, s))
    }

    pub fn get_ticks_since_song_started(&self) -> i64 {
        self.ticks_since_song_started
    }

    /// `setSongWithoutPlaying(song, ticksSinceSongStarted)`.
    pub fn set_song_without_playing(
        &mut self,
        registries: &RegistryAccess,
        song: Holder<JukeboxSong>,
        ticks_since_song_started: i64,
    ) {
        if song_value(registries, &song)
            .is_some_and(|v| !has_finished(&v, ticks_since_song_started))
        {
            self.song = Some(song);
            self.ticks_since_song_started = ticks_since_song_started;
        }
    }

    /// `play(level, song)`: level event 1010 with the registry id of the song. Returns `true` (song changed).
    pub fn play(&mut self, level: &mut ServerLevel, song: Holder<JukeboxSong>) -> bool {
        // `registry.getId(value)`: -1 for a song that is not registered (inline definition)
        let song_id = match &song {
            Holder::Reference(name) => level
                .registries
                .registry(keys::JUKEBOX_SONG)
                .and_then(|r| r.id_of(name))
                .map_or(-1, |id| id as i32),
            Holder::Direct(_) => -1,
        };
        self.song = Some(song);
        self.ticks_since_song_started = 0;
        level.level_event_impl(None, 1010, self.block_pos, song_id);
        true
    }

    /// `stop(level, blockState)`. Returns `true` when a song was stopped.
    pub fn stop(&mut self, level: &mut ServerLevel, block_state: Option<BlockState>) -> bool {
        if self.song.is_none() {
            return false;
        }
        self.song = None;
        self.ticks_since_song_started = 0;
        level.game_event_at_block(
            GameEvent::JUKEBOX_STOP_PLAY,
            self.block_pos,
            &GameEventContext::of_state(block_state),
        );
        level.level_event_impl(None, 1011, self.block_pos, 0);
        true
    }

    /// `tick(level, blockState)`. Returns `true` when the song ended (stopped).
    pub fn tick(&mut self, level: &mut ServerLevel, block_state: Option<BlockState>) -> bool {
        let Some(song) = self.song.as_ref() else {
            return false;
        };
        let finished = match song_value(&level.registries, song) {
            Some(value) => has_finished(&value, self.ticks_since_song_started),
            None => return false,
        };
        if finished {
            return self.stop(level, block_state);
        }
        if self.ticks_since_song_started % Self::PLAY_EVENT_INTERVAL_TICKS == 0 {
            level.game_event_at_block(
                GameEvent::JUKEBOX_PLAY,
                self.block_pos,
                &GameEventContext::of_state(block_state),
            );
            self.spawn_music_particles(level);
        }
        self.ticks_since_song_started += 1;
        false
    }

    /// `spawnMusicParticles(level, blockPos)`.
    fn spawn_music_particles(&self, level: &mut ServerLevel) {
        let pos = Vec3::at_bottom_center_of(&self.block_pos).add_xyz(0.0, f64::from(1.2f32), 0.0);
        let random_color = level.random().next_int_bounded(4) as f32 / 24.0f32;
        level.send_particles(
            ParticleOptions::Simple(ParticleType::NOTE),
            pos.x,
            pos.y,
            pos.z,
            0,
            f64::from(random_color),
            0.0,
            0.0,
            1.0,
        );
    }
}
