//! `net.minecraft.world.entity.monster.*` (cube mobs, ravager, warden, creaking, breeze; piglin, hoglin, zoglin).

pub mod cubemob;
pub mod illager;
pub mod patrolling_monster;
pub mod zombie;
// [WardenMobs]

use crate::world::entity::Entity;

/// `rider.startRiding(vehicle, false, false)` for a vehicle that is not in the level yet (a mob being finalized, e.g. the
/// skeleton of a spider jockey): the same checks and links as `ServerLevel::start_riding`, without level lookups. The
/// rider is added to the level together with its vehicle (`MobData::pending_passengers`).
pub fn start_riding_unadded(rider: &mut dyn Entity, vehicle: &mut dyn Entity) -> bool {
    if rider.base().vehicle == Some(vehicle.id())
        || !vehicle.could_accept_passenger()
        || !vehicle.get_type().data().serializable
    {
        return false;
    }
    if !(rider.can_ride(&*vehicle) && vehicle.can_add_passenger(&*rider)) {
        return false;
    }
    rider.base_mut().vehicle = Some(vehicle.id());
    vehicle.add_passenger(&*rider, false);
    super::mob::set_first_passenger(vehicle, Some(&*rider));
    true
}

/// A goal with `setFlags(...)` replaced after construction (`goal.setFlags(EnumSet.of(...))` on a library goal).
pub struct WithFlags<G: crate::world::entity::ai::goal::Goal> {
    goal: G,
    flags: crate::world::entity::ai::goal::GoalFlags,
}

impl<G: crate::world::entity::ai::goal::Goal> WithFlags<G> {
    pub fn new(goal: G, flags: crate::world::entity::ai::goal::GoalFlags) -> Self {
        Self { goal, flags }
    }
}

impl<G: crate::world::entity::ai::goal::Goal> crate::world::entity::ai::goal::Goal
    for WithFlags<G>
{
    fn can_use(
        &mut self,
        mob: &mut dyn Entity,
        level: &mut crate::server::level::ServerLevel,
    ) -> bool {
        self.goal.can_use(mob, level)
    }

    fn can_continue_to_use(
        &mut self,
        mob: &mut dyn Entity,
        level: &mut crate::server::level::ServerLevel,
    ) -> bool {
        self.goal.can_continue_to_use(mob, level)
    }

    fn is_interruptable(&self) -> bool {
        self.goal.is_interruptable()
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut crate::server::level::ServerLevel) {
        self.goal.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut crate::server::level::ServerLevel) {
        self.goal.stop(mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.goal.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut crate::server::level::ServerLevel) {
        self.goal.tick(mob, level);
    }

    fn flags(&self) -> crate::world::entity::ai::goal::GoalFlags {
        self.flags
    }

    fn name(&self) -> &'static str {
        self.goal.name()
    }
}
