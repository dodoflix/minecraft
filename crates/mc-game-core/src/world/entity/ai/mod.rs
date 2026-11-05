//! `net.minecraft.world.entity.ai`: the Brain stack (memories, sensors, behaviors, activities, gossip, village types).
//! The goal framework, controls, util and targeting and navigation are siblings (`goal`, `control`, `util`, `targeting`, `navigation`).
//!
//! # Architecture
//!
//! * [`brain::Brain`] lives in `LivingEntityData.brain` (`Entity::brain()` / `brain_mut()`); a mob with a brain builds it in
//!   its constructor and in `make_brain(packed)` through a static [`brain::Provider`] (`Brain.provider(sensors, activities)`),
//!   ticks it from `custom_server_ai_step` with `Brain::tick(level, body)`, and saves/loads it with
//!   [`memory::BrainPacked`] (`Brain.pack()` / `Brain.Packed.CODEC`) — see `LivingEntityData::save_brain` / `Entity::make_brain`.
//! * [`memory`] — typed memories (`memory_module_type::HOME`, ...), expiry, persistence.
//! * [`sensing`] — `Sensor` / `SensorType` and all vanilla sensors.
//! * [`behavior`] — `BehaviorControl`, `Behavior`, `OneShot`, `BehaviorBuilder`, gates; worked examples in the module header.
//! * [`gossip`], [`village`] — villager gossip arithmetic, reputation event types.

pub mod behavior;
pub mod brain;
pub mod control;
pub mod goal;
pub mod gossip;
pub mod memory;
pub mod navigation;
pub mod sensing;
pub mod targeting;
pub mod util;
pub mod village;
