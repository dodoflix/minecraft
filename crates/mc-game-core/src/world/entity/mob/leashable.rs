//! `Leashable` (leash data, tick, elastic force, NBT). The knot entity and the lead item belong to the item code;
//! this is the mob side. `LeashData.leashHolder` is an entity id resolved through the level.

use crate::server::level::ServerLevel;
use crate::server::level::server_entity::encode_payload;
use crate::world::entity::{Entity, EntityId};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::{Item, SoundEvent};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::packets::play::ClientboundSetEntityLinkPacket;
use mc_protocol::packets::play::level::SoundSource;
use uuid::Uuid;

/// What a leash needs to know about its holder entity.
#[derive(Clone, Copy, Debug)]
pub struct LeashHolder {
    pub id: EntityId,
    pub uuid: Uuid,
    pub knot_pos: Option<BlockPos>,
}

impl LeashHolder {
    pub fn of(e: &dyn Entity) -> Self {
        Self {
            id: e.id(),
            uuid: e.uuid(),
            knot_pos: e.leash_knot_pos(),
        }
    }
}

/// `Leashable.LeashData.delayedLeashInfo` (`Either<UUID, BlockPos>`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DelayedLeashInfo {
    Entity(Uuid),
    Knot(BlockPos),
}

/// `Leashable.LeashData`.
#[derive(Clone, Debug, Default)]
pub struct LeashData {
    pub delayed_leash_holder_id: i32,
    pub leash_holder: Option<EntityId>,
    /// UUID of the holder (what `LeashData.CODEC` encodes; cached because saving has no level).
    pub holder_uuid: Option<Uuid>,
    /// The knot position when the holder is a `LeashFenceKnotEntity`.
    pub holder_knot_pos: Option<BlockPos>,
    pub delayed_leash_info: Option<DelayedLeashInfo>,
    pub angular_momentum: f64,
}

impl LeashData {
    /// `new LeashData(entity)`.
    pub fn of_holder(holder: &LeashHolder) -> Self {
        Self {
            leash_holder: Some(holder.id),
            holder_uuid: Some(holder.uuid),
            holder_knot_pos: holder.knot_pos,
            ..Default::default()
        }
    }

    /// `new LeashData(Either)`.
    pub fn of_delayed(info: DelayedLeashInfo) -> Self {
        Self {
            delayed_leash_info: Some(info),
            ..Default::default()
        }
    }

    /// `new LeashData(entityId)`.
    pub fn of_delayed_id(entity_id: i32) -> Self {
        Self {
            delayed_leash_holder_id: entity_id,
            ..Default::default()
        }
    }

    /// `setLeashHolder(entity)`.
    pub fn set_leash_holder(&mut self, holder: &LeashHolder) {
        self.leash_holder = Some(holder.id);
        self.holder_uuid = Some(holder.uuid);
        self.holder_knot_pos = holder.knot_pos;
        self.delayed_leash_info = None;
        self.delayed_leash_holder_id = 0;
    }
}

const ENTITY_ATTACHMENT_POINT: [Vec3; 1] = [Vec3 {
    x: 0.0,
    y: 0.5,
    z: 0.5,
}];
const LEASHER_ATTACHMENT_POINT: [Vec3; 1] = [Vec3 {
    x: 0.0,
    y: 0.5,
    z: 0.0,
}];
const SHARED_QUAD_ATTACHMENT_POINTS: [Vec3; 4] = [
    Vec3 {
        x: -0.5,
        y: 0.5,
        z: 0.5,
    },
    Vec3 {
        x: -0.5,
        y: 0.5,
        z: -0.5,
    },
    Vec3 {
        x: 0.5,
        y: 0.5,
        z: -0.5,
    },
    Vec3 {
        x: 0.5,
        y: 0.5,
        z: 0.5,
    },
];
const AXIS_SPECIFIC_ELASTICITY: Vec3 = Vec3 {
    x: 0.8,
    y: 0.2,
    z: 0.8,
};

/// `Leashable.Wrench`.
#[derive(Clone, Copy, Debug)]
struct Wrench {
    force: Vec3,
    torque: f64,
}

impl Wrench {
    fn torque_from_force(lever_arm: Vec3, force: Vec3) -> f64 {
        lever_arm.z * force.x - lever_arm.x * force.z
    }

    fn accumulate(wrenches: &[Wrench]) -> Wrench {
        let (mut x, mut y, mut z, mut t) = (0.0, 0.0, 0.0, 0.0);
        for w in wrenches {
            x += w.force.x;
            y += w.force.y;
            z += w.force.z;
            t += w.torque;
        }
        Wrench {
            force: Vec3::new(x, y, z),
            torque: t,
        }
    }

    fn scale(self, scale: f64) -> Wrench {
        Wrench {
            force: self.force.scale(scale),
            torque: self.torque * scale,
        }
    }
}

fn data(e: &dyn Entity) -> Option<&LeashData> {
    e.mob().and_then(|m| m.leash_data.as_ref())
}

fn data_mut(e: &mut dyn Entity) -> Option<&mut LeashData> {
    e.mob_mut().and_then(|m| m.leash_data.as_mut())
}

/// `isLeashed()`.
pub fn is_leashed(e: &dyn Entity) -> bool {
    data(e).is_some_and(|d| d.leash_holder.is_some())
}

/// `mayBeLeashed()`.
pub fn may_be_leashed(e: &dyn Entity) -> bool {
    data(e).is_some()
}

/// `getLeashHolder()`.
pub fn get_leash_holder(e: &dyn Entity) -> Option<EntityId> {
    data(e)?.leash_holder
}

/// `leashDistanceTo(entity)`.
pub fn leash_distance_to(e: &dyn Entity, other: &dyn Entity) -> f64 {
    other
        .base()
        .bounding_box()
        .get_center()
        .distance_to(e.base().bounding_box().get_center())
}

/// `canHaveALeashAttachedTo(entity)`.
pub fn can_have_a_leash_attached_to(
    level: &ServerLevel,
    e: &dyn Entity,
    other: &dyn Entity,
) -> bool {
    if e.id() == other.id() {
        return false;
    }
    if leash_distance_to(e, other) > e.leash_snap_distance() {
        false
    } else {
        e.can_be_leashed_in(level)
    }
}

/// `setDelayedLeashHolderId(entityId)`.
pub fn set_delayed_leash_holder_id(e: &mut dyn Entity, level: &mut ServerLevel, entity_id: i32) {
    if let Some(m) = e.mob_mut() {
        m.leash_data = Some(LeashData::of_delayed_id(entity_id));
    }
    drop_leash_impl(e, level, false, false);
}

/// `LeashData.CODEC` (`xor(UUID field, BlockPos)`) as the `leash` tag; `None` when it does not decode.
fn read_leash_tag(tag: &Tag) -> Option<LeashData> {
    match tag {
        Tag::Compound(c) => {
            let uuid = crate::world::entity::entity::read_uuid(c.get("UUID"))?;
            Some(LeashData::of_delayed(DelayedLeashInfo::Entity(uuid)))
        }
        Tag::IntArray(_) | Tag::List(_) => {
            let pos = crate::world::entity::living::read_block_pos(Some(tag))?;
            Some(LeashData::of_delayed(DelayedLeashInfo::Knot(pos)))
        }
        _ => None,
    }
}

/// `readLeashData(input)`.
pub fn read_leash_data(
    e: &mut dyn Entity,
    level_hint: Option<&mut ServerLevel>,
    input: &CompoundTag,
) {
    let new_data = input.get("leash").and_then(read_leash_tag);
    let had = data(e).is_some();
    if had
        && new_data.is_none()
        && let Some(level) = level_hint
    {
        remove_leash(e, level);
    }
    if let Some(m) = e.mob_mut() {
        m.leash_data = new_data;
    }
}

/// `writeLeashData(output, leashData)`: the holder's UUID, the knot position, or the still-delayed info.
pub fn write_leash_data(e: &dyn Entity, out: &mut CompoundTag) {
    let Some(d) = data(e) else { return };
    let tag = if d.leash_holder.is_some() {
        match (d.holder_knot_pos, d.holder_uuid) {
            (Some(pos), _) => Some(Tag::IntArray(vec![pos.x(), pos.y(), pos.z()])),
            (None, Some(uuid)) => Some(uuid_compound(uuid)),
            (None, None) => None,
        }
    } else {
        match d.delayed_leash_info {
            Some(DelayedLeashInfo::Entity(u)) => Some(uuid_compound(u)),
            Some(DelayedLeashInfo::Knot(p)) => Some(Tag::IntArray(vec![p.x(), p.y(), p.z()])),
            None => None,
        }
    };
    if let Some(tag) = tag {
        out.put("leash", tag);
    }
}

fn uuid_compound(uuid: Uuid) -> Tag {
    let mut c = CompoundTag::new();
    c.put_int_array(
        "UUID",
        mc_core::uuid_util::uuid_to_int_array(&uuid).to_vec(),
    );
    Tag::Compound(c)
}

/// `restoreLeashFromSave(entity, leashData)`.
fn restore_leash_from_save(e: &mut dyn Entity, level: &mut ServerLevel) {
    let Some(info) = data(e).and_then(|d| d.delayed_leash_info) else {
        return;
    };
    match info {
        DelayedLeashInfo::Entity(uuid) => {
            if let Some(leasher) = level.get_entity_by_uuid(uuid).map(LeashHolder::of) {
                set_leashed_to(e, level, leasher, true);
                return;
            }
        }
        DelayedLeashInfo::Knot(pos) => {
            if let Some(knot) = super::ext::get_or_create_leash_knot(level, pos)
                .and_then(|id| level.get_entity(id))
                .map(LeashHolder::of)
            {
                set_leashed_to(e, level, knot, true);
                return;
            }
        }
    }
    if e.base().tick_count > 100 {
        crate::world::entity::item::spawn_at_location(
            level,
            e,
            crate::world::item::ItemStack::new(Item::LEAD),
            Vec3::ZERO,
        );
        if let Some(m) = e.mob_mut() {
            m.leash_data = None;
        }
    }
}

/// `dropLeash()`.
pub fn drop_leash(e: &mut dyn Entity, level: &mut ServerLevel) {
    drop_leash_impl(e, level, true, true);
}

/// `removeLeash()`.
pub fn remove_leash(e: &mut dyn Entity, level: &mut ServerLevel) {
    drop_leash_impl(e, level, true, false);
}

/// `dropLeash(entity, sendPacket, dropLead)`.
fn drop_leash_impl(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    send_packet: bool,
    drop_lead: bool,
) {
    let Some(holder) = data(e).and_then(|d| d.leash_holder) else {
        return;
    };
    if let Some(m) = e.mob_mut() {
        m.leash_data = None;
    }
    e.on_leash_removed();
    if drop_lead {
        crate::world::entity::item::spawn_at_location(
            level,
            e,
            crate::world::item::ItemStack::new(Item::LEAD),
            Vec3::ZERO,
        );
    }
    if send_packet {
        level.entity_tracker.send_to_tracking_players(
            e.id(),
            &encode_payload(&ClientboundSetEntityLinkPacket {
                source_id: e.id(),
                dest_id: 0,
            }),
        );
    }
    let leashee = e.id();
    level.with_entity(holder, |level, h| h.notify_leashee_removed(level, leashee));
}

/// `Leashable.tickLeash(level, entity)`.
pub fn tick_leash(e: &mut dyn Entity, level: &mut ServerLevel) {
    if data(e).is_some_and(|d| d.delayed_leash_info.is_some()) {
        restore_leash_from_save(e, level);
    }
    let Some(holder_id) = data(e).and_then(|d| d.leash_holder) else {
        return;
    };
    let holder_interacts = level
        .get_entity(holder_id)
        .is_some_and(|h| h.can_interact_with_level());
    if !e.can_interact_with_level() || !holder_interacts {
        if level.entity_drops() {
            drop_leash(e, level);
        } else {
            remove_leash(e, level);
        }
    }
    let Some(holder_id) = data(e).and_then(|d| d.leash_holder) else {
        return;
    };
    if level.get_entity(holder_id).is_none() {
        return;
    }
    let (distance_to, holder_pos, bb_width) = {
        let h = level.get_entity(holder_id).expect("checked");
        (
            leash_distance_to(e, h),
            h.position(),
            h.base().get_bb_width(),
        )
    };
    e.when_leashed_to(level, holder_id);
    // the Java local `leashData` outlives `dropLeash()`: a snapped leash still applies its momentum once
    let momentum_before = data(e).map_or(0.0, |d| d.angular_momentum);
    if distance_to > e.leash_snap_distance() {
        level.play_sound_at(
            None,
            holder_pos.x,
            holder_pos.y,
            holder_pos.z,
            SoundEvent::ITEM_LEAD_BREAK,
            SoundSource::Neutral,
            1.0,
            1.0,
        );
        e.leash_too_far_behaviour(level);
    } else if distance_to
        > e.leash_elastic_distance() - f64::from(bb_width) - f64::from(e.base().get_bb_width())
        && check_elastic_interactions(e, level, holder_id)
    {
        e.on_elastic_leash_pull();
    } else {
        e.close_range_leash_behaviour(level, holder_id);
    }
    let friction = angular_friction(e, level);
    // `(float)(getYRot() - angularMomentum)`: the subtraction is in double
    let angular_momentum = data(e).map_or(momentum_before, |d| d.angular_momentum);
    let new_y_rot = (f64::from(e.base().y_rot()) - angular_momentum) as f32;
    e.set_y_rot(new_y_rot);
    if let Some(d) = data_mut(e) {
        d.angular_momentum *= f64::from(friction);
    }
}

/// `angularFriction(entity)`.
fn angular_friction(e: &dyn Entity, level: &ServerLevel) -> f32 {
    use crate::world::level::BlockGetter;
    if e.base().on_ground() {
        let pos =
            crate::world::entity::movement::get_block_pos_below_that_affects_my_movement(e, level);
        level.get_block_state(pos).block().data().friction * 0.91
    } else if e.is_in_liquid() {
        0.8
    } else {
        0.91
    }
}

fn get_holder_movement(level: &ServerLevel, holder: &dyn Entity) -> Vec3 {
    if holder.mob().is_some_and(|m| m.is_no_ai()) {
        Vec3::ZERO
    } else {
        holder.get_known_movement(level)
    }
}

/// `checkElasticInteractions(leashHolder, leashData)`.
fn check_elastic_interactions(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    holder_id: EntityId,
) -> bool {
    let (wrenches, quad, relative_velocity) = {
        let Some(holder) = level.get_entity(holder_id) else {
            return false;
        };
        let quad = holder.support_quad_leash_as_holder() && e.support_quad_leash();
        let (entity_points, leasher_points): (&[Vec3], &[Vec3]) = if quad {
            (
                &SHARED_QUAD_ATTACHMENT_POINTS,
                &SHARED_QUAD_ATTACHMENT_POINTS,
            )
        } else {
            (&ENTITY_ATTACHMENT_POINT, &LEASHER_ATTACHMENT_POINT)
        };
        let wrenches = compute_elastic_interaction(level, e, holder, entity_points, leasher_points);
        let relative_velocity =
            get_holder_movement(level, holder).subtract(e.get_known_movement(level));
        (wrenches, quad, relative_velocity)
    };
    if wrenches.is_empty() {
        return false;
    }
    let result = Wrench::accumulate(&wrenches).scale(if quad { 0.25 } else { 1.0 });
    if let Some(d) = data_mut(e) {
        d.angular_momentum += 10.0 * result.torque;
    }
    let push = result
        .force
        .multiply(AXIS_SPECIFIC_ELASTICITY)
        .add(relative_velocity.scale(0.11));
    e.base_mut().add_delta_movement(push);
    true
}

fn compute_elastic_interaction(
    level: &ServerLevel,
    entity: &dyn Entity,
    holder: &dyn Entity,
    entity_points: &[Vec3],
    leasher_points: &[Vec3],
) -> Vec<Wrench> {
    let slack_distance = entity.leash_elastic_distance();
    let current_movement = get_holder_movement(level, entity);
    let entity_y_rot = entity.base().y_rot() * (std::f64::consts::PI / 180.0) as f32;
    let entity_dimensions = Vec3::new(
        f64::from(entity.base().get_bb_width()),
        f64::from(entity.base().get_bb_height()),
        f64::from(entity.base().get_bb_width()),
    );
    let holder_y_rot = holder.base().y_rot() * (std::f64::consts::PI / 180.0) as f32;
    let leasher_dimensions = Vec3::new(
        f64::from(holder.base().get_bb_width()),
        f64::from(holder.base().get_bb_height()),
        f64::from(holder.base().get_bb_width()),
    );
    let mut wrenches = Vec::new();
    for i in 0..entity_points.len() {
        let entity_attach_vector = entity_points[i]
            .multiply(entity_dimensions)
            .y_rot(-entity_y_rot);
        let entity_attach_pos = entity.position().add(entity_attach_vector);
        let leasher_attach_vector = leasher_points[i]
            .multiply(leasher_dimensions)
            .y_rot(-holder_y_rot);
        let leasher_attach_pos = holder.position().add(leasher_attach_vector);
        if let Some(w) = compute_dampened_spring_interaction(
            leasher_attach_pos,
            entity_attach_pos,
            slack_distance,
            current_movement,
            entity_attach_vector,
        ) {
            wrenches.push(w);
        }
    }
    wrenches
}

fn compute_dampened_spring_interaction(
    pivot_point: Vec3,
    object_position: Vec3,
    spring_slack: f64,
    object_motion: Vec3,
    lever_arm: Vec3,
) -> Option<Wrench> {
    let distance = object_position.distance_to(pivot_point);
    if distance < spring_slack {
        return None;
    }
    let mut displacement = pivot_point
        .subtract(object_position)
        .normalize()
        .scale(distance - spring_slack);
    let torque = Wrench::torque_from_force(lever_arm, displacement);
    let same_direction_to_movement = object_motion.dot(displacement) >= 0.0;
    if same_direction_to_movement {
        displacement = displacement.scale(f64::from(0.3f32));
    }
    Some(Wrench {
        force: displacement,
        torque,
    })
}

/// `createQuadLeashOffsets(entity, frontOffset, frontBack, leftRight, height)`.
pub fn create_quad_leash_offsets(
    entity: &dyn Entity,
    front_offset: f64,
    front_back: f64,
    left_right: f64,
    height: f64,
) -> [Vec3; 4] {
    let width = f64::from(entity.base().get_bb_width());
    let front_offset_scaled = front_offset * width;
    let front_back_scaled = front_back * width;
    let left_right_scaled = left_right * width;
    let height_scaled = height * f64::from(entity.base().get_bb_height());
    [
        Vec3::new(
            -left_right_scaled,
            height_scaled,
            front_back_scaled + front_offset_scaled,
        ),
        Vec3::new(
            -left_right_scaled,
            height_scaled,
            -front_back_scaled + front_offset_scaled,
        ),
        Vec3::new(
            left_right_scaled,
            height_scaled,
            -front_back_scaled + front_offset_scaled,
        ),
        Vec3::new(
            left_right_scaled,
            height_scaled,
            front_back_scaled + front_offset_scaled,
        ),
    ]
}

/// `getLeashOffset()`.
pub fn get_leash_offset(e: &dyn Entity) -> Vec3 {
    Vec3::new(
        0.0,
        f64::from(e.base().eye_height),
        f64::from(e.base().get_bb_width() * 0.4f32),
    )
}

/// `setLeashedTo(holder, synch)`.
pub fn set_leashed_to(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    holder: LeashHolder,
    synch: bool,
) {
    if e.id() == holder.id {
        return;
    }
    let old_holder = match data_mut(e) {
        None => {
            if let Some(m) = e.mob_mut() {
                m.leash_data = Some(LeashData::of_holder(&holder));
            }
            None
        }
        Some(d) => {
            let old = d.leash_holder;
            d.set_leash_holder(&holder);
            old
        }
    };
    if let Some(old) = old_holder.filter(|&o| o != holder.id) {
        let leashee = e.id();
        level.with_entity(old, |level, h| h.notify_leashee_removed(level, leashee));
    }
    if synch {
        level.entity_tracker.send_to_tracking_players(
            e.id(),
            &encode_payload(&ClientboundSetEntityLinkPacket {
                source_id: e.id(),
                dest_id: holder.id,
            }),
        );
    }
    if e.base().is_passenger() {
        level.stop_riding(e);
    }
}

/// `Leashable.leashableInArea(level, pos, test)`: the leashable entities within the 32 block scan cube around `pos`.
pub fn leashable_in_area(
    level: &ServerLevel,
    pos: Vec3,
    test: impl Fn(&dyn Entity) -> bool,
) -> Vec<EntityId> {
    let scan_area = mc_core::phys::AABB::of_size(pos, 32.0, 32.0, 32.0);
    level.get_entities_of_class(&scan_area, |x| x.mob().is_some(), |x| test(x))
}

/// `Leashable.leashableLeashedTo(entity)`.
pub fn leashable_leashed_to(level: &ServerLevel, holder: &dyn Entity) -> Vec<EntityId> {
    let holder_id = holder.id();
    leashable_in_area(level, holder.base().bounding_box().get_center(), |l| {
        get_leash_holder(l) == Some(holder_id)
    })
}

/// `Leashable.whenLeashedTo(leashHolder)` (`leashHolder.notifyLeashHolder(this)`).
pub fn when_leashed_to(e: &dyn Entity, level: &mut ServerLevel, holder: EntityId) {
    let leashee = e.id();
    level.with_entity(holder, |_, h| h.notify_leash_holder(leashee));
}

/// `Mob.leashTooFarBehaviour()`: `dropLeash()` and the MOVE flag of the goal selector is disabled.
pub fn mob_leash_too_far_behaviour(e: &mut dyn Entity, level: &mut ServerLevel) {
    drop_leash(e, level);
    if let Some(m) = e.mob_mut() {
        m.goal_selector
            .disable_control_flag(crate::world::entity::ai::goal::Flag::Move);
    }
}
