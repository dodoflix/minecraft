//! `net.minecraft.network.protocol.game.DebugEntityNameGenerator`: the names the debug renderers show for entities.

use crate::world::entity::{Entity, ServerPlayer};
use mc_core::random::RandomSource as _;
use mc_core::random::create_thread_local_instance;
use mc_core::uuid_util::java_hash_code;
use uuid::Uuid;

const NAMES_FIRST_PART: [&str; 33] = [
    "Slim", "Far", "River", "Silly", "Fat", "Thin", "Fish", "Bat", "Dark", "Oak", "Sly", "Bush",
    "Zen", "Bark", "Cry", "Slack", "Soup", "Grim", "Hook", "Dirt", "Mud", "Sad", "Hard", "Crook",
    "Sneak", "Stink", "Weird", "Fire", "Soot", "Soft", "Rough", "Cling", "Scar",
];

const NAMES_SECOND_PART: [&str; 30] = [
    "Fox", "Tail", "Jaw", "Whisper", "Twig", "Root", "Finder", "Nose", "Brow", "Blade", "Fry",
    "Seek", "Wart", "Tooth", "Foot", "Leaf", "Stone", "Fall", "Face", "Tongue", "Voice", "Lip",
    "Mouth", "Snail", "Toe", "Ear", "Hair", "Beard", "Shirt", "Fist",
];

/// `getEntityName(entity)`: a player's name, a custom name, or the name derived from the UUID.
pub fn get_entity_name(entity: &dyn Entity) -> String {
    if let Some(player) = entity.as_any().downcast_ref::<ServerPlayer>() {
        return player.get_plain_text_name();
    }
    match entity.base().get_custom_name() {
        Some(name) => name.get_string(),
        None => get_entity_name_for_uuid(entity.uuid()),
    }
}

/// `getEntityName(uuid)`: `RandomSource.createThreadLocalInstance(uuid.hashCode() >> 2)` picks both halves.
pub fn get_entity_name_for_uuid(uuid: Uuid) -> String {
    let mut random = create_thread_local_instance(i64::from(java_hash_code(&uuid) >> 2));
    let first = NAMES_FIRST_PART[random.next_int_bounded(NAMES_FIRST_PART.len() as i32) as usize];
    let second =
        NAMES_SECOND_PART[random.next_int_bounded(NAMES_SECOND_PART.len() as i32) as usize];
    format!("{first}{second}")
}
