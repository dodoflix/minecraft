//! `net.minecraft.world.effect.*`: `MobEffect` behaviours (`mob_effect`), `MobEffectUtil` (`mob_effect_util`),
//! `PotionContents` / `Potions` behaviour (`potion_contents`) and the consumable glue (`consume`).
//! `MobEffectInstance` itself lives in `world::entity::effect`; applying effects to a living entity is
//! `world::entity::living_entity::effects`.

pub mod consume;
pub mod mob_effect;
pub mod mob_effect_util;
pub mod potion_contents;
pub mod reactions;

pub use mob_effect::{EffectCause, MobEffectExt};
