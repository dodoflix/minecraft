//! `MobEffectInstance` data (+ its NBT codec). Ticking/applying effects is the effects slice's job.
use mc_core::ident::Identifier;
use mc_data::MobEffect;
use mc_nbt::{CompoundTag, Tag};

pub const INFINITE_DURATION: i32 = -1;

#[derive(Clone, Debug, PartialEq)]
pub struct MobEffectInstance {
    pub effect: MobEffect,
    pub duration: i32,
    pub amplifier: i32,
    pub ambient: bool,
    pub visible: bool,
    pub show_icon: bool,
    pub hidden_effect: Option<Box<MobEffectInstance>>,
}

impl MobEffectInstance {
    /// `new MobEffectInstance(effect, duration, amplifier, ambient, visible, showIcon, hidden)`: the amplifier is clamped to 0..=255.
    pub fn new(effect: MobEffect, duration: i32, amplifier: i32, ambient: bool, visible: bool, show_icon: bool, hidden_effect: Option<Box<MobEffectInstance>>) -> Self {
        Self { effect, duration, amplifier: amplifier.clamp(0, 255), ambient, visible, show_icon, hidden_effect }
    }

    /// `MobEffectInstance.CODEC` encode.
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.put_string("id", self.effect.identifier().to_string());
        self.put_details(&mut tag);
        tag
    }

    fn put_details(&self, tag: &mut CompoundTag) {
        if self.amplifier != 0 {
            tag.put_byte("amplifier", (self.amplifier & 0xFF) as i8);
        }
        if self.duration != 0 {
            tag.put_int("duration", self.duration);
        }
        if self.ambient {
            tag.put_boolean("ambient", true);
        }
        if !self.visible {
            tag.put_boolean("show_particles", false);
        }
        tag.put_boolean("show_icon", self.show_icon);
        if let Some(hidden) = &self.hidden_effect {
            let mut h = CompoundTag::new();
            hidden.put_details(&mut h);
            tag.put("hidden_effect", Tag::Compound(h));
        }
    }

    /// `MobEffectInstance.CODEC` decode (`None` for an unknown effect id / malformed fields).
    pub fn from_nbt(tag: &CompoundTag) -> Option<Self> {
        let id = Identifier::parse(tag.get_string("id")?).ok()?;
        let effect = MobEffect::by_name(&id.to_string())?;
        Self::read_details(effect, tag)
    }

    fn read_details(effect: MobEffect, tag: &CompoundTag) -> Option<Self> {
        let num = |key: &str| -> Option<Option<i32>> {
            match tag.get(key) {
                None => Some(None),
                Some(t) => t.as_number().map(|n| Some(n.int_value())),
            }
        };
        let flag = |key: &str, default: bool| -> Option<bool> {
            match tag.get(key) {
                None => Some(default),
                Some(t) => t.as_number().map(|n| n.int_value() != 0),
            }
        };
        let amplifier = num("amplifier")?.unwrap_or(0);
        if !(0..=255).contains(&(amplifier & 0xFF)) {
            return None;
        }
        let duration = num("duration")?.unwrap_or(0);
        let ambient = flag("ambient", false)?;
        let visible = flag("show_particles", true)?;
        let show_icon = match tag.get("show_icon") {
            None => visible,
            Some(t) => t.as_number()?.int_value() != 0,
        };
        let hidden = match tag.get_compound("hidden_effect") {
            Some(h) => Some(Box::new(Self::read_details(effect, h)?)),
            None => None,
        };
        Some(Self::new(effect, duration, amplifier & 0xFF, ambient, visible, show_icon, hidden))
    }
}
