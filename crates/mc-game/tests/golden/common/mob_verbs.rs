//! Mob verbs and dumps of the scenario DSL, replayed the same way by the entity replay (`entity_scenario.rs`) and the block replay
//! (`block_scenario_actors.rs`): `target love age breed tame leash mob_attack nav path_query nav_stop` (+ the `goal`/`brain`/`path`
//! shorthands of the Java driver) and the dump kinds `goals:REF brain:REF path:REF attrs:REF nbt:REF` (the grammar of the driver,
//! `tests/oracle/EntityOracleDriver.java`; the `E` line of `mob:REF`/`mobs` is `entity_describe.rs`). A replay implements [`MobHost`]
//! (its [`ActionHost`] plus the name tables of the dump context) and forwards unknown commands to [`command`] and unknown dump kinds
//! to [`dump`]; a verb needs one new arm here, not one per replay. Formatters live in `mc_game::world::entity::scenario_dump`.
use super::entity_scenario::{canon, vec3};
use super::player_actions::ActionHost;
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::mob::{ageable, animal, leashable, navigation, tamable};
use mc_game::world::entity::scenario_dump::{self as sd, ScenarioDumpCtx};
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_nbt::{CompoundTag, Tag};
use std::collections::BTreeMap;

/// What the mob verbs need from a replay next to the player actions.
pub trait MobHost: ActionHost {
    /// A failed verb: the replay adds its scenario position.
    fn fail(&self, msg: &str) -> !;

    /// The scenario names of the entities and the UUID table of the dump context (the driver's `refs` / `uuidName`).
    fn dump_names(&self) -> (BTreeMap<EntityId, String>, BTreeMap<uuid::Uuid, EntityId>);
}

fn num<H: MobHost>(h: &H, s: &str) -> f64 {
    s.parse()
        .unwrap_or_else(|_| h.fail(&format!("not a number: {s}")))
}

fn int<H: MobHost>(h: &H, s: &str) -> i32 {
    s.parse()
        .unwrap_or_else(|_| h.fail(&format!("not an integer: {s}")))
}

/// Run `f` on entity `a`.
fn with_entity<H: MobHost, R>(
    h: &mut H,
    a: &str,
    f: impl FnOnce(&mut ServerLevel, &mut dyn Entity) -> R,
) -> R {
    let id = h.action_ref(a);
    match h.action_level().with_entity(id, f) {
        Some(r) => r,
        None => h.fail(&format!("entity {a} is not in the level")),
    }
}

/// Run `f` on entity `a` with entity `b` taken out of the level next to it (so `f` may read `b` while the level is mutably borrowed).
fn with_other<H: MobHost, R>(
    h: &mut H,
    a: &str,
    b: &str,
    f: impl FnOnce(&mut ServerLevel, &mut dyn Entity, &mut dyn Entity) -> R,
) -> R {
    let (ida, idb) = (h.action_ref(a), h.action_ref(b));
    let Some(mut other) = h.action_level().take_entity(idb) else {
        h.fail(&format!("entity {b} is not in the level"))
    };
    let r = h
        .action_level()
        .with_entity(ida, |level, e| f(level, e, other.as_mut()));
    h.action_level().put_back_entity(idb, other);
    r.unwrap_or_else(|| h.fail(&format!("entity {a} is not in the level")))
}

/// Path currently held by the mob's navigation.
fn current_path<H: MobHost>(
    h: &mut H,
    name: &str,
) -> Option<mc_game::world::level::pathfinder::Path> {
    let id = h.action_ref(name);
    let level = h.action_level();
    let e = level.get_entity_or_removed(id)?;
    navigation::with_navigation_ref(e, level, |n| n.data().path.clone()).flatten()
}

fn path_lines<H: MobHost>(
    h: &mut H,
    name: &str,
    path: Option<&mc_game::world::level::pathfinder::Path>,
) {
    for line in sd::path_lines(name, path) {
        h.emit(line);
    }
}

/// Run a mob verb; `false` when `a[0]` is not one of them.
pub fn command<H: MobHost>(h: &mut H, a: &[String]) -> bool {
    match a[0].as_str() {
        // target REF REF2|-            Mob.setTarget
        "target" => {
            h.boot();
            let t = (a[2] != "-").then(|| h.action_ref(&a[2]));
            with_entity(h, &a[1], |level, e| e.set_target(level, t));
        }
        // love REF [PLAYER]            Animal.setInLove(player|null)
        "love" => {
            h.boot();
            if let Some(p) = a.get(2) {
                with_other(h, &a[1], p, |level, e, player| {
                    animal::set_in_love(e, level, Some(&*player))
                });
            } else {
                with_entity(h, &a[1], |level, e| animal::set_in_love(e, level, None));
            }
        }
        // age REF N                    AgeableMob.setAge
        "age" => {
            h.boot();
            let n = int(h, &a[2]);
            with_entity(h, &a[1], |level, e| ageable::set_age_in_level(e, level, n));
        }
        // breed A B                    A.spawnChildFromBreeding(level, B)
        "breed" => {
            h.boot();
            let partner = h.action_ref(&a[2]);
            with_entity(h, &a[1], |level, e| {
                e.spawn_child_from_breeding(level, partner)
            });
        }
        // tame REF PLAYER              TamableAnimal.tame(player) / AbstractHorse.tameWithName(player)
        "tame" => {
            h.boot();
            let player_id = h.action_ref(&a[2]);
            let horse = with_entity(h, &a[1], |level, e| {
                e.as_abstract_horse_mut()
                    .map(|horse| horse.tame_with_name(level, player_id))
                    .is_some()
            });
            if !horse {
                with_other(h, &a[1], &a[2], |level, e, player| {
                    let player = player
                        .as_any_mut()
                        .downcast_mut::<ServerPlayer>()
                        .expect("tame needs a player");
                    tamable::tame(e, level, player);
                });
            }
        }
        // leash REF HOLDER|-           Leashable.setLeashedTo(holder, true) / removeLeash()
        "leash" => {
            h.boot();
            if a[2] == "-" {
                with_entity(h, &a[1], |level, e| leashable::remove_leash(e, level));
            } else {
                with_other(h, &a[1], &a[2], |level, e, holder| {
                    leashable::set_leashed_to(e, level, leashable::LeashHolder::of(&*holder), true)
                });
            }
        }
        // mob_attack MOB REF           Mob.doHurtTarget(level, REF); prints `MOBATTACK mob ref -> result`
        "mob_attack" => {
            h.boot();
            let result = with_other(h, &a[1], &a[2], |level, mob, target| {
                mob.do_hurt_target(level, target)
            });
            h.emit(format!("MOBATTACK {} {} -> {result}", a[1], a[2]));
        }
        // nav REF X Y Z [speed] [reach]: moveTo(createPath(x,y,z,reach), speed); prints NAV + the navigation's path
        "nav" => {
            h.boot();
            let (x, y, z) = (num(h, &a[2]), num(h, &a[3]), num(h, &a[4]));
            let speed = a.get(5).map_or(1.0, |s| num(h, s));
            let reach = a.get(6).map_or(1, |s| int(h, s));
            let moved = with_entity(h, &a[1], |level, e| {
                let path = navigation::create_path_xyz(e, level, x, y, z, reach);
                navigation::move_to_path(e, level, path, speed)
            });
            h.emit(format!("NAV {} {} moved={moved}", a[1], vec3(x, y, z)));
            let path = current_path(h, &a[1]);
            path_lines(h, &a[1], path.as_ref());
        }
        // path_query REF X Y Z [reach]: createPath without moving
        "path_query" => {
            h.boot();
            let (x, y, z) = (num(h, &a[2]), num(h, &a[3]), num(h, &a[4]));
            let reach = a.get(5).map_or(1, |s| int(h, s));
            let path = with_entity(h, &a[1], |level, e| {
                navigation::create_path_xyz(e, level, x, y, z, reach)
            });
            h.emit(format!(
                "PATHQ {} {} found={}",
                a[1],
                vec3(x, y, z),
                path.is_some()
            ));
            path_lines(h, &a[1], path.as_ref());
        }
        // nav_stop REF                 navigation.stop()
        "nav_stop" => {
            h.boot();
            with_entity(h, &a[1], |level, e| navigation::stop(e, level));
        }
        // goal|brain|path REF          = dump goals:REF | brain:REF | path:REF
        "goal" | "brain" | "path" => {
            h.boot();
            let kind = if a[0] == "goal" {
                "goals"
            } else {
                a[0].as_str()
            };
            dump(h, kind, Some(&a[1]));
        }
        _ => return false,
    }
    true
}

/// The mob dump kinds `goals:REF brain:REF path:REF attrs:REF size:REF nbt:REF`; `false` for any other kind.
pub fn dump<H: MobHost>(h: &mut H, kind: &str, arg: Option<&str>) -> bool {
    let Some(name) = arg else { return false };
    if !matches!(
        kind,
        "goals" | "brain" | "path" | "attrs" | "attrorder" | "size" | "nbt"
    ) {
        return false;
    }
    let id = h.action_ref(name);
    // only `brain:` prints references, so only it needs the name tables
    let (names, uuids) = if kind == "brain" {
        h.dump_names()
    } else {
        Default::default()
    };
    let path = (kind == "path").then(|| current_path(h, name));
    let lines: Result<Vec<String>, String> = {
        let level = h.action_level();
        let lines_of = |e: &dyn Entity| -> Result<Vec<String>, String> {
            match kind {
                "goals" => Ok(sd::goal_lines(name, e)),
                "brain" => {
                    let ctx = ScenarioDumpCtx {
                        names: &names,
                        uuids: &uuids,
                    };
                    Ok(sd::brain_lines(name, e, level, &ctx))
                }
                "path" => Ok(sd::path_lines(name, path.flatten().as_ref())),
                // ATTR REF id:value ...  every attribute of the type's supplier, sorted, final values
                "attrs" => match e.living() {
                    None => Err(format!("entity {name} is not a living entity")),
                    Some(l) => {
                        let mut v: Vec<String> = e
                            .get_type()
                            .data()
                            .default_attributes
                            .iter()
                            .map(|(a, _)| {
                                format!(
                                    "{}:{}",
                                    a.name(),
                                    mc_core::math::java::java_double_to_string(
                                        l.attributes.get_value(*a)
                                    )
                                )
                            })
                            .collect();
                        v.sort();
                        let value = if v.is_empty() {
                            "-".to_string()
                        } else {
                            v.join(" ")
                        };
                        Ok(vec![format!("ATTR {name} {value}")])
                    }
                },
                // ATTRORDER REF id ..  the instances of the attribute map in its hash map's iteration order
                "attrorder" => match e.living() {
                    None => Err(format!("entity {name} is not a living entity")),
                    Some(l) => Ok(vec![format!(
                        "ATTRORDER {name}{}",
                        l.attributes
                            .instance_order()
                            .iter()
                            .map(|a| format!(" {}", a.identifier()))
                            .collect::<String>()
                    )]),
                },
                // SIZE REF scale=getScale() size=width,height,eye bb=min,max corners
                "size" => {
                    use mc_core::math::java::{
                        java_double_to_string as d, java_float_to_string as f,
                    };
                    let b = e.base();
                    let bb = b.bounding_box();
                    Ok(vec![format!(
                        "SIZE {name} scale={} size={},{},{} bb={},{},{},{},{},{}",
                        f(e.get_scale()),
                        f(b.get_bb_width()),
                        f(b.get_bb_height()),
                        f(b.eye_height),
                        d(bb.min_x),
                        d(bb.min_y),
                        d(bb.min_z),
                        d(bb.max_x),
                        d(bb.max_y),
                        d(bb.max_z)
                    )])
                }
                // N REF {canonical sorted SNBT of saveWithoutId, UUID removed}
                _ => {
                    let mut tag = CompoundTag::new();
                    // the pearls of a player may be in another level of the server
                    if mc_game::commands::server_access::has_server() {
                        mc_game::commands::server_access::with_server(|server| {
                            e.save_without_id(&mut tag, &|i| {
                                level
                                    .get_entity(i)
                                    .or_else(|| server.all_levels().find_map(|l| l.get_entity(i)))
                            })
                        });
                    } else {
                        e.save_without_id(&mut tag, &|i| level.get_entity(i));
                    }
                    tag.remove("UUID");
                    sort_recipe_book(&mut tag);
                    Ok(vec![format!("N {name} {}", canon(&Tag::Compound(tag)))])
                }
            }
        };
        let mut lines_of = Some(lines_of);
        // a player the end credits removed from its level lives on in the server (the level keeps a removed stand-in)
        let detached = mc_game::commands::server_access::has_server().then(|| {
            mc_game::commands::server_access::with_server(|server| {
                server
                    .detached_players
                    .values()
                    .find(|p| p.base_id() == id)
                    .map(|p| (lines_of.take().expect("once"))(&**p as &dyn Entity))
            })
        });
        match detached.flatten() {
            Some(lines) => lines,
            None => match level.get_entity_or_removed(id) {
                None => Err(format!("entity {name} is not in the level")),
                Some(e) => (lines_of.take().expect("once"))(e),
            },
        }
    };
    match lines {
        Ok(lines) => lines.into_iter().for_each(|line| h.emit(line)),
        Err(why) => h.fail(&format!("{kind}:{name}: {why}")),
    }
    true
}

/// `ServerRecipeBook.known` is an identity hash set in vanilla: its order differs from run to run, so both sides dump the player's
/// recipes sorted.
pub fn sort_recipe_book(tag: &mut CompoundTag) {
    let Some(Tag::Compound(book)) = tag.get_mut("recipeBook") else {
        return;
    };
    for key in ["recipes", "toBeDisplayed"] {
        if let Some(Tag::List(list)) = book.get_mut(key) {
            let mut items: Vec<Tag> = list.iter().cloned().collect();
            items.sort_by_key(canon);
            let mut sorted = mc_nbt::ListTag::new();
            items.into_iter().for_each(|item| sorted.push(item));
            *list = sorted;
        }
    }
}
