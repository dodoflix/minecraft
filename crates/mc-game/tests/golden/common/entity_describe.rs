//! The `P`/`E` lines of the driver's `describe(name, entity, rich)` and the `ED` line of `dump edata:REF`
//! (`tests/oracle/EntityOracleDriver.java`), one implementation for the entity replay and the block replay.
//!
//! The base line holds the fields of the entity classes the driver prints for every dump (player, item, orb, TNT, arrows, bolt,
//! falling block, boats). A rich dump (`mob:REF`, `mobs`, `xentity:REF`, `xentities`) appends the fields of every
//! `SCENARIO_DUMP_EXTRA` registration that applies to the entity (`EntityExtras.java` is the Java twin of that registry).
use super::block_scenario::state_string;
use super::entity_scenario::{abstract_arrow, item_full, item_text};
use mc_core::math::java::{java_double_to_string as jd, java_float_to_string as jf};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpCtx};
use mc_game::world::entity::vehicle::boat::abstract_boat::{AbstractBoat, Status};
use mc_game::world::entity::vehicle::vehicle_entity;
use mc_game::world::entity::{Entity, ServerPlayer};

fn vec3(x: f64, y: f64, z: f64) -> String {
    format!("{},{},{}", jd(x), jd(y), jd(z))
}

fn snake(s: &str) -> String {
    s.chars().fold(String::new(), |mut r, c| {
        if c.is_uppercase() && !r.is_empty() {
            r.push('_');
        }
        r.push(c.to_ascii_lowercase());
        r
    })
}

/// `describe(name, e, rich)`; `ctx` resolves entity references and UUIDs to scenario names.
pub fn describe(
    name: &str,
    e: &dyn Entity,
    level: &ServerLevel,
    ctx: &ScenarioDumpCtx<'_>,
    rich: bool,
) -> String {
    let b = e.base();
    let is_player = e.as_any().downcast_ref::<ServerPlayer>().is_some();
    let (p, d) = (b.position(), b.delta_movement());
    let mut s = format!(
        "{} {name} {} pos={} delta={} rot={},{} og={} fall={} fire={} air={} alive={} pose={}",
        if is_player { "P" } else { "E" },
        e.get_type().name(),
        vec3(p.x, p.y, p.z),
        vec3(d.x, d.y, d.z),
        jf(b.y_rot()),
        jf(b.x_rot()),
        u8::from(b.on_ground()),
        jd(b.fall_distance),
        b.remaining_fire_ticks,
        b.get_air_supply(),
        u8::from(e.is_alive()),
        snake(&format!("{:?}", b.get_pose())),
    );
    if let Some(l) = e.living() {
        s.push_str(&format!(
            " hp={} abs={} hurt={}/{} death={} fx=",
            jf(e.get_health()),
            jf(e.get_absorption_amount()),
            l.hurt_time,
            l.damage_cooldown_time,
            l.death_time
        ));
        let mut fx: Vec<_> = l.active_effects.values().collect();
        fx.sort_by_key(|i| i.effect.name());
        if fx.is_empty() {
            s.push('-');
        }
        for (i, m) in fx.iter().enumerate() {
            s.push_str(&format!(
                "{}{}/{}/{}",
                if i > 0 { ";" } else { "" },
                m.effect.name(),
                m.amplifier,
                m.duration
            ));
        }
    }
    let any = e.as_any();
    if let Some(sp) = any.downcast_ref::<ServerPlayer>() {
        let pd = &sp.player;
        s.push_str(&format!(
            " gm={} food={} sat={} exh={} xp={}/{}/{} sel={} main={}",
            sp.game_mode.game_mode_for_player.name().to_lowercase(),
            pd.food_data.get_food_level(),
            jf(pd.food_data.get_saturation_level()),
            jf(pd.food_data.exhaustion_level),
            pd.experience_level,
            pd.total_experience,
            jf(pd.experience_progress),
            pd.inventory.get_selected_slot(),
            item_full(pd.inventory.get_selected_item()),
        ));
    } else if let Some(ie) = any.downcast_ref::<mc_game::world::entity::item::ItemEntity>() {
        s.push_str(&format!(
            " item={} age={}",
            item_text(&ie.get_item()),
            ie.age
        ));
    } else if let Some(o) =
        any.downcast_ref::<mc_game::world::entity::experience_orb::ExperienceOrb>()
    {
        s.push_str(&format!(" value={}", o.get_value()));
    } else if let Some(t) =
        any.downcast_ref::<mc_game::world::entity::item::primed_tnt::PrimedTnt>()
    {
        s.push_str(&format!(" fuse={}", t.get_fuse()));
    } else if let Some(a) = abstract_arrow(e) {
        let d = a.arrow_data();
        s.push_str(&format!(
            " ground={} crit={} pierce={} pickup={} shake={} life={} dmg={}",
            u8::from(a.is_in_ground()),
            u8::from(a.is_crit_arrow()),
            a.pierce_level(),
            d.pickup.ordinal(),
            d.shake_time,
            d.life,
            jd(d.base_damage)
        ));
    } else if let Some(lb) =
        any.downcast_ref::<mc_game::world::entity::lightning_bolt::LightningBolt>()
    {
        s.push_str(&format!(
            " life={} flashes={} seed={} fires={}",
            lb.life(),
            lb.flashes(),
            lb.seed,
            lb.get_blocks_set_on_fire()
        ));
    }
    if let Some(ie) = any.downcast_ref::<mc_game::world::entity::item::ItemEntity>() {
        s.push_str(&format!(
            " pickup={} health={} owner={} thrower={}",
            ie.pickup_delay,
            ie.health,
            ctx.name_uuid(ie.target),
            ctx.name_uuid(ie.thrower)
        ));
    } else if let Some(o) =
        any.downcast_ref::<mc_game::world::entity::experience_orb::ExperienceOrb>()
    {
        s.push_str(&format!(
            " count={} age={} health={}",
            o.count, o.age, o.health
        ));
    } else if let Some(f) =
        any.downcast_ref::<mc_game::world::entity::falling_block_entity::FallingBlockEntity>()
    {
        s.push_str(&format!(
            " state={} time={} hurt={} cancel={} drop={}",
            state_string(f.get_block_state()),
            f.time,
            f.hurts_entities(),
            f.is_drop_cancelled(),
            f.drop_item
        ));
    }
    if let Some(boat) = any.downcast_ref::<AbstractBoat>() {
        s.push_str(&format!(
            " status={} dmg={} hurt={}/{} bubble={}/{} paddle={}{} pass=",
            match boat.get_status_value() {
                None => "-",
                Some(Status::InWater) => "in_water",
                Some(Status::UnderWater) => "under_water",
                Some(Status::UnderFlowingWater) => "under_flowing_water",
                Some(Status::OnLand) => "on_land",
                Some(Status::InAir) => "in_air",
            },
            jf(vehicle_entity::get_damage(b)),
            vehicle_entity::get_hurt_time(b),
            vehicle_entity::get_hurt_dir(b),
            boat.is_above_bubble_column(),
            boat.get_bubble_time(),
            u8::from(boat.get_paddle_state(0)),
            u8::from(boat.get_paddle_state(1)),
        ));
        if b.passengers.is_empty() {
            s.push('-');
        }
        for (i, p) in b.passengers.iter().enumerate() {
            let uuid = level.get_entity(*p).map(|p| p.uuid());
            s.push_str(&format!(
                "{}{}",
                if i > 0 { ";" } else { "" },
                ctx.name_uuid(uuid)
            ));
        }
        if let Some(chest) = e.chest_vehicle() {
            use mc_game::world::container::Container;
            s.push_str(" items=");
            let mut n = 0;
            for i in 0..chest.get_container_size() {
                if !chest.get_item(i).is_empty() {
                    s.push_str(&format!(
                        "{}{i}:{}",
                        if n > 0 { ";" } else { "" },
                        item_text(chest.get_item(i))
                    ));
                    n += 1;
                }
            }
            if n == 0 {
                s.push('-');
            }
        }
    }
    if rich {
        let mut extras: Vec<_> = SCENARIO_DUMP_EXTRA
            .iter()
            .filter(|x| (x.applies)(e))
            .collect();
        extras.sort_by_key(|x| x.order);
        for x in extras {
            for (k, v) in (x.fields)(e, level, ctx) {
                s.push_str(&format!(" {k}={v}"));
            }
        }
    }
    s
}

/// `ED REF id:serializer:hex ..` of `dump edata:REF`: every synched data item (accessor id, serializer id, wire form of the value).
pub fn entity_data_line(name: &str, e: &dyn Entity) -> String {
    use mc_protocol::codec::Encode;
    let mut s = format!("ED {name}");
    for item in e.base().entity_data.pack_all() {
        let mut wire = Vec::new();
        item.encode(&mut wire).expect("entity data encodes");
        // the accessor id and the serializer id (one byte: there are fewer than 128 serializers) come first
        s.push_str(&format!(" {}:{}:", wire[0], wire[1] as i8));
        if wire.len() == 2 {
            s.push('-');
        }
        for byte in &wire[2..] {
            s.push_str(&format!("{byte:02x}"));
        }
    }
    s
}
