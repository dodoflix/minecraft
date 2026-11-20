//! `net.minecraft.server.PlayerAdvancements`: one player's advancement progress, the active trigger listeners, the
//! visible subset of the tree sent to the client, and `players/advancements/<uuid>.json`.
//!
//! The state sits behind a lock ([`SharedAdvancements`]) shared by the player list (save, reload, removal) and the
//! `ServerPlayer` (a respawned player keeps its advancements). Operations that run rewards (`award`) never hold the lock
//! while doing so: rewards can fire further triggers (recipe unlocks, loot) on the same player.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use indexmap::IndexMap;
use mc_core::ident::Identifier;
use mc_datafix::{DataFixTypes, UnsupportedVersion, data_fixer};
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, DataResult, Dyn, JSON};
use mc_protocol::packets::play::{
    ClientboundSelectAdvancementsTabPacket, ClientboundUpdateAdvancementsPacket,
    PositionedAdvancement,
};
use parking_lot::{Mutex, MutexGuard};
use serde_json::Value;

use super::advancement::AdvancementHolder;
use super::advancement_progress::AdvancementProgress;
use super::advancement_tree::{AdvancementTree, NodeId};
use super::advancement_visibility_evaluator::evaluate_visibility;
use super::java_hash::{JavaHash, JavaHashMap, JavaHashSet};
use super::server_advancement_manager::ServerAdvancementManager;
use super::triggers::{Criterion, CriterionTriggerInstance};
use crate::server::level::ServerLevel;
use crate::server::level::level_commands::PendingAnnouncement;
use crate::server::players::player_files::check_file_version;
use crate::world::entity::Entity as _;
use crate::world::entity::player::ServerPlayer;

/// `PlayerAdvancements.TriggerInstanceKey`.
#[derive(Clone, PartialEq, Eq)]
pub struct TriggerInstanceKey {
    pub advancement: AdvancementHolder,
    pub criterion: String,
}

impl JavaHash for TriggerInstanceKey {
    /// The record hash: `31 * advancement.hashCode() + criterion.hashCode()`.
    fn java_hash(&self) -> i32 {
        self.advancement
            .java_hash()
            .wrapping_mul(31)
            .wrapping_add(self.criterion.java_hash())
    }
}

/// `PlayerAdvancements.Data.CODEC`: `Codec.unboundedMap(Identifier.CODEC, AdvancementProgress.CODEC)`.
fn data_codec() -> Codec<Vec<(Identifier, AdvancementProgress)>> {
    IDENTIFIER.unbounded_map(&AdvancementProgress::codec())
}

pub struct PlayerAdvancements {
    player_save_path: Option<PathBuf>,
    tree: Arc<AdvancementTree>,
    progress: IndexMap<AdvancementHolder, AdvancementProgress>,
    visible: HashSet<AdvancementHolder>,
    progress_changed: JavaHashSet<AdvancementHolder>,
    /// `rootsToUpdate` (`AdvancementNode`s hash like their holder).
    roots_to_update: JavaHashSet<AdvancementHolder>,
    last_selected_tab: Option<AdvancementHolder>,
    is_first_packet: bool,
    active_triggers:
        HashMap<&'static str, JavaHashMap<TriggerInstanceKey, Arc<dyn CriterionTriggerInstance>>>,
}

impl PlayerAdvancements {
    /// `new PlayerAdvancements(dataFixer, playerList, manager, playerSavePath, player)`: loads the file; one older
    /// than the registered part of the fix chain is an error (vanilla's fixer throws and the constructor ends).
    /// `None` path: nothing is read or written (players without a player list).
    pub fn new(
        manager: &ServerAdvancementManager,
        player_save_path: Option<PathBuf>,
    ) -> Result<PlayerAdvancements, UnsupportedVersion> {
        let mut this = PlayerAdvancements {
            player_save_path,
            tree: manager.tree().clone(),
            progress: IndexMap::new(),
            visible: HashSet::new(),
            progress_changed: JavaHashSet::new(),
            roots_to_update: JavaHashSet::new(),
            last_selected_tab: None,
            is_first_packet: true,
            active_triggers: HashMap::new(),
        };
        this.load(manager)?;
        Ok(this)
    }

    /// Advancements of a player that has no advancement data (an empty manager, no file).
    pub fn detached() -> PlayerAdvancements {
        PlayerAdvancements::new(&ServerAdvancementManager::default(), None)
            .expect("a set without a file has nothing the fixer could refuse")
    }

    /// The progress file this player's advancements are loaded from and saved to (`None`: no file).
    pub fn save_path(&self) -> Option<&std::path::Path> {
        self.player_save_path.as_deref()
    }

    /// `clearTriggers()`.
    pub fn clear_triggers(&mut self) {
        self.active_triggers.clear();
    }

    /// `reload(manager)`. The file was read when the player joined, so it can only fail the fixer when it was replaced
    /// by an older one since; that is logged (vanilla's exception would end the reload).
    pub fn reload(&mut self, manager: &ServerAdvancementManager) {
        self.clear_triggers();
        self.progress.clear();
        self.visible.clear();
        self.roots_to_update.clear();
        self.progress_changed.clear();
        self.is_first_packet = true;
        self.last_selected_tab = None;
        self.tree = manager.tree().clone();
        if let Err(e) = self.load(manager) {
            log::error!("Couldn't reload player advancements: {e}");
        }
    }

    fn register_all_listeners(&mut self, manager: &ServerAdvancementManager) {
        for advancement in manager.get_all_advancements() {
            self.register_listeners(advancement);
        }
    }

    fn load(&mut self, manager: &ServerAdvancementManager) -> Result<(), UnsupportedVersion> {
        if let Some(path) = self.player_save_path.clone().filter(|p| p.is_file()) {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    // the fixer's exception is not a `JsonParseException`: it leaves `load`, the file stays as it is
                    if let Ok(json) = Dyn::from_json_str(&text) {
                        check_file_version(&json)?;
                    }
                    match parse_player_file(&text) {
                        Ok(data) => self.apply_from(manager, data),
                        Err(e) => log::error!(
                            "Couldn't parse player advancements in {}: {e}",
                            path.display()
                        ),
                    }
                }
                Err(e) => log::error!(
                    "Couldn't access player advancements in {}: {e}",
                    path.display()
                ),
            }
        }
        // `checkForAutomaticTriggers`: an advancement without criteria cannot exist (`Advancement.CODEC` rejects it)
        self.register_all_listeners(manager);
        Ok(())
    }

    /// `save()`: logs `Couldn't save player advancements to {}` on failure.
    pub fn save(&self) {
        let Some(path) = &self.player_save_path else {
            return;
        };
        let data: Vec<(Identifier, AdvancementProgress)> = self
            .progress
            .iter()
            .filter(|(_, p)| p.has_progress())
            .map(|(h, p)| (h.id.clone(), p.clone()))
            .collect();
        let Some(json) = progress_file_json(data) else {
            log::error!(
                "Couldn't save player advancements to {}: encoding failed",
                path.display()
            );
            return;
        };
        if let Err(e) = crate::server::players::player_files::save_json(path, &Value::Object(json))
        {
            log::error!(
                "Couldn't save player advancements to {}: {e}",
                path.display()
            );
        }
    }

    fn apply_from(
        &mut self,
        manager: &ServerAdvancementManager,
        mut data: Vec<(Identifier, AdvancementProgress)>,
    ) {
        // `Data.forEach` iterates sorted by progress (first obtained time; stable)
        data.sort_by(|a, b| a.1.compare_to(&b.1));
        for (id, progress) in data {
            match manager.get(&id) {
                None => log::warn!(
                    "Ignored advancement '{id}' in progress file {} - it doesn't exist anymore?",
                    self.player_save_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                ),
                Some(advancement) => {
                    let advancement = advancement.clone();
                    self.start_progress(&advancement, progress);
                    self.progress_changed.insert(advancement.clone());
                    self.mark_for_visibility_update(&advancement);
                }
            }
        }
    }

    /// The state change of `award(holder, criterion)`: `(result, completed just now)`.
    fn award_state(&mut self, holder: &AdvancementHolder, criterion: &str) -> (bool, bool) {
        let mut result = false;
        self.get_or_start_progress(holder);
        let was_done = self.progress[holder].is_done();
        let granted = self
            .progress
            .get_mut(holder)
            .expect("started")
            .grant_progress(criterion);
        let mut newly_done = false;
        if granted {
            self.unregister_listeners(holder);
            self.progress_changed.insert(holder.clone());
            result = true;
            newly_done = !was_done && self.progress[holder].is_done();
        }
        (result, newly_done)
    }

    /// `revoke(advancement, criterion)`.
    pub fn revoke(&mut self, advancement: &AdvancementHolder, criterion: &str) -> bool {
        let mut result = false;
        self.get_or_start_progress(advancement);
        let was_done = self.progress[advancement].is_done();
        if self
            .progress
            .get_mut(advancement)
            .expect("started")
            .revoke_progress(criterion)
        {
            self.register_listeners(advancement);
            self.progress_changed.insert(advancement.clone());
            result = true;
        }
        if was_done && !self.progress[advancement].is_done() {
            self.mark_for_visibility_update(advancement);
        }
        result
    }

    fn mark_for_visibility_update(&mut self, advancement: &AdvancementHolder) {
        if let Some(node) = self.tree.get_holder(advancement) {
            let root = self.tree.root_of(node);
            self.roots_to_update
                .insert(self.tree.node(root).holder.clone());
        }
    }

    fn register_listeners(&mut self, holder: &AdvancementHolder) {
        self.get_or_start_progress(holder);
        let progress = &self.progress[holder];
        if progress.is_done() {
            return;
        }
        let mut to_add = Vec::new();
        for (name, criterion) in &holder.value().criteria {
            if progress.get_criterion(name).is_some_and(|c| !c.is_done()) {
                to_add.push((
                    criterion.clone(),
                    TriggerInstanceKey {
                        advancement: holder.clone(),
                        criterion: name.clone(),
                    },
                ));
            }
        }
        for (criterion, key) in to_add {
            self.add_listener(&criterion, key);
        }
    }

    fn unregister_listeners(&mut self, holder: &AdvancementHolder) {
        self.get_or_start_progress(holder);
        let progress = &self.progress[holder];
        let mut to_remove = Vec::new();
        for (name, criterion) in &holder.value().criteria {
            if progress
                .get_criterion(name)
                .is_some_and(|c| c.is_done() || progress.is_done())
            {
                to_remove.push((
                    criterion.trigger,
                    TriggerInstanceKey {
                        advancement: holder.clone(),
                        criterion: name.clone(),
                    },
                ));
            }
        }
        for (trigger, key) in to_remove {
            self.remove_listener(trigger, &key);
        }
    }

    /// `setSelectedTab(holder)`: the packet to send, if the selection changed.
    fn set_selected_tab_state(
        &mut self,
        holder: Option<&AdvancementHolder>,
    ) -> Option<ClientboundSelectAdvancementsTabPacket> {
        let old = self.last_selected_tab.clone();
        self.last_selected_tab = holder
            .filter(|h| h.value().is_root() && h.value().display.is_some())
            .cloned();
        (old != self.last_selected_tab).then(|| ClientboundSelectAdvancementsTabPacket {
            tab: self.last_selected_tab.as_ref().map(|h| h.id.clone()),
        })
    }

    /// `getOrStartProgress(advancement)`.
    pub fn get_or_start_progress(
        &mut self,
        advancement: &AdvancementHolder,
    ) -> &mut AdvancementProgress {
        if !self.progress.contains_key(advancement) {
            self.start_progress(advancement, AdvancementProgress::new());
        }
        self.progress.get_mut(advancement).expect("started")
    }

    fn start_progress(&mut self, holder: &AdvancementHolder, mut progress: AdvancementProgress) {
        progress.update(&holder.value().requirements);
        self.progress.insert(holder.clone(), progress);
    }

    fn update_tree_visibility(
        &mut self,
        root: NodeId,
        added: &mut JavaHashSet<AdvancementHolder>,
        removed: &mut JavaHashSet<Identifier>,
    ) {
        let tree = self.tree.clone();
        let mut done_cache: HashMap<NodeId, bool> = HashMap::new();
        for n in 0..tree.len() {
            let node = NodeId(n);
            if tree.root_of(node) == root {
                let holder = tree.node(node).holder.clone();
                done_cache.insert(node, self.get_or_start_progress(&holder).is_done());
            }
        }
        let mut changes: Vec<(NodeId, bool)> = Vec::new();
        evaluate_visibility(&tree, root, &mut |n| done_cache[&n], &mut |n, v| {
            changes.push((n, v))
        });
        for (node, should_be_visible) in changes {
            let advancement = tree.node(node).holder.clone();
            if should_be_visible {
                if self.visible.insert(advancement.clone()) {
                    added.insert(advancement.clone());
                    if self.progress.contains_key(&advancement) {
                        self.progress_changed.insert(advancement);
                    }
                }
            } else if self.visible.remove(&advancement) {
                removed.insert(advancement.id.clone());
            }
        }
    }

    /// `flushDirty(player, showAdvancements)`'s packet: `None` when nothing changed.
    fn flush_dirty_state(
        &mut self,
        show_advancements: bool,
    ) -> Option<ClientboundUpdateAdvancementsPacket> {
        let mut packet = None;
        if self.is_first_packet
            || !self.roots_to_update.is_empty()
            || !self.progress_changed.is_empty()
        {
            let mut progress: JavaHashMap<Identifier, AdvancementProgress> = JavaHashMap::new();
            let mut added: JavaHashSet<AdvancementHolder> = JavaHashSet::new();
            let mut removed: JavaHashSet<Identifier> = JavaHashSet::new();
            let roots: Vec<AdvancementHolder> = self.roots_to_update.iter().cloned().collect();
            for root in roots {
                if let Some(node) = self.tree.get_holder(&root) {
                    self.update_tree_visibility(node, &mut added, &mut removed);
                }
            }
            self.roots_to_update.clear();
            let changed: Vec<AdvancementHolder> = self.progress_changed.iter().cloned().collect();
            for holder in changed {
                if self.visible.contains(&holder)
                    && let Some(p) = self.progress.get(&holder)
                {
                    progress.insert(holder.id.clone(), p.clone());
                }
            }
            self.progress_changed.clear();
            if !progress.is_empty() || !added.is_empty() || !removed.is_empty() {
                let converted_added = added
                    .iter()
                    .filter_map(|h| self.tree.get_holder(h))
                    .map(|n| {
                        let node = self.tree.node(n);
                        PositionedAdvancement {
                            advancement: mc_protocol::packets::play::AdvancementHolder {
                                id: node.holder.id.clone(),
                                value: node.holder.value().to_network(),
                            },
                            x: node.x,
                            y: node.y,
                        }
                    })
                    .collect();
                packet = Some(ClientboundUpdateAdvancementsPacket {
                    should_reset: self.is_first_packet,
                    added: converted_added,
                    removed: removed.iter().cloned().collect(),
                    progress: progress
                        .iter()
                        .map(|(k, v)| (k.clone(), v.to_network()))
                        .collect(),
                    show_advancements,
                });
            }
        }
        self.is_first_packet = false;
        packet
    }

    fn add_listener(&mut self, criterion: &Criterion, key: TriggerInstanceKey) {
        self.active_triggers
            .entry(criterion.trigger)
            .or_default()
            .insert(key, criterion.trigger_instance.clone());
    }

    fn remove_listener(&mut self, trigger: &'static str, key: &TriggerInstanceKey) {
        if let Some(map) = self.active_triggers.get_mut(trigger) {
            map.remove(key);
            if map.is_empty() {
                self.active_triggers.remove(trigger);
            }
        }
    }

    /// `getTriggerMapForType(type)` in the map's iteration order.
    pub fn listeners_for(
        &self,
        trigger: &str,
    ) -> Vec<(TriggerInstanceKey, Arc<dyn CriterionTriggerInstance>)> {
        self.active_triggers
            .get(trigger)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    /// The progress of `advancement` if it was started (read-only; `getOrStartProgress` without the insertion).
    pub fn progress_of(&self, advancement: &AdvancementHolder) -> Option<&AdvancementProgress> {
        self.progress.get(advancement)
    }

    /// The holder of a tracked advancement by id (every advancement of the manager is tracked, whether or not the tree has it).
    pub fn holder(&self, id: &Identifier) -> Option<&AdvancementHolder> {
        self.progress.get_key_value(id).map(|(holder, _)| holder)
    }

    pub fn tree(&self) -> &Arc<AdvancementTree> {
        &self.tree
    }
}

/// `DataFixTypes.ADVANCEMENTS.wrapCodec(Data.CODEC, dataFixer, 1343)`.
fn file_codec() -> Codec<Vec<(Identifier, AdvancementProgress)>> {
    DataFixTypes::ADVANCEMENTS.wrap_codec(&data_codec(), data_fixer(), 1343)
}

/// `codec.encodeStart(JsonOps.INSTANCE, data)`: the progress file's object including `DataVersion`.
fn progress_file_json(
    data: Vec<(Identifier, AdvancementProgress)>,
) -> Option<serde_json::Map<String, Value>> {
    match file_codec()
        .encode_start(&JSON, &data)
        .into_result()
        .map(|d| d.to_json_string())
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    {
        Some(Value::Object(json)) => Some(json),
        _ => None,
    }
}

/// Strict JSON parse of a progress file + the wrapped codec (the `DataFixTypes.ADVANCEMENTS` upgrade from the file's
/// `DataVersion`, 1343 when absent). A file older than the registered part of the fix chain is a parse error.
fn parse_player_file(text: &str) -> Result<Vec<(Identifier, AdvancementProgress)>, String> {
    let json = Dyn::from_json_str(text)?;
    match file_codec().parse(&JSON, &json) {
        DataResult::Success(v) => Ok(v),
        DataResult::Error { message, .. } => Err(message.get()),
    }
}

/// The advancements of one player, shared by the player list and the `ServerPlayer`.
#[derive(Clone)]
pub struct SharedAdvancements(Arc<Mutex<PlayerAdvancements>>);

impl SharedAdvancements {
    pub fn new(advancements: PlayerAdvancements) -> Self {
        SharedAdvancements(Arc::new(Mutex::new(advancements)))
    }

    pub fn detached() -> Self {
        Self::new(PlayerAdvancements::detached())
    }

    /// Direct access to the state; never hold the guard across `award`/reward code.
    pub fn lock(&self) -> MutexGuard<'_, PlayerAdvancements> {
        self.0.lock()
    }

    pub fn same_as(&self, other: &SharedAdvancements) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// `award(holder, criterion)`: grants the criterion; when that completes the advancement, the rewards are granted
    /// and the chat announcement sent. Returns whether the criterion was newly granted.
    pub fn award(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        holder: &AdvancementHolder,
        criterion: &str,
    ) -> bool {
        let (result, newly_done) = self.0.lock().award_state(holder, criterion);
        if newly_done {
            holder.value().rewards.grant(level, player);
            if let Some(display) = &holder.value().display
                && display.announce_to_chat
            {
                // vanilla runs a reward function at once, so its output precedes this announcement (and the announcements of
                // awards after it): behind a queued function the announcement waits in the queue too. Inside a command
                // execution vanilla queues the function behind the command and announces at once.
                if level.has_pending_functions()
                    && !crate::commands::commands::is_executing_command()
                {
                    level.queue_announcement(PendingAnnouncement {
                        player: player.id(),
                        advancement: holder.clone(),
                        player_name: player.get_display_name(),
                    });
                } else if level.show_advancement_messages() {
                    let message = display
                        .type_
                        .create_announcement(holder, player.get_display_name());
                    broadcast_system_message(level, &message);
                }
            }
            self.0.lock().mark_for_visibility_update(holder);
        }
        result
    }

    /// `revoke(advancement, criterion)`.
    pub fn revoke(&self, advancement: &AdvancementHolder, criterion: &str) -> bool {
        self.0.lock().revoke(advancement, criterion)
    }

    /// `flushDirty(player, showAdvancements)`.
    pub fn flush_dirty(&self, player: &ServerPlayer, show_advancements: bool) {
        let packet = self.0.lock().flush_dirty_state(show_advancements);
        if let Some(packet) = packet {
            player.send(&packet);
        }
    }

    /// `setSelectedTab(holder)`.
    pub fn set_selected_tab(&self, player: &ServerPlayer, holder: Option<&AdvancementHolder>) {
        let packet = self.0.lock().set_selected_tab_state(holder);
        if let Some(packet) = packet {
            player.send(&packet);
        }
    }

    /// `getOrStartProgress(advancement)` (a snapshot).
    pub fn get_or_start_progress(&self, advancement: &AdvancementHolder) -> AdvancementProgress {
        self.0.lock().get_or_start_progress(advancement).clone()
    }

    /// `getTriggerMapForType(type)`.
    pub fn listeners_for(
        &self,
        trigger: &str,
    ) -> Vec<(TriggerInstanceKey, Arc<dyn CriterionTriggerInstance>)> {
        self.0.lock().listeners_for(trigger)
    }

    pub fn clear_triggers(&self) {
        self.0.lock().clear_triggers();
    }

    pub fn reload(&self, manager: &ServerAdvancementManager) {
        self.0.lock().reload(manager);
    }

    pub fn save(&self) {
        self.0.lock().save();
    }
}

/// `PlayerList.broadcastSystemMessage(message, false)` from a level: the roster handles plus the console log.
pub fn broadcast_system_message(level: &ServerLevel, message: &mc_text::Component) {
    log::info!("System chat: {}", message.get_string());
    for player in level.all_player_handles() {
        player.send_system_message(message, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The obtained times are written in the system's zone: the golden has them in UTC.
    fn utc_times(json: Value) -> Value {
        match json {
            Value::String(s) => {
                match chrono::DateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S %z") {
                    Ok(time) => Value::String(
                        time.with_timezone(&chrono::Utc)
                            .format("%Y-%m-%d %H:%M:%S +0000")
                            .to_string(),
                    ),
                    Err(_) => Value::String(s),
                }
            }
            Value::Object(o) => {
                Value::Object(o.into_iter().map(|(k, v)| (k, utc_times(v))).collect())
            }
            other => other,
        }
    }

    /// The file `PlayerAdvancements.save()` writes equals the real class's output (`saved_data.golden`, `advancements` rows):
    /// progress read from the input, written with Gson's `htmlSafe` escapes of criterion names.
    #[test]
    fn progress_file_bytes_match_vanilla() {
        let golden = include_str!("../../../mc-game/tests/fixtures/saved_data.golden");
        let mut rows = 0;
        for (input, expected) in golden
            .lines()
            .filter_map(|l| l.strip_prefix("advancements "))
            .map(|l| l.split_once('\t').unwrap())
        {
            // the golden builds the progress map straight from the input (no fixer on the way in, `asData()` order):
            // the data fixer's `updateMapValues` would reorder the top-level keys through its hashed intermediate map
            let data = data_codec()
                .parse(&JSON, &Dyn::from_json_str(input).unwrap())
                .into_result()
                .unwrap();
            let json = utc_times(Value::Object(progress_file_json(data).unwrap()));
            let written = crate::server::players::player_files::to_gson_pretty(&json);
            assert_eq!(written.replace('\n', "¶"), expected, "{input}");
            rows += 1;
        }
        assert_eq!(rows, 2);
    }

    fn golden_inputs() -> Vec<&'static str> {
        include_str!("../../../mc-game/tests/fixtures/saved_data.golden")
            .lines()
            .filter_map(|l| l.strip_prefix("advancements "))
            .map(|l| l.split_once('\t').unwrap().0)
            .collect()
    }

    /// The file's own `DataVersion` (set to `version`) selects where the upgrade starts.
    fn with_version(input: &str, version: i32) -> String {
        let mut json: Value = serde_json::from_str(input).unwrap();
        json.as_object_mut()
            .unwrap()
            .insert("DataVersion".into(), Value::from(version));
        json.to_string()
    }

    #[test]
    fn a_file_of_an_older_version_goes_through_the_fixer() {
        for input in golden_inputs() {
            let current = progress_file_json(
                parse_player_file(&with_version(input, mc_datafix::CURRENT_VERSION)).unwrap(),
            );
            let older = progress_file_json(parse_player_file(&with_version(input, 4903)).unwrap());
            assert_eq!(older, current, "{input}");
        }
    }

    #[test]
    fn a_file_below_the_chain_is_a_parse_error_not_a_panic() {
        let oldest = data_fixer().oldest_supported_version();
        if oldest == i32::MIN {
            return;
        }
        let input = golden_inputs()[0];
        let Err(err) = parse_player_file(&with_version(input, oldest - 1)) else {
            panic!("a file below the chain was upgraded");
        };
        assert!(err.contains("older than"), "{err}");
        // no `DataVersion` reads as 1343
        let mut json: Value = serde_json::from_str(input).unwrap();
        json.as_object_mut().unwrap().remove("DataVersion");
        if oldest > 1343 {
            assert!(parse_player_file(&json.to_string()).is_err());
        }
        // the load fails (vanilla's fixer throws) and the file stays as it is: no copy, no overwrite
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.json");
        let text = with_version(input, oldest - 1);
        std::fs::write(&file, &text).unwrap();
        let manager = ServerAdvancementManager::default();
        let err = PlayerAdvancements::new(&manager, Some(file.clone()))
            .err()
            .unwrap();
        assert_eq!(err.from_version, oldest - 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        // a file that is not JSON is still logged and read as empty
        std::fs::write(&file, "not json").unwrap();
        let loaded = PlayerAdvancements::new(&manager, Some(file)).unwrap();
        assert!(loaded.progress.is_empty());
    }
}
