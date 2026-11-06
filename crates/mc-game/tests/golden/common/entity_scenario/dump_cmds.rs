//! Extra dump kinds (after `dump` / `watch`): `goals:REF brain:REF path:REF attrs:REF nbt:REF`.
//! The formatters live in `mc_game::world::entity::scenario_dump` (shared with the mob spine's registrations).
use super::*;
use mc_game::world::entity::scenario_dump as sd;

pub(super) fn dump(h: &mut Harness, kind: &str, arg: Option<&str>) -> bool {
    let Some(name) = arg else { return false };
    match kind {
        "goals" => {
            let id = h.entity_ref(name);
            let e = h
                .level
                .get_entity(id)
                .unwrap_or_else(|| h.fail(&format!("entity {name} is not in the level")));
            h.out.extend(sd::goal_lines(name, e));
        }
        "brain" => {
            let id = h.entity_ref(name);
            let ctx = ScenarioDumpCtx {
                names: &h.names,
                uuids: &h.uuids,
            };
            let e = h
                .level
                .get_entity(id)
                .unwrap_or_else(|| h.fail(&format!("entity {name} is not in the level")));
            let lines = sd::brain_lines(name, e, &h.level, &ctx);
            h.out.extend(lines);
        }
        "path" => {
            let path = super::nav_cmds::current_path(h, name);
            h.out.extend(sd::path_lines(name, path.as_ref()));
        }
        // ATTR REF id:value ...  every attribute of the type's supplier, sorted, final values
        "attrs" => {
            let id = h.entity_ref(name);
            let e = h
                .level
                .get_entity(id)
                .unwrap_or_else(|| h.fail(&format!("entity {name} is not in the level")));
            let l = e
                .living()
                .unwrap_or_else(|| h.fail("attrs: not a living entity"));
            let mut v: Vec<String> = e
                .get_type()
                .data()
                .default_attributes
                .iter()
                .map(|(a, _)| {
                    format!(
                        "{}:{}",
                        a.name(),
                        mc_core::math::java::java_double_to_string(l.attributes.get_value(*a))
                    )
                })
                .collect();
            v.sort();
            h.out.push(format!(
                "ATTR {name} {}",
                if v.is_empty() {
                    "-".to_string()
                } else {
                    v.join(" ")
                }
            ));
        }
        // N REF {canonical sorted SNBT of saveWithoutId, UUID removed}
        "nbt" => {
            let id = h.entity_ref(name);
            let e = h
                .level
                .get_entity(id)
                .unwrap_or_else(|| h.fail(&format!("entity {name} is not in the level")));
            let mut tag = CompoundTag::new();
            e.save_without_id(&mut tag, &|i| h.level.get_entity(i));
            tag.remove("UUID");
            let line = format!("N {name} {}", canon(&Tag::Compound(tag)));
            h.out.push(line);
        }
        _ => return false,
    }
    true
}
