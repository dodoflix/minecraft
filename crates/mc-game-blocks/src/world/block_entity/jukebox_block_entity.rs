//! `JukeboxBlockEntity`: one record (a one-slot `Container`, vanilla `ContainerSingleItem`), the item-side
//! [`JukeboxSongPlayer`], ticking while `has_record`.

use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, GameEvent, props};
use mc_game_core::block_entity_base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, changed_log,
    deferred,
};
use mc_game_core::world::container::{Container, empty_stack, still_valid_block_entity};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::inventory::MenuPlayer;
use mc_game_core::world::item::{
    ItemStack, JukeboxSongPlayer, jukebox_song_from_stack, jukebox_song_value, with_item_registries,
};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_nbt::CompoundTag;
use mc_protocol::components::{Holder, JUKEBOX_PLAYABLE, JukeboxSong};
use std::any::Any;

/// `JukeboxSong.fromStack(stack)` resolved against the level's `jukebox_song` registry.
fn resolve_song(level: &ServerLevel, stack: &ItemStack) -> Option<Holder<JukeboxSong>> {
    jukebox_song_from_stack(stack)
        .filter(|song| jukebox_song_value(&level.registries, song).is_some())
}

pub struct JukeboxBlockEntity {
    base: BlockEntityData,
    item: ItemStack,
    song_player: JukeboxSongPlayer,
    /// The song and `ticks_since_song_started` that were loaded: the registry is not reachable while loading, so the song is
    /// set (`setSongWithoutPlaying`) on the first access with a level (and written back as is meanwhile).
    pending_song: Option<(Holder<JukeboxSong>, i64)>,
    /// The stacks the `Container` view stored since the last [`BlockEntity::container_accessed`]: vanilla's `setTheItem`
    /// effects need the level, which `Container::set_item` has not.
    pending_items: Vec<ItemStack>,
}

impl JukeboxBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::JUKEBOX, pos, state),
            item: ItemStack::empty(),
            song_player: JukeboxSongPlayer::new(pos),
            pending_song: None,
            pending_items: Vec::new(),
        }
    }

    pub fn is_playing(&self) -> bool {
        self.song_player.is_playing() || self.pending_song.is_some()
    }

    /// Sets the loaded song once a level is at hand (vanilla does it in `loadAdditional`).
    fn resolve_pending_song(&mut self, level: &ServerLevel) {
        if let Some((song, ticks)) = self.pending_song.take() {
            self.song_player
                .set_song_without_playing(&level.registries, song, ticks);
        }
    }

    pub fn get_the_item(&self) -> &ItemStack {
        &self.item
    }

    /// `getComparatorOutput()`.
    pub fn comparator_output(&self, level: &ServerLevel) -> i32 {
        resolve_song(level, &self.item)
            .and_then(|song| jukebox_song_value(&level.registries, &song))
            .map_or(0, |song| song.comparator_output)
    }

    /// `onSongChanged()`: the neighbours read this block entity's signal, so it runs once the entity is back in its chunk.
    fn on_song_changed(&self, level: &mut ServerLevel) {
        let (pos, state) = (self.base.pos, self.base.block_state);
        deferred::run_or_push(level, move |level| {
            level.update_neighbors_at(pos, state.block());
            mc_game_core::world::block_entity::set_changed(level, pos, state);
        });
    }

    /// `notifyItemChangedInJukebox(wasInserted)`.
    fn notify_item_changed(&mut self, level: &mut ServerLevel, was_inserted: bool) {
        let pos = self.base.pos;
        if mc_game_core::world::level::BlockGetter::get_block_state(level, pos)
            == self.base.block_state
        {
            let new_state = self.base.block_state.set(props::HAS_RECORD, was_inserted);
            level.set_block(pos, new_state, 2, 512);
            self.base.block_state = new_state;
            level.game_event_at_block(
                GameEvent::BLOCK_CHANGE,
                pos,
                &GameEventContext::of_state(Some(new_state)),
            );
        }
    }

    /// `setTheItem(itemStack)`.
    pub fn set_the_item(&mut self, level: &mut ServerLevel, stack: ItemStack) {
        self.resolve_pending_song(level);
        self.item = stack;
        self.item_changed(level);
    }

    /// What `setTheItem` does once `item` is stored: block state, game event and song start/stop.
    fn item_changed(&mut self, level: &mut ServerLevel) {
        let inserted = !self.item.is_empty();
        let maybe_song = resolve_song(level, &self.item);
        self.notify_item_changed(level, inserted);
        let state = self.base.block_state;
        let song_changed = match maybe_song {
            Some(song) if inserted => self.song_player.play(level, song),
            _ => self.song_player.stop(level, Some(state)),
        };
        if song_changed {
            self.on_song_changed(level);
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
        let item_pos = Vec3::at_lower_corner_with_offset(&pos, 0.5, 1.01, 0.5)
            .offset_random_xz(LevelAccessor::random(level), 0.7);
        let mut entity =
            ItemEntity::with_stack(level, item_pos.x, item_pos.y, item_pos.z, before.copy());
        entity.set_default_pick_up_delay();
        level.add_fresh_entity(Box::new(entity));
        self.on_song_changed(level);
    }
}

impl BlockEntity for JukeboxBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        let new_item =
            mc_game_core::world::block_entity::read_codec(tag, "RecordItem", &ItemStack::codec())
                .unwrap_or_else(ItemStack::empty);
        if !self.item.is_empty() && !ItemStack::is_same_item_same_components(&new_item, &self.item)
        {
            // vanilla stops the player with the level; there is none here, so the song just ends
            self.song_player = JukeboxSongPlayer::new(self.base.pos);
        }
        self.item = new_item;
        self.pending_song = tag
            .get_long("ticks_since_song_started")
            .zip(jukebox_song_from_stack(&self.item))
            .map(|(ticks, song)| (song, ticks));
        // vanilla's holder resolves right here; with the server's registries installed so does ours
        if let Some((song, ticks)) = self.pending_song.take()
            && with_item_registries(|registries| {
                self.song_player
                    .set_song_without_playing(registries, song.clone(), ticks);
            })
            .is_none()
        {
            self.pending_song = Some((song, ticks));
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if !self.item.is_empty()
            && let Ok(t) = self.item.to_tag().into_result()
        {
            tag.put("RecordItem", t);
        }
        if self.song_player.is_playing() {
            tag.put_long(
                "ticks_since_song_started",
                self.song_player.get_ticks_since_song_started(),
            );
        } else if let Some((_, ticks)) = &self.pending_song {
            tag.put_long("ticks_since_song_started", *ticks);
        }
    }

    fn is_ticking(&self) -> bool {
        self.base.block_state.get(props::HAS_RECORD)
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        self.resolve_pending_song(level);
        let state = self.base.block_state;
        if self.song_player.tick(level, Some(state)) {
            self.on_song_changed(level);
        }
    }

    fn pre_remove_side_effects(
        &mut self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _state: BlockState,
    ) {
        self.pop_out_the_item(level);
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self)
    }

    fn container_pending(&self) -> bool {
        !self.pending_items.is_empty()
    }

    /// The `setTheItem` effects of every `Container::set_item` / `remove_item` since the last call, in order.
    fn container_accessed(&mut self, level: &mut ServerLevel) {
        self.resolve_pending_song(level);
        let latest = std::mem::take(&mut self.item);
        for stack in std::mem::take(&mut self.pending_items) {
            self.item = stack;
            self.item_changed(level);
        }
        self.item = latest;
    }

    fn set_removed_in_level(&mut self, level: &mut ServerLevel) {
        self.set_removed();
        level.game_event_at_block(
            GameEvent::JUKEBOX_STOP_PLAY,
            self.base.pos,
            &GameEventContext::of_state(Some(self.base.block_state)),
        );
        level.level_event(None, 1011, self.base.pos, 0);
    }
}

/// `ContainerSingleItem.BlockContainerSingleItem`: the jukebox is a one-slot `Container` for hoppers, droppers,
/// `/item replace` and `/loot insert`. Vanilla's `setTheItem` effects run in [`BlockEntity::container_accessed`].
impl Container for JukeboxBlockEntity {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        1
    }
    fn is_empty(&self) -> bool {
        self.item.is_empty()
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        if slot == 0 { &self.item } else { empty_stack() }
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        (slot == 0).then_some(&mut self.item)
    }
    /// `splitTheItem(count)` (the whole item, whatever the count) then `setTheItem(EMPTY)`.
    fn remove_item(&mut self, slot: usize, _count: i32) -> ItemStack {
        if slot != 0 {
            return ItemStack::empty();
        }
        self.pending_items.push(ItemStack::empty());
        std::mem::take(&mut self.item)
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.remove_item(slot, 1)
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        if slot == 0 {
            self.item = stack.clone();
            self.pending_items.push(stack);
        }
    }
    fn clear_content(&mut self) {
        self.remove_item(0, 1);
    }
    fn set_changed(&mut self) {
        changed_log::push(self.base.pos);
    }
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        still_valid_block_entity(self.base.pos, player)
    }
    fn get_max_stack_size(&self) -> i32 {
        1
    }
    fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        stack.get(&JUKEBOX_PLAYABLE).is_some() && self.get_item(slot).is_empty()
    }
    fn can_take_item(&self, into: &dyn Container, _slot: usize, _stack: &ItemStack) -> bool {
        into.has_any_matching(&ItemStack::is_empty)
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static JUKEBOX: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::JUKEBOX,
    create: |pos, state| Box::new(JukeboxBlockEntity::new(pos, state)),
};
