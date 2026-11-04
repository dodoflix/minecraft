//! Typed registry ids: one `Copy` newtype per built-in registry with a vanilla-named const per entry
//! (`Block::STONE`, `Item::DIAMOND_PICKAXE`, `EntityType::PIG`, `SoundEvent::ENTITY_ALLAY_HURT`, ...).
//! Generated; the behaviour of `Block`/`Item`/`EntityType`/`Fluid` lives in their own modules.

include!(concat!(env!("OUT_DIR"), "/ids.rs"));
