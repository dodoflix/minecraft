//! `net.minecraft.world.entity.animal.*`: the variant types of the animals (the mob entities themselves are added by the
//! mob slices). Variant selection and the shared codecs are in [`crate::world::entity::variant`].

// pub mod bee; // needs GoalsB + PoiManager

pub mod chicken;
pub mod cow;
// pub mod dolphin; // needs GoalsB + locate
pub mod feline;
pub mod fish;
pub mod frog;
pub mod nautilus;
pub mod pig;
pub mod squid;
pub mod temperature_variants;
pub mod wolf;
