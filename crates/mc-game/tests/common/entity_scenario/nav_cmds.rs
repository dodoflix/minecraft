//! Navigation commands of the DSL: `nav REF X Y Z [speed] [reach]`, `path_query REF X Y Z [reach]`, `nav_stop REF`.
use super::*;
use mc_game::world::entity::mob::navigation;

/// Path currently held by the mob's navigation.
pub(super) fn current_path(h: &mut Harness, name: &str) -> Option<mc_game::world::level::pathfinder::Path> {
    let id = h.entity_ref(name);
    let e = h.level.get_entity(id)?;
    navigation::with_navigation_ref(e, &h.level, |n| n.data().path.clone()).flatten()
}

pub(super) fn run(h: &mut Harness, a: &[String]) -> bool {
    match a[0].as_str() {
        // nav REF X Y Z [speed] [reach]: moveTo(createPath(x,y,z,reach), speed); prints NAV + the navigation's path
        "nav" => {
            h.world();
            let (x, y, z) = (h.num(&a[2]), h.num(&a[3]), h.num(&a[4]));
            let speed = a.get(5).map_or(1.0, |s| h.num(s));
            let reach = a.get(6).map_or(1, |s| h.int(s));
            let moved = h.with_entity(&a[1], |level, e| {
                let path = navigation::create_path_xyz(e, level, x, y, z, reach);
                navigation::move_to_path(e, level, path, speed)
            });
            h.out.push(format!("NAV {} {} moved={moved}", a[1], vec3(x, y, z)));
            let path = current_path(h, &a[1]);
            h.out.extend(mc_game::world::entity::scenario_dump::path_lines(&a[1], path.as_ref()));
        }
        // path_query REF X Y Z [reach]: createPath without moving
        "path_query" => {
            h.world();
            let (x, y, z) = (h.num(&a[2]), h.num(&a[3]), h.num(&a[4]));
            let reach = a.get(5).map_or(1, |s| h.int(s));
            let path = h.with_entity(&a[1], |level, e| navigation::create_path_xyz(e, level, x, y, z, reach));
            h.out.push(format!("PATHQ {} {} found={}", a[1], vec3(x, y, z), path.is_some()));
            h.out.extend(mc_game::world::entity::scenario_dump::path_lines(&a[1], path.as_ref()));
        }
        "nav_stop" => {
            h.world();
            h.with_entity(&a[1], |level, e| navigation::stop(e, level));
        }
        _ => return false,
    }
    true
}
