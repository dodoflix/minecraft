//! The entity commands (`summon`, `kill`, `damage`, `effect`, `attribute`, `experience`, `ride`, `rotate`, `spectate`, `swing`) against a
//! real `MinecraftServer` with one void overworld that holds two survival players (`Dev`, `Other`): feedback texts (the strings of
//! `en_us.json`), results and the state the commands leave behind. Mobs are loaded as plain entities until mobs are ported, so the
//! living-entity commands run on the players. The dual-server corpus (`commands_corpus/entity_*.txt`) compares against real vanilla.

use crate::command_common;

use mc_core::ident::Identifier;
use mc_data::enums::GameType;
use mc_data::{Attribute, MobEffect};
use mc_game::commands::{CommandSource, CommandSourceStack};
use mc_game::server::MinecraftServer;
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::{Entity, EntityId, ServerPlayer};
use mc_protocol::codec::GameProfile;
use mc_text::Component;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Capture(Mutex<Vec<String>>);

impl CommandSource for Capture {
    fn send_system_message(&self, message: &Component) {
        self.0.lock().unwrap().push(message.get_string());
    }
    fn accepts_success(&self) -> bool {
        true
    }
    fn accepts_failure(&self) -> bool {
        true
    }
    fn should_inform_admins(&self) -> bool {
        false
    }
}

fn run(server: &mut MinecraftServer, command: &str) -> Vec<String> {
    mc_game::commands::BrigadierExceptions::install();
    let capture = Arc::new(Capture::default());
    let source: CommandSourceStack = server
        .create_command_source_stack()
        .with_source(capture.clone());
    let commands = server.commands.clone();
    commands.perform_prefixed_command(server, source, command);
    let lines = capture.0.lock().unwrap().clone();
    lines
}

fn overworld() -> Identifier {
    Identifier::parse("minecraft:overworld").unwrap()
}

fn server_with_players() -> (MinecraftServer, Box<dyn std::any::Any>) {
    let (mut server, guard) = command_common::test_server();
    let mut level = ServerLevel::detached(overworld());
    level.registries = server.registries.composite_access().clone();
    // entities are only visible in loaded chunks (`EntityLookup` of tracked entities)
    for (x, z) in (-2..=2).flat_map(|x| (-2..=2).map(move |z| (x, z))) {
        level.entity_manager.update_chunk_status(
            mc_core::pos::ChunkPos::new(x, z),
            mc_game::world::level::entity::Visibility::Ticking,
        );
    }
    for name in ["Dev", "Other"] {
        let id = level.next_entity_id();
        let profile = GameProfile {
            id: uuid::Uuid::from_u128(u128::from(id.unsigned_abs())),
            name: name.into(),
            properties: vec![],
        };
        let mut player = ServerPlayer::new(id, overworld(), profile, Default::default());
        player.set_game_mode_for_player(GameType::Survival, None);
        player.snap_to(0.5, 64.0, 0.5, 0.0, 0.0);
        level.add_player(Box::new(player));
    }
    server.add_level(level);
    (server, guard)
}

fn ids_of_type(server: &MinecraftServer, entity_type: &str) -> Vec<EntityId> {
    let level = server.level(&overworld()).unwrap();
    mc_game::commands::host::level_entity_ids(server, &overworld(), None)
        .into_iter()
        .filter(|&id| {
            level
                .get_entity(id)
                .is_some_and(|e| e.get_type().name() == entity_type)
        })
        .collect()
}

fn entity(server: &MinecraftServer, id: EntityId) -> &dyn Entity {
    server
        .level(&overworld())
        .unwrap()
        .get_entity(id)
        .expect("entity is in the level")
}

fn player<'a>(server: &'a MinecraftServer, name: &str) -> &'a ServerPlayer {
    let id = ids_of_type(server, "minecraft:player")
        .into_iter()
        .find(|&id| {
            entity(server, id)
                .as_any()
                .downcast_ref::<ServerPlayer>()
                .unwrap()
                .player
                .game_profile
                .name
                == name
        })
        .unwrap();
    entity(server, id)
        .as_any()
        .downcast_ref::<ServerPlayer>()
        .unwrap()
}

#[test]
fn summon_places_entities_and_reports_failures() {
    let (mut server, _guard) = server_with_players();
    assert_eq!(
        run(&mut server, "summon minecraft:cow 1 64 2"),
        ["Summoned new Cow"]
    );
    let cows = ids_of_type(&server, "minecraft:cow");
    assert_eq!(cows.len(), 1);
    let pos = entity(&server, cows[0]).position();
    assert_eq!((pos.x, pos.y, pos.z), (1.5, 64.0, 2.5));

    // custom name, explicit rotation; the position argument wins over `Pos`
    assert_eq!(
        run(
            &mut server,
            "summon minecraft:cow 5 65 6 {CustomName:Bob,Pos:[9.0d,9.0d,9.0d],Rotation:[30.0f,10.0f]}"
        ),
        ["Summoned new Bob"]
    );
    let bob = *ids_of_type(&server, "minecraft:cow")
        .iter()
        .find(|&&id| id != cows[0])
        .unwrap();
    let bob = entity(&server, bob);
    assert_eq!(
        (bob.position().x, bob.position().y, bob.position().z),
        (5.5, 65.0, 6.5)
    );
    assert_eq!((bob.base().y_rot(), bob.base().x_rot()), (30.0, 10.0));

    assert_eq!(
        run(
            &mut server,
            "summon minecraft:cow 0 64 0 {UUID:[I;1,2,3,4]}"
        ),
        ["Summoned new Cow"]
    );
    assert_eq!(
        run(
            &mut server,
            "summon minecraft:cow 0 64 0 {UUID:[I;1,2,3,4]}"
        ),
        ["Unable to summon entity due to duplicate UUIDs"]
    );
    assert_eq!(
        run(&mut server, "summon minecraft:cow 0 30000000 0"),
        ["Invalid position for summon"]
    );
    assert_eq!(
        run(&mut server, "summon minecraft:player"),
        ["Can't summon entity of type minecraft:player"]
    );
    let unknown = run(&mut server, "summon minecraft:nothing");
    assert_eq!(
        unknown[0],
        "Can't find element 'minecraft:nothing' of type 'minecraft:entity_type'"
    );
}

#[test]
fn summon_loads_passengers_recursively() {
    let (mut server, _guard) = server_with_players();
    assert_eq!(
        run(
            &mut server,
            "summon minecraft:cow 3 70 4 {Passengers:[{id:\"minecraft:pig\",Passengers:[{id:\"minecraft:armor_stand\"}]}]}"
        ),
        ["Summoned new Cow"]
    );
    let cow = ids_of_type(&server, "minecraft:cow")[0];
    let pig = ids_of_type(&server, "minecraft:pig")[0];
    let stand = ids_of_type(&server, "minecraft:armor_stand")[0];
    assert_eq!(entity(&server, cow).base().passengers, [pig]);
    assert_eq!(entity(&server, pig).base().passengers, [stand]);
    assert_eq!(entity(&server, pig).base().vehicle, Some(cow));
    for id in [cow, pig, stand] {
        let pos = entity(&server, id).position();
        assert_eq!(
            (pos.x, pos.y, pos.z),
            (3.5, 70.0, 4.5),
            "every entity of the tree is moved to the summon position"
        );
    }
}

#[test]
fn kill_removes_entities_and_kills_players() {
    let (mut server, _guard) = server_with_players();
    run(&mut server, "summon minecraft:cow 1 64 1");
    run(&mut server, "summon minecraft:cow 2 64 2");
    run(&mut server, "summon minecraft:pig 3 64 3");
    assert_eq!(
        run(&mut server, "kill @e[type=minecraft:pig]"),
        ["Killed Pig"]
    );
    assert!(ids_of_type(&server, "minecraft:pig").is_empty());
    assert_eq!(
        run(&mut server, "kill @e[type=minecraft:cow]"),
        ["Killed 2 entities"]
    );
    assert!(ids_of_type(&server, "minecraft:cow").is_empty());
    assert_eq!(
        run(&mut server, "kill @e[type=minecraft:cow]"),
        ["No entity was found"]
    );

    assert_eq!(run(&mut server, "kill Dev"), ["Killed Dev"]);
    assert_eq!(player(&server, "Dev").player.living.get_health(), 0.0);
}

#[test]
fn damage_hurts_living_entities_and_reports_invulnerability() {
    let (mut server, _guard) = server_with_players();
    assert_eq!(
        run(&mut server, "damage Dev 5"),
        ["Applied 5.0 damage to Dev"]
    );
    assert_eq!(player(&server, "Dev").player.living.get_health(), 15.0);
    // the 20 invulnerable ticks after the first hit swallow a weaker one
    assert_eq!(
        run(&mut server, "damage Dev 2.5 minecraft:magic"),
        ["Target is invulnerable to the given damage type"]
    );
    assert_eq!(player(&server, "Dev").player.living.get_health(), 15.0);
    // `at` / `by` / `by ... from` build different sources (growing amounts pass the damage cooldown)
    assert_eq!(
        run(&mut server, "damage Other 1 minecraft:explosion at 0 64 0"),
        ["Applied 1.0 damage to Other"]
    );
    assert_eq!(
        run(&mut server, "damage Other 2 minecraft:player_attack by Dev"),
        ["Applied 2.0 damage to Other"]
    );
    assert_eq!(
        run(
            &mut server,
            "damage Other 3 minecraft:arrow by Dev from Dev"
        ),
        ["Applied 3.0 damage to Other"]
    );
    assert_eq!(
        run(&mut server, "damage Dev -1")[0],
        "Float must not be less than 0.0: found -1.0"
    );
    // non-living entities do not take damage
    run(&mut server, "summon minecraft:cow 1 64 1");
    assert_eq!(
        run(&mut server, "damage @e[type=minecraft:cow,limit=1] 1"),
        ["Target is invulnerable to the given damage type"]
    );
    // a hit that does not beat the last one inside the cooldown fails
    assert_eq!(
        run(&mut server, "damage Other 0 minecraft:generic"),
        ["Target is invulnerable to the given damage type"]
    );
}

#[test]
fn effect_give_and_clear() {
    let (mut server, _guard) = server_with_players();
    assert_eq!(
        run(&mut server, "effect give Dev minecraft:speed 10 1"),
        ["Applied effect Speed to Dev"]
    );
    let speed = player(&server, "Dev")
        .player
        .living
        .active_effects
        .get(&MobEffect::SPEED)
        .cloned()
        .unwrap();
    assert_eq!(
        (
            speed.duration,
            speed.amplifier,
            speed.ambient,
            speed.visible,
            speed.show_icon
        ),
        (200, 1, false, true, true)
    );
    assert_eq!(
        run(&mut server, "effect give Dev minecraft:speed 10 1"),
        [
            "Unable to apply this effect (target is either immune to effects, or has something stronger)"
        ]
    );

    // default duration 30 s, hideParticles, infinite
    assert_eq!(
        run(&mut server, "effect give Other minecraft:speed"),
        ["Applied effect Speed to Other"]
    );
    assert_eq!(
        player(&server, "Other").player.living.active_effects[&MobEffect::SPEED].duration,
        600
    );
    assert_eq!(
        run(&mut server, "effect give Other minecraft:slowness 5 2 true"),
        ["Applied effect Slowness to Other"]
    );
    assert!(!player(&server, "Other").player.living.active_effects[&MobEffect::SLOWNESS].visible);
    assert_eq!(
        run(&mut server, "effect give @a minecraft:glowing infinite"),
        ["Applied effect Glowing to 2 targets"]
    );
    assert_eq!(
        player(&server, "Dev").player.living.active_effects[&MobEffect::GLOWING].duration,
        -1
    );
    // instantaneous effects take their "seconds" as ticks
    assert_eq!(
        run(&mut server, "effect give Dev minecraft:instant_health"),
        ["Applied effect Instant Health to Dev"]
    );
    // the instantaneous effect (duration 1 tick) stays until the entity ticks
    assert_eq!(
        player(&server, "Dev").player.living.active_effects[&MobEffect::INSTANT_HEALTH].duration,
        1
    );

    assert_eq!(
        run(&mut server, "effect give Dev minecraft:speed 1 256")[0],
        "Integer must not be more than 255: found 256"
    );
    assert_eq!(
        run(&mut server, "effect give Dev minecraft:speed 0")[0],
        "Integer must not be less than 1: found 0"
    );
    // non-living targets are skipped, a command that applied nothing fails
    run(&mut server, "summon minecraft:cow 1 64 1");
    assert_eq!(
        run(
            &mut server,
            "effect give @e[type=minecraft:cow] minecraft:speed"
        ),
        [
            "Unable to apply this effect (target is either immune to effects, or has something stronger)"
        ]
    );

    assert_eq!(
        run(&mut server, "effect clear Dev minecraft:speed"),
        ["Removed effect Speed from Dev"]
    );
    assert_eq!(
        run(&mut server, "effect clear Dev minecraft:speed"),
        ["Target doesn't have the requested effect"]
    );
    assert_eq!(
        run(&mut server, "effect clear @a"),
        ["Removed every effect from 2 targets"]
    );
    assert_eq!(
        run(&mut server, "effect clear @a"),
        ["Target has no effects to remove"]
    );
}

#[test]
fn attribute_get_set_and_modifiers() {
    let (mut server, _guard) = server_with_players();
    assert_eq!(
        run(&mut server, "attribute Dev minecraft:max_health get"),
        ["The value of attribute Max Health for entity Dev is 20.0"]
    );
    assert_eq!(
        run(&mut server, "attribute Dev minecraft:max_health base get"),
        ["The base value of attribute Max Health for entity Dev is 20.0"]
    );
    assert_eq!(
        run(
            &mut server,
            "attribute Dev minecraft:max_health base set 30"
        ),
        ["The base value for attribute Max Health for entity Dev set to 30.0"]
    );
    assert_eq!(
        player(&server, "Dev")
            .player
            .living
            .attributes
            .get_base_value(Attribute::MAX_HEALTH),
        30.0
    );
    assert_eq!(
        run(&mut server, "attribute Dev minecraft:max_health base reset"),
        ["The base value for attribute Max Health for entity Dev reset to default 20.0"]
    );

    assert_eq!(
        run(
            &mut server,
            "attribute Dev minecraft:armor modifier add test:a 2 add_value"
        ),
        ["Added modifier test:a to attribute Armor for entity Dev"]
    );
    assert_eq!(
        run(
            &mut server,
            "attribute Dev minecraft:armor modifier add test:a 2 add_value"
        ),
        ["Modifier test:a is already present on attribute Armor for entity Dev"]
    );
    assert_eq!(
        run(
            &mut server,
            "attribute Dev minecraft:armor modifier add test:b 0.5 add_multiplied_total"
        ),
        ["Added modifier test:b to attribute Armor for entity Dev"]
    );
    assert_eq!(
        run(&mut server, "attribute Dev minecraft:armor get"),
        ["The value of attribute Armor for entity Dev is 3.0"]
    );
    assert_eq!(
        run(
            &mut server,
            "attribute Dev minecraft:armor modifier value get test:b"
        ),
        ["The value of modifier test:b on attribute Armor for entity Dev is 0.5"]
    );
    assert_eq!(
        run(
            &mut server,
            "attribute Dev minecraft:armor modifier remove test:a"
        ),
        ["Removed modifier test:a from attribute Armor for entity Dev"]
    );
    assert_eq!(
        run(
            &mut server,
            "attribute Dev minecraft:armor modifier remove test:a"
        ),
        ["Attribute Armor for entity Dev has no modifier test:a"]
    );
    assert_eq!(
        run(
            &mut server,
            "attribute Dev minecraft:armor modifier value get test:a"
        ),
        ["Attribute Armor for entity Dev has no modifier test:a"]
    );

    assert_eq!(
        run(&mut server, "attribute Dev minecraft:flying_speed get"),
        ["Entity Dev has no attribute Flying Speed"]
    );
    assert_eq!(
        run(
            &mut server,
            "attribute Dev minecraft:flying_speed base reset"
        ),
        ["Entity Dev has no attribute Flying Speed"]
    );
    run(&mut server, "summon minecraft:cow 1 64 1");
    assert_eq!(
        run(
            &mut server,
            "attribute @e[type=minecraft:cow,limit=1] minecraft:max_health get"
        ),
        ["Cow is not a valid entity for this command"]
    );
    assert_eq!(
        run(
            &mut server,
            "attribute @e[type=minecraft:cow,limit=1] minecraft:max_health base set 3"
        ),
        ["Cow is not a valid entity for this command"]
    );
}

#[test]
fn experience_add_set_query() {
    let (mut server, _guard) = server_with_players();
    assert_eq!(
        run(&mut server, "xp add Dev 10"),
        ["Gave 10 experience points to Dev"]
    );
    assert_eq!(
        run(&mut server, "xp query Dev points"),
        ["Dev has 3 experience points"]
    );
    assert_eq!(player(&server, "Dev").player.experience_level, 1);
    assert_eq!(
        run(&mut server, "experience add Dev 3 levels"),
        ["Gave 3 experience levels to Dev"]
    );
    assert_eq!(
        run(&mut server, "xp query Dev levels"),
        ["Dev has 4 experience levels"]
    );
    assert_eq!(
        run(&mut server, "xp add @a -100 levels"),
        ["Gave -100 experience levels to 2 players"]
    );
    assert_eq!(player(&server, "Dev").player.experience_level, 0);
    assert_eq!(
        run(&mut server, "xp set Dev 5 levels"),
        ["Set 5 experience levels on Dev"]
    );
    assert_eq!(
        run(&mut server, "xp set Dev 17 points"),
        ["Cannot set experience points above the maximum points for the player's current level"]
    );
    assert_eq!(
        run(&mut server, "xp set Dev 3 points"),
        ["Set 3 experience points on Dev"]
    );
    assert_eq!(
        run(&mut server, "xp set @a 0 levels"),
        ["Set 0 experience levels on 2 players"]
    );
    assert_eq!(
        run(&mut server, "xp set Dev -1 levels")[0],
        "Integer must not be less than 0: found -1"
    );
    assert_eq!(
        run(&mut server, "xp query @a points")[0],
        "Only one player is allowed, but the provided selector allows more than one"
    );
}

#[test]
fn ride_mounts_and_dismounts() {
    let (mut server, _guard) = server_with_players();
    run(&mut server, "summon minecraft:cow 1 64 1");
    run(&mut server, "summon minecraft:pig 3 64 3");
    let (cow, pig) = (
        ids_of_type(&server, "minecraft:cow")[0],
        ids_of_type(&server, "minecraft:pig")[0],
    );
    assert_eq!(
        run(
            &mut server,
            "ride @e[type=minecraft:pig,limit=1] mount @e[type=minecraft:cow,limit=1]"
        ),
        ["Pig started riding Cow"]
    );
    assert_eq!(entity(&server, pig).base().vehicle, Some(cow));
    assert_eq!(entity(&server, cow).base().passengers, [pig]);
    assert_eq!(
        run(
            &mut server,
            "ride @e[type=minecraft:pig,limit=1] mount @e[type=minecraft:cow,limit=1]"
        ),
        ["Pig is already riding Cow"]
    );
    assert_eq!(
        run(
            &mut server,
            "ride @e[type=minecraft:cow,limit=1] mount @e[type=minecraft:pig,limit=1]"
        ),
        ["Can't mount entity on itself or any of its passengers"]
    );
    assert_eq!(
        run(
            &mut server,
            "ride @e[type=minecraft:cow,limit=1] mount @e[type=minecraft:cow,limit=1]"
        ),
        ["Can't mount entity on itself or any of its passengers"]
    );
    assert_eq!(
        run(&mut server, "ride @e[type=minecraft:cow,limit=1] mount Dev"),
        ["Players can't be ridden"]
    );
    assert_eq!(
        run(&mut server, "ride @e[type=minecraft:pig,limit=1] dismount"),
        ["Pig stopped riding Cow"]
    );
    assert_eq!(entity(&server, pig).base().vehicle, None);
    assert!(entity(&server, cow).base().passengers.is_empty());
    assert_eq!(
        run(&mut server, "ride @e[type=minecraft:pig,limit=1] dismount"),
        ["Pig is not riding any vehicle"]
    );
}

#[test]
fn rotate_sets_and_faces() {
    let (mut server, _guard) = server_with_players();
    run(&mut server, "summon minecraft:cow 0 64 0");
    let cow = ids_of_type(&server, "minecraft:cow")[0];
    assert_eq!(
        run(&mut server, "rotate @e[type=minecraft:cow,limit=1] 90 10"),
        ["Rotated Cow"]
    );
    assert_eq!(
        (
            entity(&server, cow).base().y_rot(),
            entity(&server, cow).base().x_rot()
        ),
        (90.0, 10.0)
    );
    assert_eq!(
        run(
            &mut server,
            "rotate @e[type=minecraft:cow,limit=1] ~10 ~-20"
        ),
        ["Rotated Cow"]
    );
    // `~` is relative to the source's rotation (0, 0), not to the entity's
    assert_eq!(
        (
            entity(&server, cow).base().y_rot(),
            entity(&server, cow).base().x_rot()
        ),
        (10.0, -20.0)
    );
    assert_eq!(
        run(
            &mut server,
            "rotate @e[type=minecraft:cow,limit=1] facing 0 64 10"
        ),
        ["Rotated Cow"]
    );
    assert_eq!(
        (
            entity(&server, cow).base().y_rot(),
            entity(&server, cow).base().x_rot()
        ),
        (0.0, 0.0)
    );
    assert_eq!(
        run(
            &mut server,
            "rotate @e[type=minecraft:cow,limit=1] facing entity Dev eyes"
        ),
        ["Rotated Cow"]
    );
    // the cow stands at x/z .5 like Dev: straight down, facing is -90 (atan2(0, 0) - 90)
    assert_eq!(entity(&server, cow).base().y_rot(), -90.0);
    assert!(entity(&server, cow).base().x_rot() < 0.0);
    assert_eq!(
        run(&mut server, "rotate @e[type=minecraft:cow,limit=1] 0 91")[0],
        "Rotated Cow"
    );
    assert_eq!(entity(&server, cow).base().x_rot(), 90.0);
}

#[test]
fn spectate_validates_before_touching_the_camera() {
    let (mut server, _guard) = server_with_players();
    run(&mut server, "summon minecraft:cow 1 64 1");
    assert_eq!(
        run(&mut server, "spectate @e[type=minecraft:cow,limit=1] Dev"),
        ["Dev is not in spectator mode"]
    );
    assert_eq!(
        run(&mut server, "spectate Dev Dev"),
        ["Cannot spectate yourself"]
    );
    assert_eq!(
        run(&mut server, "spectate"),
        ["A player is required to run this command here"]
    );
    assert_eq!(
        run(&mut server, "spectate Dev @a")[0],
        "Only one player is allowed, but the provided selector allows more than one"
    );
}

#[test]
fn swing_starts_the_swing_of_living_entities() {
    let (mut server, _guard) = server_with_players();
    assert_eq!(run(&mut server, "swing Dev"), ["Made Dev swing an arm"]);
    assert!(
        player(&server, "Dev")
            .player
            .living
            .swing_state
            .is_swinging()
    );
    assert_eq!(
        run(&mut server, "swing @a offhand stab 10"),
        ["Made 2 entities swing their arms"]
    );
    run(&mut server, "summon minecraft:cow 1 64 1");
    assert_eq!(
        run(&mut server, "swing @e[type=minecraft:cow]"),
        ["No living entities were found to swing"]
    );
    assert_eq!(
        run(&mut server, "swing Other mainhand whack 0")[0],
        "The tick count must not be less than 1: found 0"
    );
}
