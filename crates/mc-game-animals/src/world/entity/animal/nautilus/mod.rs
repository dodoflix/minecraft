//! `net.minecraft.world.entity.animal.nautilus`.

pub mod abstract_nautilus;
pub mod nautilus;
pub mod nautilus_ai;
pub mod zombie_nautilus;
pub mod zombie_nautilus_ai;
pub mod zombie_nautilus_variant;

pub use nautilus::Nautilus;
pub use zombie_nautilus::ZombieNautilus;
pub use zombie_nautilus_variant::ZombieNautilusVariant;
