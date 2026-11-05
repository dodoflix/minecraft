//! `Scoreboard` + `ServerScoreboard`, `Objective`, `PlayerTeam`, `ScoreAccess`, `PlayerScores`.
//!
//! One struct implements both: [`Scoreboard::new`] is the plain model (vanilla's `Scoreboard`, whose `on*` hooks do
//! nothing); [`Scoreboard::new_server`] (= [`ServerScoreboard`]) adds the `ServerScoreboard` overrides: dirty tracking
//! for the saved data, objective tracking, and the packets to broadcast, queued in order as [`ScoreboardOutput`]s
//! that the server drains with [`Scoreboard::take_outputs`] after each operation.

use std::collections::{BTreeMap, HashMap, HashSet};

use indexmap::IndexMap;
use crate::util::object_open_hash_map::ObjectOpenHashMap;
use crate::world::entity::tag_set::JavaStringSet;
use mc_nbt::Tag;
use mc_text::util::wrap_in_square_brackets;
use mc_text::{Component, HoverEvent, Style};

use super::number_format::NumberFormat;
use super::objective_criteria::{ObjectiveCriteria, RenderType};
use super::packets::{ScoreboardOutput, ScoreboardPacket, SetObjective, SetPlayerTeam, TeamParameters};
use super::score::{PlayerScoreEntry, Score, ScoreHolder};
use super::scoreboard_save_data::{PackedObjective, PackedScore, PackedTeam, ScoreboardSaveData, ScoreboardSaveDataPacked};
use super::team::{CollisionRule, DisplaySlot, TeamColor, Visibility};

/// `Scoreboard.HIDDEN_SCORE_PREFIX`.
pub const HIDDEN_SCORE_PREFIX: &str = "#";

/// `Objective`.
#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    name: String,
    criteria: ObjectiveCriteria,
    display_name: Component,
    formatted_display_name: Component,
    render_type: RenderType,
    display_auto_update: bool,
    number_format: Option<NumberFormat>,
}

impl Objective {
    fn new(
        name: String,
        criteria: ObjectiveCriteria,
        display_name: Component,
        render_type: RenderType,
        display_auto_update: bool,
        number_format: Option<NumberFormat>,
    ) -> Objective {
        let formatted_display_name = Self::create_formatted_display_name(&name, &display_name);
        Objective { name, criteria, display_name, formatted_display_name, render_type, display_auto_update, number_format }
    }

    fn create_formatted_display_name(name: &str, display_name: &Component) -> Component {
        wrap_in_square_brackets(display_name.copy().with_style_fn(|s| s.with_hover_event(Some(HoverEvent::show_text(Component::literal(name))))))
    }

    pub fn pack(&self) -> PackedObjective {
        PackedObjective {
            name: self.name.clone(),
            criteria: self.criteria.clone(),
            display_name: self.display_name.clone(),
            render_type: self.render_type,
            display_auto_update: self.display_auto_update,
            number_format: self.number_format.clone(),
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }
    pub fn get_criteria(&self) -> &ObjectiveCriteria {
        &self.criteria
    }
    pub fn get_display_name(&self) -> &Component {
        &self.display_name
    }
    pub fn get_formatted_display_name(&self) -> &Component {
        &self.formatted_display_name
    }
    pub fn get_render_type(&self) -> RenderType {
        self.render_type
    }
    pub fn display_auto_update(&self) -> bool {
        self.display_auto_update
    }
    pub fn number_format(&self) -> Option<&NumberFormat> {
        self.number_format.as_ref()
    }
    pub fn number_format_or_default<'a>(&'a self, default: &'a NumberFormat) -> &'a NumberFormat {
        self.number_format.as_ref().unwrap_or(default)
    }

    fn set_packet(&self, method: u8) -> ScoreboardPacket {
        ScoreboardPacket::SetObjective(SetObjective {
            objective_name: self.name.clone(),
            display_name: self.display_name.clone(),
            render_type: self.render_type,
            number_format: self.number_format.clone(),
            method,
        })
    }
}

/// `PlayerTeam`.
#[derive(Debug, Clone)]
pub struct PlayerTeam {
    name: String,
    players: JavaStringSet,
    display_name: Component,
    player_prefix: Component,
    player_suffix: Component,
    allow_friendly_fire: bool,
    see_friendly_invisibles: bool,
    name_tag_visibility: Visibility,
    death_message_visibility: Visibility,
    color: Option<TeamColor>,
    collision_rule: CollisionRule,
    display_name_style: Style,
}

const BIT_FRIENDLY_FIRE: u8 = 1;
const BIT_SEE_INVISIBLES: u8 = 2;

impl PlayerTeam {
    fn new(name: &str) -> PlayerTeam {
        PlayerTeam {
            name: name.to_string(),
            players: JavaStringSet::new(),
            display_name: Component::literal(name),
            player_prefix: Component::empty(),
            player_suffix: Component::empty(),
            allow_friendly_fire: true,
            see_friendly_invisibles: true,
            name_tag_visibility: Visibility::Always,
            death_message_visibility: Visibility::Always,
            color: None,
            collision_rule: CollisionRule::Always,
            display_name_style: Style::EMPTY
                .with_insertion(Some(name.to_string()))
                .with_hover_event(Some(HoverEvent::show_text(Component::literal(name)))),
        }
    }

    pub fn pack(&self) -> PackedTeam {
        PackedTeam {
            name: self.name.clone(),
            display_name: Some(self.display_name.clone()),
            color: self.color,
            allow_friendly_fire: self.allow_friendly_fire,
            see_friendly_invisibles: self.see_friendly_invisibles,
            member_name_prefix: self.player_prefix.clone(),
            member_name_suffix: self.player_suffix.clone(),
            name_tag_visibility: self.name_tag_visibility,
            death_message_visibility: self.death_message_visibility,
            collision_rule: self.collision_rule,
            players: self.players.iter_java_order().into_iter().map(str::to_owned).collect(),
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }
    pub fn get_display_name(&self) -> &Component {
        &self.display_name
    }
    pub fn get_player_prefix(&self) -> &Component {
        &self.player_prefix
    }
    pub fn get_player_suffix(&self) -> &Component {
        &self.player_suffix
    }
    pub fn get_players(&self) -> &JavaStringSet {
        &self.players
    }
    pub fn is_allow_friendly_fire(&self) -> bool {
        self.allow_friendly_fire
    }
    pub fn can_see_friendly_invisibles(&self) -> bool {
        self.see_friendly_invisibles
    }
    pub fn get_name_tag_visibility(&self) -> Visibility {
        self.name_tag_visibility
    }
    pub fn get_death_message_visibility(&self) -> Visibility {
        self.death_message_visibility
    }
    pub fn get_collision_rule(&self) -> CollisionRule {
        self.collision_rule
    }
    pub fn get_color(&self) -> Option<TeamColor> {
        self.color
    }

    /// `Team.isAlliedTo`: same team (identity) — by name within one scoreboard.
    pub fn is_allied_to(&self, other: Option<&PlayerTeam>) -> bool {
        other.is_some_and(|o| o.name == self.name)
    }

    fn apply_color(&self, result: Component) -> Component {
        match self.color {
            Some(c) => result.with_color(c.rgb()),
            None => result,
        }
    }

    pub fn get_formatted_display_name(&self) -> Component {
        self.apply_color(wrap_in_square_brackets(self.display_name.copy().with_style(&self.display_name_style)))
    }

    /// `getFormattedName(teamMemberName)`.
    pub fn get_formatted_name(&self, team_member_name: &Component) -> Component {
        self.apply_color(Component::empty().append(self.player_prefix.clone()).append(team_member_name.clone()).append(self.player_suffix.clone()))
    }

    /// `PlayerTeam.formatNameForTeam(team, name)`.
    pub fn format_name_for_team(team: Option<&PlayerTeam>, name: &Component) -> Component {
        match team {
            None => name.copy(),
            Some(t) => t.get_formatted_name(name),
        }
    }

    /// `packOptions`.
    pub fn pack_options(&self) -> u8 {
        let mut result = 0;
        if self.allow_friendly_fire {
            result |= BIT_FRIENDLY_FIRE;
        }
        if self.see_friendly_invisibles {
            result |= BIT_SEE_INVISIBLES;
        }
        result
    }

    fn parameters(&self) -> TeamParameters {
        TeamParameters {
            display_name: self.display_name.clone(),
            player_prefix: self.player_prefix.clone(),
            player_suffix: self.player_suffix.clone(),
            name_tag_visibility: self.name_tag_visibility,
            collision_rule: self.collision_rule,
            color: self.color,
            options: self.pack_options(),
        }
    }

    /// `ClientboundSetPlayerTeamPacket.createAddOrModifyPacket`.
    pub fn create_add_or_modify_packet(&self, create_new: bool) -> SetPlayerTeam {
        SetPlayerTeam {
            name: self.name.clone(),
            method: if create_new { 0 } else { 2 },
            parameters: Some(self.parameters()),
            players: if create_new { self.players.iter_java_order().into_iter().map(str::to_owned).collect() } else { Vec::new() },
        }
    }

    /// `createRemovePacket`.
    pub fn create_remove_packet(&self) -> SetPlayerTeam {
        SetPlayerTeam { name: self.name.clone(), method: 1, parameters: None, players: Vec::new() }
    }

    /// `createPlayerPacket(team, player, action)`.
    pub fn create_player_packet(&self, player: &str, add: bool) -> SetPlayerTeam {
        SetPlayerTeam { name: self.name.clone(), method: if add { 3 } else { 4 }, parameters: None, players: vec![player.to_string()] }
    }
}

/// `PlayerScores`: one holder's scores by objective name.
#[derive(Debug, Default)]
struct PlayerScores {
    scores: IndexMap<String, Score>,
}

#[derive(Debug, Default)]
struct ServerState {
    tracked_objectives: HashSet<String>,
    dirty: bool,
    outputs: Vec<ScoreboardOutput>,
}

/// `Scoreboard` / `ServerScoreboard`.
#[derive(Debug, Default)]
pub struct Scoreboard {
    objectives_by_name: ObjectOpenHashMap<Objective>,
    objectives_by_criteria: HashMap<ObjectiveCriteria, Vec<String>>,
    player_scores: ObjectOpenHashMap<PlayerScores>,
    display_objectives: BTreeMap<DisplaySlot, String>,
    teams_by_name: ObjectOpenHashMap<PlayerTeam>,
    teams_by_player: HashMap<String, String>,
    server: Option<ServerState>,
}

/// `net.minecraft.server.ServerScoreboard`.
pub type ServerScoreboard = Scoreboard;

/// Errors vanilla reports as exceptions.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScoreboardError {
    #[error("An objective with the name '{0}' already exists!")]
    ObjectiveExists(String),
    #[error("Cannot modify read-only score")]
    ReadOnlyScore,
    #[error("Player is either on another team or not on any team. Cannot remove from team '{0}'.")]
    NotOnTeam(String),
}

impl Scoreboard {
    /// The plain `Scoreboard` (no server hooks).
    pub fn new() -> Scoreboard {
        Scoreboard::default()
    }

    /// A `ServerScoreboard`.
    pub fn new_server() -> Scoreboard {
        Scoreboard { server: Some(ServerState::default()), ..Scoreboard::default() }
    }

    // ---------------- hooks (ServerScoreboard overrides) ----------------

    fn set_dirty(&mut self) {
        if let Some(s) = &mut self.server {
            s.dirty = true;
        }
    }

    fn broadcast(&mut self, packet: ScoreboardPacket) {
        if let Some(s) = &mut self.server {
            s.outputs.push(ScoreboardOutput::Broadcast(packet));
        }
    }

    fn is_tracked(&self, objective: &str) -> bool {
        self.server.as_ref().is_some_and(|s| s.tracked_objectives.contains(objective))
    }

    /// Drains the queued packets / side effects (server mode).
    pub fn take_outputs(&mut self) -> Vec<ScoreboardOutput> {
        self.server.as_mut().map(|s| std::mem::take(&mut s.outputs)).unwrap_or_default()
    }

    fn on_objective_added(&mut self) {
        self.set_dirty();
    }

    fn on_objective_changed(&mut self, objective: &str) {
        if self.server.is_none() {
            return;
        }
        if self.is_tracked(objective) {
            let packet = self.objectives_by_name[objective].set_packet(2);
            self.broadcast(packet);
        }
        self.set_dirty();
    }

    fn on_objective_removed(&mut self, objective: &Objective) {
        if self.server.is_none() {
            return;
        }
        if self.is_tracked(&objective.name) {
            self.stop_tracking_objective_snapshot(objective);
        }
        self.set_dirty();
    }

    fn on_score_changed(&mut self, owner: &str, objective: &str) {
        if self.server.is_none() {
            return;
        }
        if self.is_tracked(objective) {
            let score = &self.player_scores[owner].scores[objective];
            let packet = ScoreboardPacket::SetScore {
                owner: owner.to_string(),
                objective_name: objective.to_string(),
                value: score.value,
                display: score.display.clone(),
                number_format: score.number_format.clone(),
            };
            self.broadcast(packet);
        }
        self.set_dirty();
    }

    fn on_score_lock_changed(&mut self) {
        self.set_dirty();
    }

    fn on_player_removed(&mut self, player: &str) {
        if self.server.is_none() {
            return;
        }
        if !self.server.as_ref().expect("server").tracked_objectives.is_empty() {
            self.broadcast(ScoreboardPacket::ResetScore { owner: player.to_string(), objective_name: None });
        }
        self.set_dirty();
    }

    fn on_player_score_removed(&mut self, player: &str, objective: &str) {
        if self.server.is_none() {
            return;
        }
        if self.is_tracked(objective) {
            self.broadcast(ScoreboardPacket::ResetScore { owner: player.to_string(), objective_name: Some(objective.to_string()) });
        }
        self.set_dirty();
    }

    fn on_team_added(&mut self, team: &str) {
        if self.server.is_none() {
            return;
        }
        let packet = self.teams_by_name[team].create_add_or_modify_packet(true);
        self.broadcast(ScoreboardPacket::SetPlayerTeam(packet));
        self.set_dirty();
    }

    fn on_team_changed(&mut self, team: &str) {
        if self.server.is_none() {
            return;
        }
        let packet = self.teams_by_name[team].create_add_or_modify_packet(false);
        self.broadcast(ScoreboardPacket::SetPlayerTeam(packet));
        self.update_team_waypoints(team);
        self.set_dirty();
    }

    fn on_team_removed(&mut self, team: &PlayerTeam) {
        if self.server.is_none() {
            return;
        }
        self.broadcast(ScoreboardPacket::SetPlayerTeam(team.create_remove_packet()));
        self.push_waypoints(team.players.iter_java_order().into_iter().map(str::to_owned).collect());
        self.set_dirty();
    }

    fn push_waypoints(&mut self, players: Vec<String>) {
        if let Some(s) = &mut self.server {
            s.outputs.push(ScoreboardOutput::RemakeWaypointConnections(players));
        }
    }

    fn update_team_waypoints(&mut self, team: &str) {
        let players = self.teams_by_name[team].players.iter_java_order().into_iter().map(str::to_owned).collect();
        self.push_waypoints(players);
    }

    // ---------------- objectives ----------------

    pub fn get_objective(&self, name: &str) -> Option<&Objective> {
        self.objectives_by_name.get(name)
    }

    /// `addObjective`: `IllegalArgumentException` when the name is taken.
    pub fn add_objective(
        &mut self,
        name: &str,
        criteria: ObjectiveCriteria,
        display_name: Component,
        render_type: RenderType,
        display_auto_update: bool,
        number_format: Option<NumberFormat>,
    ) -> Result<&Objective, ScoreboardError> {
        if self.objectives_by_name.contains_key(name) {
            return Err(ScoreboardError::ObjectiveExists(name.to_string()));
        }
        let objective = Objective::new(name.to_string(), criteria.clone(), display_name, render_type, display_auto_update, number_format);
        self.objectives_by_criteria.entry(criteria).or_default().push(name.to_string());
        self.objectives_by_name.insert(name.to_string(), objective);
        self.on_objective_added();
        Ok(&self.objectives_by_name[name])
    }

    pub fn get_objectives(&self) -> impl Iterator<Item = &Objective> {
        self.objectives_by_name.iter().map(|(_, o)| o)
    }

    pub fn get_objective_names(&self) -> impl Iterator<Item = &str> {
        self.objectives_by_name.iter().map(|(k, _)| k)
    }

    pub fn set_objective_display_name(&mut self, name: &str, display_name: Component) {
        if let Some(o) = self.objectives_by_name.get_mut(name) {
            o.formatted_display_name = Objective::create_formatted_display_name(&o.name, &display_name);
            o.display_name = display_name;
            self.on_objective_changed(name);
        }
    }

    pub fn set_objective_render_type(&mut self, name: &str, render_type: RenderType) {
        if let Some(o) = self.objectives_by_name.get_mut(name) {
            o.render_type = render_type;
            self.on_objective_changed(name);
        }
    }

    pub fn set_objective_display_auto_update(&mut self, name: &str, display_auto_update: bool) {
        if let Some(o) = self.objectives_by_name.get_mut(name) {
            o.display_auto_update = display_auto_update;
            self.on_objective_changed(name);
        }
    }

    pub fn set_objective_number_format(&mut self, name: &str, number_format: Option<NumberFormat>) {
        if let Some(o) = self.objectives_by_name.get_mut(name) {
            o.number_format = number_format;
            self.on_objective_changed(name);
        }
    }

    pub fn remove_objective(&mut self, name: &str) {
        // The display slots are cleared while the objective is still registered: `setDisplayObjective` has to find it
        // to send the stop-tracking packets (vanilla holds the `Objective` instance itself).
        let Some(objective) = self.objectives_by_name.get(name).cloned() else { return };
        for slot in DisplaySlot::VALUES {
            if self.get_display_objective(slot).is_some_and(|o| o.name == objective.name) {
                self.set_display_objective(slot, None);
            }
        }
        self.objectives_by_name.remove(name);
        if let Some(list) = self.objectives_by_criteria.get_mut(&objective.criteria) {
            list.retain(|n| *n != objective.name);
        }
        for scores in self.player_scores.values_mut() {
            scores.scores.shift_remove(&objective.name);
        }
        self.on_objective_removed(&objective);
    }

    // ---------------- scores ----------------

    /// `forAllObjectives(criteria, name, operation)`: the holder's (writable) score in every objective of `criteria`.
    pub fn for_all_objectives(&mut self, criteria: &ObjectiveCriteria, holder: &dyn ScoreHolder, mut operation: impl FnMut(&mut ScoreAccess<'_>)) {
        let names = self.objectives_by_criteria.get(criteria).cloned().unwrap_or_default();
        for objective in names {
            if let Some(mut access) = self.get_or_create_player_score_forced(holder, &objective, true) {
                operation(&mut access);
            }
        }
    }

    pub fn get_or_create_player_score(&mut self, holder: &dyn ScoreHolder, objective: &str) -> Option<ScoreAccess<'_>> {
        self.get_or_create_player_score_forced(holder, objective, false)
    }

    /// `getOrCreatePlayerScore(holder, objective, forceWritable)`; `None` when the objective does not exist.
    pub fn get_or_create_player_score_forced(&mut self, holder: &dyn ScoreHolder, objective: &str, force_writable: bool) -> Option<ScoreAccess<'_>> {
        let obj = self.objectives_by_name.get(objective)?;
        let can_modify = force_writable || !obj.criteria.is_read_only();
        let display_auto_update = obj.display_auto_update;
        let holder_name = holder.get_scoreboard_name();
        let scores = &mut self.player_scores.get_or_insert_with(&holder_name, PlayerScores::default).scores;
        let mut requires_sync = false;
        if !scores.contains_key(objective) {
            scores.insert(objective.to_string(), Score::default());
            requires_sync = true;
        }
        let holder_display = if display_auto_update { holder.get_display_name() } else { None };
        Some(ScoreAccess { scoreboard: self, holder: holder_name, holder_display, objective: objective.to_string(), can_modify, requires_sync })
    }

    pub fn get_player_score_info(&self, holder: &dyn ScoreHolder, objective: &str) -> Option<&Score> {
        self.player_scores.get(&holder.get_scoreboard_name())?.scores.get(objective)
    }

    /// `listPlayerScores(objective)` (`playerScores.forEach`: fastutil iterator order).
    pub fn list_player_scores(&self, objective: &str) -> Vec<PlayerScoreEntry> {
        self.player_scores
            .iter()
            .filter_map(|(player, scores)| {
                scores.scores.get(objective).map(|s| PlayerScoreEntry {
                    owner: player.to_owned(),
                    value: s.value,
                    display: s.display.clone(),
                    number_format_override: s.number_format.clone(),
                })
            })
            .collect()
    }

    /// `listPlayerScores(holder)`: objective name -> value.
    pub fn list_player_scores_for(&self, holder: &dyn ScoreHolder) -> IndexMap<String, i32> {
        self.player_scores
            .get(&holder.get_scoreboard_name())
            .map(|ps| ps.scores.iter().map(|(o, s)| (o.clone(), s.value)).collect())
            .unwrap_or_default()
    }

    /// `getTrackedPlayers` (the holders' names; `keySet().stream()` order).
    pub fn get_tracked_players(&self) -> Vec<String> {
        self.player_scores.iter_stream().map(|(k, _)| k.to_owned()).collect()
    }

    pub fn reset_all_player_scores(&mut self, holder: &dyn ScoreHolder) {
        let name = holder.get_scoreboard_name();
        if self.player_scores.remove(&name).is_some() {
            self.on_player_removed(&name);
        }
    }

    pub fn reset_single_player_score(&mut self, holder: &dyn ScoreHolder, objective: &str) {
        let name = holder.get_scoreboard_name();
        let Some(scores) = self.player_scores.get_mut(&name) else { return };
        let has_removed = scores.scores.shift_remove(objective).is_some();
        if scores.scores.is_empty() {
            if self.player_scores.remove(&name).is_some() {
                self.on_player_removed(&name);
            }
        } else if has_removed {
            self.on_player_score_removed(&name, objective);
        }
    }

    /// `entityRemoved`: a dead non-player entity loses its scores and team.
    pub fn entity_removed(&mut self, holder: &dyn ScoreHolder, is_player: bool, is_alive: bool) {
        if !is_player && !is_alive {
            self.reset_all_player_scores(holder);
            self.remove_player_from_team(&holder.get_scoreboard_name());
        }
    }

    // ---------------- display slots ----------------

    pub fn get_display_objective(&self, slot: DisplaySlot) -> Option<&Objective> {
        self.display_objectives.get(&slot).and_then(|n| self.objectives_by_name.get(n))
    }

    /// `setDisplayObjective(slot, objective)` (`ServerScoreboard` override included).
    pub fn set_display_objective(&mut self, slot: DisplaySlot, objective: Option<&str>) {
        let old = self.display_objectives.get(&slot).cloned();
        match objective {
            Some(o) => {
                self.display_objectives.insert(slot, o.to_string());
            }
            None => {
                self.display_objectives.remove(&slot);
            }
        }
        if self.server.is_none() {
            return;
        }
        let new = objective.map(str::to_string);
        if old != new
            && let Some(old) = old
        {
            if self.get_objective_display_slot_count(&old) > 0 {
                self.broadcast(ScoreboardPacket::SetDisplayObjective { slot, objective_name: new.clone() });
            } else {
                self.stop_tracking_objective_by_name(&old);
            }
        }
        if let Some(new) = new {
            if self.is_tracked(&new) {
                self.broadcast(ScoreboardPacket::SetDisplayObjective { slot, objective_name: Some(new) });
            } else {
                self.start_tracking_objective(&new);
            }
        }
        self.set_dirty();
    }

    pub fn get_objective_display_slot_count(&self, objective: &str) -> usize {
        DisplaySlot::VALUES.iter().filter(|s| self.display_objectives.get(s).is_some_and(|o| o == objective)).count()
    }

    // ---------------- objective tracking (ServerScoreboard) ----------------

    fn display_slot_packets(&self, objective: &str) -> Vec<ScoreboardPacket> {
        DisplaySlot::VALUES
            .iter()
            .filter(|s| self.display_objectives.get(s).is_some_and(|o| o == objective))
            .map(|&slot| ScoreboardPacket::SetDisplayObjective { slot, objective_name: Some(objective.to_string()) })
            .collect()
    }

    /// `getStartTrackingPackets`.
    pub fn get_start_tracking_packets(&self, objective: &Objective) -> Vec<ScoreboardPacket> {
        let mut packets = vec![objective.set_packet(0)];
        packets.extend(self.display_slot_packets(&objective.name));
        for score in self.list_player_scores(&objective.name) {
            packets.push(ScoreboardPacket::SetScore {
                owner: score.owner,
                objective_name: objective.name.clone(),
                value: score.value,
                display: score.display,
                number_format: score.number_format_override,
            });
        }
        packets
    }

    /// `getStopTrackingPackets`.
    pub fn get_stop_tracking_packets(&self, objective: &Objective) -> Vec<ScoreboardPacket> {
        let mut packets = vec![objective.set_packet(1)];
        packets.extend(self.display_slot_packets(&objective.name));
        packets
    }

    /// `startTrackingObjective`: sends the start packets to every player and marks the objective tracked.
    pub fn start_tracking_objective(&mut self, name: &str) {
        let Some(objective) = self.objectives_by_name.get(name) else { return };
        let packets = self.get_start_tracking_packets(objective);
        for p in packets {
            self.broadcast(p);
        }
        if let Some(s) = &mut self.server {
            s.tracked_objectives.insert(name.to_string());
        }
    }

    fn stop_tracking_objective_snapshot(&mut self, objective: &Objective) {
        for p in self.get_stop_tracking_packets(objective) {
            self.broadcast(p);
        }
        if let Some(s) = &mut self.server {
            s.tracked_objectives.remove(&objective.name);
        }
    }

    fn stop_tracking_objective_by_name(&mut self, name: &str) {
        if let Some(objective) = self.objectives_by_name.get(name).cloned() {
            self.stop_tracking_objective_snapshot(&objective);
        }
    }

    /// `stopTrackingObjective`.
    pub fn stop_tracking_objective(&mut self, name: &str) {
        self.stop_tracking_objective_by_name(name);
    }

    pub fn is_tracking_objective(&self, name: &str) -> bool {
        self.is_tracked(name)
    }

    // ---------------- teams ----------------

    pub fn get_player_team(&self, name: &str) -> Option<&PlayerTeam> {
        self.teams_by_name.get(name)
    }

    /// `addPlayerTeam`: an existing team is returned (with a warning).
    pub fn add_player_team(&mut self, name: &str) -> &PlayerTeam {
        if self.teams_by_name.contains_key(name) {
            log::warn!("Requested creation of existing team '{name}'");
        } else {
            self.teams_by_name.insert(name.to_string(), PlayerTeam::new(name));
            self.on_team_added(name);
        }
        &self.teams_by_name[name]
    }

    pub fn remove_player_team(&mut self, name: &str) {
        let Some(team) = self.teams_by_name.remove(name) else { return };
        for player in team.players.iter_java_order() {
            self.teams_by_player.remove(player);
        }
        self.on_team_removed(&team);
    }

    /// `addPlayerToTeam`: leaves the previous team first; `false` when already a member.
    pub fn add_player_to_team(&mut self, player: &str, team: &str) -> bool {
        if !self.teams_by_name.contains_key(team) {
            return false;
        }
        if self.get_players_team(player).is_some() {
            self.remove_player_from_team(player);
        }
        self.teams_by_player.insert(player.to_string(), team.to_string());
        let added = self.teams_by_name.get_mut(team).expect("checked").players.insert(player.to_string());
        if self.server.is_some() {
            if added {
                let packet = self.teams_by_name[team].create_player_packet(player, true);
                self.broadcast(ScoreboardPacket::SetPlayerTeam(packet));
                self.push_waypoints(vec![player.to_string()]);
                self.set_dirty();
            }
        }
        added
    }

    /// `removePlayerFromTeam(player)`.
    pub fn remove_player_from_team(&mut self, player: &str) -> bool {
        match self.get_players_team(player).map(|t| t.name.clone()) {
            Some(team) => {
                let _ = self.remove_player_from_team_of(player, &team);
                true
            }
            None => false,
        }
    }

    /// `removePlayerFromTeam(player, team)`: `IllegalStateException` when `player` is not on `team`.
    pub fn remove_player_from_team_of(&mut self, player: &str, team: &str) -> Result<(), ScoreboardError> {
        if self.teams_by_player.get(player).map(String::as_str) != Some(team) {
            return Err(ScoreboardError::NotOnTeam(team.to_string()));
        }
        self.teams_by_player.remove(player);
        if let Some(t) = self.teams_by_name.get_mut(team) {
            t.players.remove(player);
        }
        if self.server.is_some() {
            if let Some(t) = self.teams_by_name.get(team) {
                let packet = t.create_player_packet(player, false);
                self.broadcast(ScoreboardPacket::SetPlayerTeam(packet));
            }
            self.push_waypoints(vec![player.to_string()]);
            self.set_dirty();
        }
        Ok(())
    }

    pub fn get_team_names(&self) -> impl Iterator<Item = &str> {
        self.teams_by_name.iter().map(|(k, _)| k)
    }

    pub fn get_player_teams(&self) -> impl Iterator<Item = &PlayerTeam> {
        self.teams_by_name.iter().map(|(_, t)| t)
    }

    pub fn get_players_team(&self, name: &str) -> Option<&PlayerTeam> {
        self.teams_by_player.get(name).and_then(|t| self.teams_by_name.get(t))
    }

    /// A handle for the `PlayerTeam` setters; each setter fires `onTeamChanged`.
    pub fn team_mut(&mut self, name: &str) -> Option<TeamMut<'_>> {
        self.teams_by_name.contains_key(name).then(|| TeamMut { scoreboard: self, name: name.to_string() })
    }

    // ---------------- persistence ----------------

    fn pack_player_scores(&self) -> Vec<PackedScore> {
        self.player_scores
            .iter_stream()
            .flat_map(|(player, ps)| ps.scores.iter().map(move |(objective, score)| PackedScore { owner: player.to_owned(), objective: objective.clone(), score: score.clone() }))
            .collect()
    }

    fn pack_display_slots(&self) -> BTreeMap<DisplaySlot, String> {
        self.display_objectives.clone()
    }

    /// `ServerScoreboard.store()`.
    pub fn store(&self) -> ScoreboardSaveDataPacked {
        ScoreboardSaveDataPacked {
            objectives: self.objectives_by_name.iter_stream().map(|(_, o)| o.pack()).collect(),
            scores: self.pack_player_scores(),
            display_slots: self.pack_display_slots(),
            teams: self.teams_by_name.iter_stream().map(|(_, t)| t.pack()).collect(),
        }
    }

    /// `ServerScoreboard.load(data)`: loads through the normal mutators (so tracking starts for displayed
    /// objectives); the packets this queues have no recipients at load time and are dropped.
    pub fn load(&mut self, data: &ScoreboardSaveDataPacked) {
        for o in &data.objectives {
            if let Err(e) = self.add_objective(&o.name, o.criteria.clone(), o.display_name.clone(), o.render_type, o.display_auto_update, o.number_format.clone()) {
                log::error!("{e}");
            }
        }
        for s in &data.scores {
            match self.objectives_by_name.contains_key(&s.objective) {
                true => {
                    self.player_scores.get_or_insert_with(&s.owner, PlayerScores::default).scores.insert(s.objective.clone(), s.score.clone());
                }
                false => log::error!("Unknown objective {} for name {}, ignoring", s.objective, s.owner),
            }
        }
        for (slot, name) in &data.display_slots {
            let objective = self.objectives_by_name.contains_key(name).then_some(name.as_str());
            self.set_display_objective(*slot, objective);
        }
        for t in &data.teams {
            self.add_player_team(&t.name);
            let mut team = self.team_mut(&t.name).expect("just added");
            if let Some(d) = &t.display_name {
                team.set_display_name(d.clone());
            }
            team.set_color(t.color);
            team.set_allow_friendly_fire(t.allow_friendly_fire);
            team.set_see_friendly_invisibles(t.see_friendly_invisibles);
            team.set_player_prefix(Some(t.member_name_prefix.clone()));
            team.set_player_suffix(Some(t.member_name_suffix.clone()));
            team.set_name_tag_visibility(t.name_tag_visibility);
            team.set_death_message_visibility(t.death_message_visibility);
            team.set_collision_rule(t.collision_rule);
            for player in &t.players {
                self.add_player_to_team(player, &t.name);
            }
        }
        if let Some(s) = &mut self.server {
            s.outputs.clear();
        }
    }

    /// `storeToSaveDataIfDirty`.
    pub fn store_to_save_data_if_dirty(&mut self, save_data: &mut ScoreboardSaveData) {
        if self.server.as_ref().is_some_and(|s| s.dirty) {
            self.server.as_mut().expect("server").dirty = false;
            save_data.set_data(self.store());
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.server.as_ref().is_some_and(|s| s.dirty)
    }

    /// `Tag` of the stored data, for tests and tools.
    pub fn store_to_tag(&self) -> Result<Tag, String> {
        self.store().encode()
    }
}

/// `ScoreAccess` for one (holder, objective) pair; created by [`Scoreboard::get_or_create_player_score`].
pub struct ScoreAccess<'a> {
    scoreboard: &'a mut Scoreboard,
    holder: String,
    /// `scoreHolder.getDisplayName()` when the objective auto-updates displays.
    holder_display: Option<Component>,
    objective: String,
    can_modify: bool,
    requires_sync: bool,
}

impl ScoreAccess<'_> {
    fn score(&self) -> &Score {
        &self.scoreboard.player_scores[self.holder.as_str()].scores[&self.objective]
    }

    fn score_mut(&mut self) -> &mut Score {
        &mut self.scoreboard.player_scores.get_mut(&self.holder).expect("holder exists").scores[&self.objective]
    }

    fn send_score_to_players(&mut self) {
        self.scoreboard.on_score_changed(&self.holder, &self.objective);
        self.requires_sync = false;
    }

    pub fn get(&self) -> i32 {
        self.score().value
    }

    /// `set(value)`: `IllegalStateException` for read-only criteria.
    pub fn set(&mut self, value: i32) -> Result<(), ScoreboardError> {
        if !self.can_modify {
            return Err(ScoreboardError::ReadOnlyScore);
        }
        let mut has_changed = self.requires_sync;
        if let Some(new_display) = self.holder_display.clone()
            && self.score().display.as_ref() != Some(&new_display)
        {
            self.score_mut().display = Some(new_display);
            has_changed = true;
        }
        if value != self.score().value {
            self.score_mut().value = value;
            has_changed = true;
        }
        if has_changed {
            self.send_score_to_players();
        }
        Ok(())
    }

    /// `add(count)` (Java `int` wrap).
    pub fn add(&mut self, count: i32) -> Result<i32, ScoreboardError> {
        let new_value = self.get().wrapping_add(count);
        self.set(new_value)?;
        Ok(new_value)
    }

    pub fn increment(&mut self) -> Result<i32, ScoreboardError> {
        self.add(1)
    }

    pub fn reset(&mut self) -> Result<(), ScoreboardError> {
        self.set(0)
    }

    pub fn locked(&self) -> bool {
        self.score().locked
    }

    pub fn unlock(&mut self) {
        self.set_locked(false);
    }

    pub fn lock(&mut self) {
        self.set_locked(true);
    }

    fn set_locked(&mut self, locked: bool) {
        self.score_mut().locked = locked;
        if self.requires_sync {
            self.send_score_to_players();
        }
        self.scoreboard.on_score_lock_changed();
    }

    pub fn display(&self) -> Option<&Component> {
        self.score().display.as_ref()
    }

    pub fn set_display(&mut self, display: Option<Component>) {
        if self.requires_sync || display != self.score().display {
            self.score_mut().display = display;
            self.send_score_to_players();
        }
    }

    pub fn number_format_override(&mut self, number_format: Option<NumberFormat>) {
        self.score_mut().number_format = number_format;
        self.send_score_to_players();
    }
}

impl<'a> ScoreAccess<'a> {
    /// Drops the scoreboard borrow (keeping the access' own state: `requiresSync`, writability, holder display) so
    /// two accesses can be alive at once like vanilla's `operation` does.
    pub fn detach(self) -> DetachedScoreAccess {
        DetachedScoreAccess {
            holder: self.holder,
            holder_display: self.holder_display,
            objective: self.objective,
            can_modify: self.can_modify,
            requires_sync: self.requires_sync,
        }
    }
}

/// A [`ScoreAccess`] that does not borrow the scoreboard; [`attach`](DetachedScoreAccess::attach) it for each use.
#[derive(Debug, Clone)]
pub struct DetachedScoreAccess {
    holder: String,
    holder_display: Option<Component>,
    objective: String,
    can_modify: bool,
    requires_sync: bool,
}

impl DetachedScoreAccess {
    pub fn attach(self, scoreboard: &mut Scoreboard) -> ScoreAccess<'_> {
        ScoreAccess {
            scoreboard,
            holder: self.holder,
            holder_display: self.holder_display,
            objective: self.objective,
            can_modify: self.can_modify,
            requires_sync: self.requires_sync,
        }
    }
}

/// Mutating handle for a [`PlayerTeam`]: every setter fires `onTeamChanged` like vanilla's.
pub struct TeamMut<'a> {
    scoreboard: &'a mut Scoreboard,
    name: String,
}

impl TeamMut<'_> {
    fn team(&mut self) -> &mut PlayerTeam {
        self.scoreboard.teams_by_name.get_mut(&self.name).expect("team exists")
    }

    fn changed(&mut self) {
        self.scoreboard.on_team_changed(&self.name);
    }

    pub fn set_display_name(&mut self, display_name: Component) {
        self.team().display_name = display_name;
        self.changed();
    }

    pub fn set_player_prefix(&mut self, prefix: Option<Component>) {
        self.team().player_prefix = prefix.unwrap_or_else(Component::empty);
        self.changed();
    }

    pub fn set_player_suffix(&mut self, suffix: Option<Component>) {
        self.team().player_suffix = suffix.unwrap_or_else(Component::empty);
        self.changed();
    }

    pub fn set_allow_friendly_fire(&mut self, v: bool) {
        self.team().allow_friendly_fire = v;
        self.changed();
    }

    pub fn set_see_friendly_invisibles(&mut self, v: bool) {
        self.team().see_friendly_invisibles = v;
        self.changed();
    }

    pub fn set_name_tag_visibility(&mut self, v: Visibility) {
        self.team().name_tag_visibility = v;
        self.changed();
    }

    pub fn set_death_message_visibility(&mut self, v: Visibility) {
        self.team().death_message_visibility = v;
        self.changed();
    }

    pub fn set_collision_rule(&mut self, v: CollisionRule) {
        self.team().collision_rule = v;
        self.changed();
    }

    pub fn set_color(&mut self, color: Option<TeamColor>) {
        self.team().color = color;
        self.changed();
    }

    /// `unpackOptions`: two `onTeamChanged` events.
    pub fn unpack_options(&mut self, options: u8) {
        self.set_allow_friendly_fire(options & BIT_FRIENDLY_FIRE != 0);
        self.set_see_friendly_invisibles(options & BIT_SEE_INVISIBLES != 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::scores::score::for_name_only;

    fn board() -> Scoreboard {
        let mut b = Scoreboard::new_server();
        b.add_objective("o", ObjectiveCriteria::DUMMY, Component::literal("O"), RenderType::Integer, false, None).unwrap();
        b
    }

    fn packets(b: &mut Scoreboard) -> Vec<ScoreboardPacket> {
        b.take_outputs()
            .into_iter()
            .filter_map(|o| match o {
                ScoreboardOutput::Broadcast(p) => Some(p),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn duplicate_objective_is_rejected() {
        let mut b = board();
        let err = b.add_objective("o", ObjectiveCriteria::DUMMY, Component::literal("O"), RenderType::Integer, false, None).unwrap_err();
        assert_eq!(err.to_string(), "An objective with the name 'o' already exists!");
    }

    #[test]
    fn score_access_sync_and_read_only() {
        let mut b = board();
        b.add_objective("hp", ObjectiveCriteria::HEALTH, Component::literal("hp"), RenderType::Hearts, false, None).unwrap();
        let a = for_name_only("a");
        assert_eq!(b.get_or_create_player_score(&a, "hp").unwrap().set(3), Err(ScoreboardError::ReadOnlyScore));
        assert!(b.get_or_create_player_score_forced(&a, "hp", true).unwrap().set(3).is_ok());
        b.take_outputs();
        // untracked objective: scores change silently, but the data is dirty
        b.get_or_create_player_score(&a, "o").unwrap().set(1).unwrap();
        assert!(packets(&mut b).is_empty());
        // tracking starts when the objective is shown: start packets carry the existing scores
        b.set_display_objective(DisplaySlot::Sidebar, Some("o"));
        let p = packets(&mut b);
        assert!(matches!(&p[0], ScoreboardPacket::SetObjective(o) if o.method == 0 && o.objective_name == "o"));
        assert!(matches!(&p[1], ScoreboardPacket::SetDisplayObjective { slot: DisplaySlot::Sidebar, .. }));
        assert!(matches!(&p[2], ScoreboardPacket::SetScore { owner, value: 1, .. } if owner == "a"));
        // a tracked score change broadcasts once, an equal set does not
        let mut s = b.get_or_create_player_score(&a, "o").unwrap();
        s.set(7).unwrap();
        s.set(7).unwrap();
        assert_eq!(s.add(1), Ok(8));
        drop(s);
        let p = packets(&mut b);
        assert_eq!(p.len(), 2);
        assert!(matches!(&p[1], ScoreboardPacket::SetScore { value: 8, .. }));
        // read-only objective is not tracked: no packets
        b.reset_single_player_score(&a, "o");
        let p = packets(&mut b);
        assert_eq!(p, vec![ScoreboardPacket::ResetScore { owner: "a".into(), objective_name: Some("o".into()) }]);
        // removing the last score of a holder resets the whole holder
        b.reset_single_player_score(&a, "hp");
        assert_eq!(packets(&mut b), vec![ScoreboardPacket::ResetScore { owner: "a".into(), objective_name: None }]);
    }

    #[test]
    fn display_slot_tracking_follows_vanilla() {
        let mut b = board();
        b.add_objective("p", ObjectiveCriteria::DUMMY, Component::literal("P"), RenderType::Integer, false, None).unwrap();
        b.set_display_objective(DisplaySlot::Sidebar, Some("o"));
        b.set_display_objective(DisplaySlot::List, Some("o"));
        b.take_outputs();
        // `o` still shown in LIST: the slot change is a plain display packet
        b.set_display_objective(DisplaySlot::Sidebar, Some("p"));
        let p = packets(&mut b);
        assert_eq!(p[0], ScoreboardPacket::SetDisplayObjective { slot: DisplaySlot::Sidebar, objective_name: Some("p".into()) });
        assert!(matches!(&p[1], ScoreboardPacket::SetObjective(o) if o.objective_name == "p" && o.method == 0));
        // last slot of `o` cleared: tracking stops
        b.set_display_objective(DisplaySlot::List, None);
        let p = packets(&mut b);
        assert!(matches!(&p[0], ScoreboardPacket::SetObjective(o) if o.objective_name == "o" && o.method == 1));
        assert!(!b.is_tracking_objective("o"));
        // removing a displayed objective untracks it too
        b.remove_objective("p");
        let p = packets(&mut b);
        assert!(p.iter().any(|x| matches!(x, ScoreboardPacket::SetObjective(o) if o.objective_name == "p" && o.method == 1)));
        assert!(b.get_display_objective(DisplaySlot::Sidebar).is_none());
    }

    #[test]
    fn detached_accesses_keep_their_sync_state() {
        let mut b = board();
        b.set_display_objective(DisplaySlot::Sidebar, Some("o"));
        b.take_outputs();
        let (x, y) = (for_name_only("x"), for_name_only("y"));
        // two accesses alive at once (`/scoreboard players operation`): the first creates `x`'s score, the second `y`'s
        let a = b.get_or_create_player_score(&x, "o").unwrap().detach();
        let mut s = b.get_or_create_player_score(&y, "o").unwrap().detach();
        assert!(packets(&mut b).is_empty(), "creating a score sends nothing");
        let mut access = s.attach(&mut b);
        access.set(0).unwrap();
        s = access.detach();
        // a new score is synced by its first `set` even when the value stays 0, a second `set` is not
        assert_eq!(packets(&mut b).len(), 1);
        s.attach(&mut b).set(0).unwrap();
        assert!(packets(&mut b).is_empty());
        a.attach(&mut b).set(5).unwrap();
        assert_eq!(packets(&mut b).len(), 1);
        assert_eq!(b.get_player_score_info(&x, "o").unwrap().value(), 5);
    }

    #[test]
    fn team_membership_and_packets() {
        let mut b = Scoreboard::new_server();
        b.add_player_team("a");
        b.add_player_team("b");
        assert!(b.add_player_to_team("x", "a"));
        b.take_outputs();
        assert!(b.add_player_to_team("x", "b"));
        let p = packets(&mut b);
        assert_eq!(p.len(), 2, "leave a, join b");
        assert!(matches!(&p[0], ScoreboardPacket::SetPlayerTeam(t) if t.name == "a" && t.method == 4 && t.players == ["x"]));
        assert!(matches!(&p[1], ScoreboardPacket::SetPlayerTeam(t) if t.name == "b" && t.method == 3));
        assert_eq!(b.get_players_team("x").unwrap().get_name(), "b");
        assert_eq!(b.remove_player_from_team_of("x", "a"), Err(ScoreboardError::NotOnTeam("a".into())));
        b.team_mut("b").unwrap().set_color(Some(TeamColor::Blue));
        let p = packets(&mut b);
        assert!(matches!(&p[0], ScoreboardPacket::SetPlayerTeam(t) if t.method == 2 && t.players.is_empty() && t.parameters.as_ref().unwrap().color == Some(TeamColor::Blue) && t.parameters.as_ref().unwrap().options == 3));
        b.remove_player_team("b");
        assert!(b.get_players_team("x").is_none());
        assert!(matches!(&packets(&mut b)[0], ScoreboardPacket::SetPlayerTeam(t) if t.method == 1));
    }

    #[test]
    fn team_display_formatting() {
        let mut b = Scoreboard::new();
        b.add_player_team("t");
        let mut t = b.team_mut("t").unwrap();
        t.set_color(Some(TeamColor::Red));
        t.set_player_prefix(Some(Component::literal("<")));
        t.set_player_suffix(Some(Component::literal(">")));
        let t = b.get_player_team("t").unwrap();
        assert_eq!(t.get_formatted_name(&Component::literal("n")).get_string(), "<n>");
        assert_eq!(t.get_formatted_display_name().get_string(), "[t]");
        assert_eq!(t.pack_options(), 3);
        assert!(b.take_outputs().is_empty(), "plain scoreboards queue nothing");
    }

    #[test]
    fn auto_update_copies_holder_display_name() {
        struct Named;
        impl ScoreHolder for Named {
            fn get_scoreboard_name(&self) -> String {
                "uuid".into()
            }
            fn get_display_name(&self) -> Option<Component> {
                Some(Component::literal("Pretty"))
            }
        }
        let mut b = Scoreboard::new();
        b.add_objective("o", ObjectiveCriteria::DUMMY, Component::literal("O"), RenderType::Integer, true, None).unwrap();
        b.get_or_create_player_score(&Named, "o").unwrap().set(1).unwrap();
        assert_eq!(b.get_player_score_info(&Named, "o").unwrap().display(), Some(&Component::literal("Pretty")));
    }
}
