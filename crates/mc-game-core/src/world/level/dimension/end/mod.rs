//! `net.minecraft.world.level.dimension.end`: the ender dragon fight of the End and its respawn stages.
//!
//! `EnderDragonFight` is saved data of the End level (`data/minecraft/ender_dragon_fight.dat`). Vanilla's fight holds its
//! `ServerLevel`; here the fight is plain data in the level's saved data storage and every operation is a function over
//! `&mut ServerLevel` that reads and writes the fight in short scopes: a destroyed crystal calls back into the fight while
//! the fight's own tick runs, so the fight is never taken out of its storage.

mod dragon_respawn_stage;
mod ender_dragon_fight;

pub use dragon_respawn_stage::DragonRespawnStage;
pub use ender_dragon_fight::*;
