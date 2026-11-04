//! `EntitySelector`: a parsed selector and its evaluation against the server's entities.
//!
//! Results are [`EntityRef`]s (level + entity id); the live entity is read with `host::get_entity`. Evaluation takes the
//! server explicitly (`&MinecraftServer`, shared) because the predicates read scoreboard/player state.

use crate::commands::arguments::min_max_bounds::Doubles;
use crate::commands::host;
use crate::commands::message::message;
use crate::commands::CommandSourceStack;
use crate::server::MinecraftServer;
use crate::server::permissions::permissions;
use crate::translatable;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::DimensionKey;
use mc_brigadier::{CommandSyntaxException, SimpleCommandExceptionType};
use mc_core::phys::{AABB, Vec3};
use mc_data::EntityType;
use mc_text::Component;
use rand::Rng;
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

pub static ERROR_NOT_SINGLE_ENTITY: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| SimpleCommandExceptionType::new(message(translatable!("argument.entity.toomany"))));
pub static ERROR_NOT_SINGLE_PLAYER: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| SimpleCommandExceptionType::new(message(translatable!("argument.player.toomany"))));
pub static ERROR_ONLY_PLAYERS_ALLOWED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| SimpleCommandExceptionType::new(message(translatable!("argument.player.entities"))));
pub static NO_ENTITIES_FOUND: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| SimpleCommandExceptionType::new(message(translatable!("argument.entity.notfound.entity"))));
pub static NO_PLAYERS_FOUND: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| SimpleCommandExceptionType::new(message(translatable!("argument.entity.notfound.player"))));
pub static ERROR_SELECTORS_NOT_ALLOWED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| SimpleCommandExceptionType::new(message(translatable!("argument.entity.selector.not_allowed"))));

/// An entity of a level (the entity itself is read through the server).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct EntityRef {
    pub dimension: DimensionKey,
    pub id: EntityId,
}

/// What a predicate may consult besides the entity.
pub struct SelectorContext<'a> {
    pub server: &'a MinecraftServer,
    /// The level the entity is in.
    pub dimension: &'a DimensionKey,
}

/// `Predicate<Entity>`.
pub type EntityPredicate = Arc<dyn Fn(&SelectorContext<'_>, &dyn Entity) -> bool + Send + Sync>;

/// `BiConsumer<Vec3, List<Entity>>` orders.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Order {
    /// `EntitySelector.ORDER_ARBITRARY`.
    Arbitrary,
    Nearest,
    Furthest,
    Random,
}

/// `Doubles.compare` (`Double.compare`).
fn compare_doubles(a: f64, b: f64) -> std::cmp::Ordering {
    a.total_cmp(&b)
}

#[derive(Clone)]
pub struct EntitySelector {
    max_results: i32,
    includes_entities: bool,
    world_limited: bool,
    context_free_predicates: Vec<EntityPredicate>,
    range: Option<Doubles>,
    /// The `x`/`y`/`z` options (`position` function).
    position_override: [Option<f64>; 3],
    aabb: Option<AABB>,
    order: Order,
    current_entity: bool,
    player_name: Option<String>,
    entity_uuid: Option<Uuid>,
    /// `None` = `ANY_TYPE`.
    entity_type: Option<EntityType>,
    uses_selector: bool,
}

impl std::fmt::Debug for EntitySelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntitySelector").field("max_results", &self.max_results).field("order", &self.order).finish_non_exhaustive()
    }
}

impl EntitySelector {
    pub const INFINITE: i32 = i32::MAX;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_results: i32,
        includes_entities: bool,
        world_limited: bool,
        context_free_predicates: Vec<EntityPredicate>,
        range: Option<Doubles>,
        position_override: [Option<f64>; 3],
        aabb: Option<AABB>,
        order: Order,
        current_entity: bool,
        player_name: Option<String>,
        entity_uuid: Option<Uuid>,
        entity_type: Option<EntityType>,
        uses_selector: bool,
    ) -> Self {
        Self {
            max_results,
            includes_entities,
            world_limited,
            context_free_predicates,
            range,
            position_override,
            aabb,
            order,
            current_entity,
            player_name,
            entity_uuid,
            entity_type,
            uses_selector,
        }
    }

    pub fn get_max_results(&self) -> i32 {
        self.max_results
    }

    pub fn includes_entities(&self) -> bool {
        self.includes_entities
    }

    pub fn is_self_selector(&self) -> bool {
        self.current_entity
    }

    pub fn is_world_limited(&self) -> bool {
        self.world_limited
    }

    pub fn uses_selector(&self) -> bool {
        self.uses_selector
    }

    fn check_permissions(&self, sender: &CommandSourceStack) -> Result<(), CommandSyntaxException> {
        if self.uses_selector && !sender.permissions().has_permission(&permissions::COMMANDS_ENTITY_SELECTORS) {
            return Err(ERROR_SELECTORS_NOT_ALLOWED.create());
        }
        Ok(())
    }

    fn position(&self, origin: Vec3) -> Vec3 {
        Vec3::new(self.position_override[0].unwrap_or(origin.x), self.position_override[1].unwrap_or(origin.y), self.position_override[2].unwrap_or(origin.z))
    }

    fn get_absolute_aabb(&self, pos: Vec3) -> Option<AABB> {
        self.aabb.map(|bb| bb.move_(pos.x, pos.y, pos.z))
    }

    fn get_predicate(
        &self,
        pos: Vec3,
        absolute_aabb: Option<AABB>,
        enabled_features: Option<mc_world::level_storage::FeatureFlagSet>,
    ) -> impl Fn(&SelectorContext<'_>, &dyn Entity) -> bool + '_ {
        let range = self.range;
        move |ctx, e| {
            self.context_free_predicates.iter().all(|p| p(ctx, e))
                && enabled_features.is_none_or(|f| host::entity_type_enabled(e.get_type(), f))
                && absolute_aabb.as_ref().is_none_or(|bb| bb.intersects_aabb(&e.get_bounding_box()))
                && range.as_ref().is_none_or(|r| r.matches_sqr(e.base().distance_to_sqr_vec(pos)))
        }
    }

    fn sort_and_limit(&self, server: &MinecraftServer, pos: Vec3, mut result: Vec<EntityRef>) -> Vec<EntityRef> {
        if result.len() > 1 {
            let dist = |r: &EntityRef| host::get_entity(server, r).map_or(f64::MAX, |e| e.base().distance_to_sqr_vec(pos));
            match self.order {
                Order::Arbitrary => {}
                Order::Nearest => result.sort_by(|a, b| compare_doubles(dist(a), dist(b))),
                Order::Furthest => result.sort_by(|a, b| compare_doubles(dist(b), dist(a))),
                Order::Random => {
                    // `Collections.shuffle`
                    let mut rng = rand::rng();
                    for i in (2..=result.len()).rev() {
                        result.swap(i - 1, rng.random_range(0..i));
                    }
                }
            }
        }
        result.truncate(self.max_results.max(0) as usize);
        result
    }

    fn get_result_limit(&self) -> usize {
        if self.order == Order::Arbitrary { self.max_results.max(0) as usize } else { usize::MAX }
    }

    /// `findSingleEntity`.
    pub fn find_single_entity(&self, sender: &CommandSourceStack, server: &MinecraftServer) -> Result<EntityRef, CommandSyntaxException> {
        self.check_permissions(sender)?;
        let mut entities = self.find_entities(sender, server)?;
        match entities.len() {
            0 => Err(NO_ENTITIES_FOUND.create()),
            1 => Ok(entities.remove(0)),
            _ => Err(ERROR_NOT_SINGLE_ENTITY.create()),
        }
    }

    /// `findEntities`.
    pub fn find_entities(&self, sender: &CommandSourceStack, server: &MinecraftServer) -> Result<Vec<EntityRef>, CommandSyntaxException> {
        self.check_permissions(sender)?;
        if !self.includes_entities {
            return self.find_players(sender, server);
        }
        if let Some(name) = &self.player_name {
            return Ok(host::player_by_name(server, name).into_iter().collect());
        }
        if let Some(uuid) = self.entity_uuid {
            let features = host::level_enabled_features_ref(server, sender.get_level());
            for dimension in server.level_keys() {
                if let Some(entity) = server.level(&dimension).and_then(|l| l.get_entity_by_uuid(uuid)) {
                    if host::entity_type_enabled(entity.get_type(), features) {
                        return Ok(vec![EntityRef { dimension, id: entity.id() }]);
                    }
                    break;
                }
            }
            return Ok(Vec::new());
        }
        let pos = self.position(sender.get_position());
        let absolute_aabb = self.get_absolute_aabb(pos);
        if self.current_entity {
            let predicate = self.get_predicate(pos, absolute_aabb, None);
            return Ok(sender
                .get_entity()
                .and_then(|me| {
                    let r = EntityRef { dimension: sender.get_level().clone(), id: me.id };
                    let entity = host::get_entity(server, &r)?;
                    let ctx = SelectorContext { server, dimension: sender.get_level() };
                    predicate(&ctx, entity).then_some(r)
                })
                .into_iter()
                .collect());
        }
        let features = host::level_enabled_features_ref(server, sender.get_level());
        let predicate = self.get_predicate(pos, absolute_aabb, Some(features));
        let mut result = Vec::new();
        if self.is_world_limited() {
            self.add_entities(&mut result, server, sender.get_level(), absolute_aabb.as_ref(), &predicate);
        } else {
            for dimension in server.level_keys() {
                self.add_entities(&mut result, server, &dimension, absolute_aabb.as_ref(), &predicate);
            }
        }
        Ok(self.sort_and_limit(server, pos, result))
    }

    fn add_entities(
        &self,
        result: &mut Vec<EntityRef>,
        server: &MinecraftServer,
        dimension: &DimensionKey,
        absolute_aabb: Option<&AABB>,
        predicate: &dyn Fn(&SelectorContext<'_>, &dyn Entity) -> bool,
    ) {
        let limit = self.get_result_limit();
        if result.len() >= limit {
            return;
        }
        let ctx = SelectorContext { server, dimension };
        for id in host::level_entity_ids(server, dimension, absolute_aabb) {
            let Some(entity) = server.level(dimension).and_then(|l| l.get_entity(id)) else { continue };
            if self.entity_type.is_some_and(|t| t != entity.get_type()) {
                continue;
            }
            if predicate(&ctx, entity) {
                result.push(EntityRef { dimension: dimension.clone(), id });
                if result.len() >= limit {
                    return;
                }
            }
        }
    }

    /// `findSinglePlayer`.
    pub fn find_single_player(&self, sender: &CommandSourceStack, server: &MinecraftServer) -> Result<EntityRef, CommandSyntaxException> {
        self.check_permissions(sender)?;
        let mut players = self.find_players(sender, server)?;
        if players.len() != 1 { Err(NO_PLAYERS_FOUND.create()) } else { Ok(players.remove(0)) }
    }

    /// `findPlayers`.
    pub fn find_players(&self, sender: &CommandSourceStack, server: &MinecraftServer) -> Result<Vec<EntityRef>, CommandSyntaxException> {
        self.check_permissions(sender)?;
        if let Some(name) = &self.player_name {
            return Ok(host::player_by_name(server, name).into_iter().collect());
        }
        if let Some(uuid) = self.entity_uuid {
            return Ok(host::player_by_uuid(server, uuid).into_iter().collect());
        }
        let pos = self.position(sender.get_position());
        let absolute_aabb = self.get_absolute_aabb(pos);
        let predicate = self.get_predicate(pos, absolute_aabb, None);
        if self.current_entity {
            return Ok(sender
                .get_player()
                .and_then(|me| {
                    let r = EntityRef { dimension: sender.get_level().clone(), id: me.id };
                    let entity = host::get_entity(server, &r)?;
                    let ctx = SelectorContext { server, dimension: sender.get_level() };
                    (entity.is_player() && predicate(&ctx, entity)).then_some(r)
                })
                .into_iter()
                .collect());
        }
        let limit = self.get_result_limit();
        let mut result = Vec::new();
        let candidates: Vec<EntityRef> = if self.is_world_limited() {
            server.level(sender.get_level()).map(|l| l.players().iter().map(|&id| EntityRef { dimension: sender.get_level().clone(), id }).collect()).unwrap_or_default()
        } else {
            host::server_players(server)
        };
        for candidate in candidates {
            let Some(entity) = host::get_entity(server, &candidate) else { continue };
            let ctx = SelectorContext { server, dimension: &candidate.dimension };
            if predicate(&ctx, entity) {
                result.push(candidate);
                if result.len() >= limit {
                    if self.is_world_limited() {
                        break;
                    }
                    // `PlayerList` path returns without sorting once the limit is hit.
                    return Ok(result);
                }
            }
        }
        Ok(self.sort_and_limit(server, pos, result))
    }

    /// `joinNames(entities)`.
    pub fn join_names(server: &MinecraftServer, entities: &[EntityRef]) -> Component {
        mc_text::util::format_list(entities, &mc_text::util::default_separator(), |r| {
            host::get_entity(server, r).map_or_else(Component::empty, |e| host::entity_display_name(server, e))
        })
    }
}
