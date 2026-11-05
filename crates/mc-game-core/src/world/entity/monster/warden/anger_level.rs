//! `AngerLevel`.

use mc_data::SoundEvent;

/// `AngerLevel` (`CALM`, `AGITATED`, `ANGRY`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AngerLevel {
    Calm,
    Agitated,
    Angry,
}

impl AngerLevel {
    /// `getMinimumAnger()`.
    pub fn get_minimum_anger(self) -> i32 {
        match self {
            AngerLevel::Calm => 0,
            AngerLevel::Agitated => 40,
            AngerLevel::Angry => 80,
        }
    }

    /// `getAmbientSound()`.
    pub fn get_ambient_sound(self) -> SoundEvent {
        match self {
            AngerLevel::Calm => SoundEvent::ENTITY_WARDEN_AMBIENT,
            AngerLevel::Agitated => SoundEvent::ENTITY_WARDEN_AGITATED,
            AngerLevel::Angry => SoundEvent::ENTITY_WARDEN_ANGRY,
        }
    }

    /// `getListeningSound()`.
    pub fn get_listening_sound(self) -> SoundEvent {
        match self {
            AngerLevel::Calm => SoundEvent::ENTITY_WARDEN_LISTENING,
            AngerLevel::Agitated | AngerLevel::Angry => SoundEvent::ENTITY_WARDEN_LISTENING_ANGRY,
        }
    }

    /// `byAnger(anger)`: the highest level whose minimum anger is reached.
    pub fn by_anger(anger: i32) -> AngerLevel {
        if anger >= 80 {
            AngerLevel::Angry
        } else if anger >= 40 {
            AngerLevel::Agitated
        } else {
            AngerLevel::Calm
        }
    }

    /// `isAngry()`.
    pub fn is_angry(self) -> bool {
        self == AngerLevel::Angry
    }
}
