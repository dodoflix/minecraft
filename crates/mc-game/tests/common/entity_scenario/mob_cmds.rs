//! Mob state commands of the DSL: `target love age breed tame leash interact` (driver arms of the same names).
//! `mobspawn` (create + `finalizeSpawn` hook + add) lives in the harness core.
use super::*;
use mc_game::world::entity::mob::{ageable, animal, leashable, tamable};
use mc_game::world::item::InteractionHand;

/// Run `f` on entity `a` with entity `b` taken out of the level next to it (so `f` may read `b` while the level is mutably borrowed).
fn with_other<R>(h: &mut Harness, a: &str, b: &str, f: impl FnOnce(&mut ServerLevel, &mut dyn Entity, &mut dyn Entity) -> R) -> R {
    let (ida, idb) = (h.entity_ref(a), h.entity_ref(b));
    let mut other = h.level.take_entity(idb).unwrap_or_else(|| h.fail(&format!("entity {b} is not in the level")));
    let r = h.level.with_entity(ida, |level, e| f(level, e, other.as_mut()));
    h.level.put_back_entity(idb, other);
    r.unwrap_or_else(|| h.fail(&format!("entity {a} is not in the level")))
}

pub(super) fn run(h: &mut Harness, a: &[String]) -> bool {
    match a[0].as_str() {
        // target REF REF2|-            Mob.setTarget
        "target" => {
            h.world();
            let t = (a[2] != "-").then(|| h.entity_ref(&a[2]));
            h.with_entity(&a[1], |level, e| e.set_target(level, t));
        }
        // love REF [PLAYER]            Animal.setInLove(player|null)
        "love" => {
            h.world();
            if let Some(p) = a.get(2) {
                with_other(h, &a[1], p, |level, e, player| animal::set_in_love(e, level, Some(&*player)));
            } else {
                h.with_entity(&a[1], |level, e| animal::set_in_love(e, level, None));
            }
        }
        // age REF N                    AgeableMob.setAge
        "age" => {
            h.world();
            let n = h.int(&a[2]);
            h.with_entity(&a[1], |level, e| ageable::set_age_in_level(e, level, n));
        }
        // breed A B                    A.spawnChildFromBreeding(level, B)
        "breed" => {
            h.world();
            let partner = h.entity_ref(&a[2]);
            h.with_entity(&a[1], |level, e| e.spawn_child_from_breeding(level, partner));
        }
        // tame REF PLAYER              TamableAnimal.tame(player)
        "tame" => {
            h.world();
            with_other(h, &a[1], &a[2], |level, e, player| {
                let player = player.as_any().downcast_ref::<ServerPlayer>().expect("tame needs a player");
                tamable::tame(e, level, player);
            });
        }
        // leash REF HOLDER|-           Leashable.setLeashedTo(holder, true) / removeLeash()
        "leash" => {
            h.world();
            if a[2] == "-" {
                h.with_entity(&a[1], |level, e| leashable::remove_leash(e, level));
            } else {
                with_other(h, &a[1], &a[2], |level, e, holder| leashable::set_leashed_to(e, level, leashable::LeashHolder::of(&*holder), true));
            }
        }
        // interact PLAYER REF [main|off] [sneak]   ServerboundInteractPacket (location = bounding box centre relative to the entity)
        "interact" => {
            h.world();
            let target = h.entity_ref(&a[2]);
            let hand = if a.get(3).is_some_and(|t| t == "off") { InteractionHand::OffHand } else { InteractionHand::MainHand };
            let sneak = a.iter().any(|t| t == "sneak");
            let (loc, bb) = {
                let t = h.level.get_entity(target).unwrap_or_else(|| h.fail("interact target is not in the level"));
                (t.get_bounding_box().get_center().subtract(t.position()), t.get_bounding_box())
            };
            h.with_player(&a[1], |level, p| {
                // handleInteract: setShiftKeyDown(usingSecondaryAction), range check, interactOn, swing
                p.base_mut().set_shared_flag(mc_game::world::entity::entity::FLAG_SHIFT_KEY_DOWN, sneak);
                if !p.is_within_entity_interaction_range(&bb, 3.0) {
                    return;
                }
                if let mc_game::world::item::InteractionResult::Success(s) = p.interact_on(level, target, hand, loc) {
                    if s.should_swing() {
                        let send = s.swing_source != mc_game::world::item::SwingSource::Predicted;
                        if p.swing(level, hand, mc_game::world::entity::player::item_use::SWING_ANIMATION_DEFAULT, send) {
                            p.reset_attack_strength_ticker();
                        }
                    }
                }
            });
        }
        _ => return false,
    }
    true
}
