//! `MobEffectInstance` (data, `update`/hidden-effect chain, duration ticking, NBT codec). Applying effects to an
//! entity is `living_entity::effects`; the per-effect behaviour is `world::effect`.
//! Client-only `BlendState` (`getBlendFactor`/`copyBlendState`/`skipBlending`) is not modelled.
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_data::MobEffect;
use mc_nbt::{CompoundTag, Tag};
use std::cmp::Ordering;

pub const INFINITE_DURATION: i32 = -1;

/// `Clone` copies the hidden-effect chain; vanilla's copy constructor `new MobEffectInstance(copy)` does not
/// (see [`MobEffectInstance::copy`]).
#[derive(Clone, Debug)]
pub struct MobEffectInstance {
    pub effect: MobEffect,
    pub duration: i32,
    pub amplifier: i32,
    pub ambient: bool,
    pub visible: bool,
    pub show_icon: bool,
    pub hidden_effect: Option<Box<MobEffectInstance>>,
}

/// `MobEffectInstance.equals` (the hidden effect is ignored).
impl PartialEq for MobEffectInstance {
    fn eq(&self, o: &Self) -> bool {
        self.duration == o.duration
            && self.amplifier == o.amplifier
            && self.ambient == o.ambient
            && self.visible == o.visible
            && self.show_icon == o.show_icon
            && self.effect == o.effect
    }
}

impl From<&mc_protocol::components::MobEffectInstance> for MobEffectInstance {
    /// `new MobEffectInstance(effect)` of a component value: the copy constructor drops the hidden effect.
    fn from(c: &mc_protocol::components::MobEffectInstance) -> Self {
        Self {
            effect: c.effect,
            duration: c.duration,
            amplifier: c.amplifier,
            ambient: c.ambient,
            visible: c.visible,
            show_icon: c.show_icon,
            hidden_effect: None,
        }
    }
}

impl MobEffectInstance {
    /// `new MobEffectInstance(effect, duration, amplifier, ambient, visible, showIcon, hidden)`: the amplifier is clamped to 0..=255.
    pub fn new(
        effect: MobEffect,
        duration: i32,
        amplifier: i32,
        ambient: bool,
        visible: bool,
        show_icon: bool,
        hidden_effect: Option<Box<MobEffectInstance>>,
    ) -> Self {
        Self {
            effect,
            duration,
            amplifier: amplifier.clamp(0, 255),
            ambient,
            visible,
            show_icon,
            hidden_effect,
        }
    }

    /// `new MobEffectInstance(effect, duration, amplifier)`.
    pub fn of(effect: MobEffect, duration: i32, amplifier: i32) -> Self {
        Self::with_flags(effect, duration, amplifier, false, true)
    }

    /// `new MobEffectInstance(effect, duration, amplifier, ambient, visible)` (`showIcon = visible`).
    pub fn with_flags(
        effect: MobEffect,
        duration: i32,
        amplifier: i32,
        ambient: bool,
        visible: bool,
    ) -> Self {
        Self::new(effect, duration, amplifier, ambient, visible, visible, None)
    }

    /// `new MobEffectInstance(copy)`: details only, no hidden effect.
    pub fn copy(&self) -> Self {
        Self {
            hidden_effect: None,
            ..self.clone()
        }
    }

    /// `setDetailsFrom(copy)`.
    fn set_details_from(&mut self, copy: &MobEffectInstance) {
        self.duration = copy.duration;
        self.amplifier = copy.amplifier;
        self.ambient = copy.ambient;
        self.visible = copy.visible;
        self.show_icon = copy.show_icon;
    }

    /// `update(takeOver)`: merges a second instance of the same effect into this one; true when something changed.
    pub fn update(&mut self, take_over: &MobEffectInstance) -> bool {
        if self.effect != take_over.effect {
            log::warn!("This method should only be called for matching effects!");
        }
        let mut changed = false;
        if take_over.amplifier > self.amplifier {
            if take_over.is_shorter_duration_than(self) {
                let prev_hidden = self.hidden_effect.take();
                let mut hidden = self.copy();
                hidden.hidden_effect = prev_hidden;
                self.hidden_effect = Some(Box::new(hidden));
            }
            self.amplifier = take_over.amplifier;
            self.duration = take_over.duration;
            changed = true;
        } else if self.is_shorter_duration_than(take_over) {
            if take_over.amplifier == self.amplifier {
                self.duration = take_over.duration;
                changed = true;
            } else {
                match &mut self.hidden_effect {
                    None => self.hidden_effect = Some(Box::new(take_over.copy())),
                    Some(h) => {
                        h.update(take_over);
                    }
                }
            }
        }
        if (!take_over.ambient && self.ambient) || changed {
            self.ambient = take_over.ambient;
            changed = true;
        }
        if take_over.visible != self.visible {
            self.visible = take_over.visible;
            changed = true;
        }
        if take_over.show_icon != self.show_icon {
            self.show_icon = take_over.show_icon;
            changed = true;
        }
        changed
    }

    fn is_shorter_duration_than(&self, other: &MobEffectInstance) -> bool {
        !self.is_infinite_duration()
            && (self.duration < other.duration || other.is_infinite_duration())
    }

    pub fn is_infinite_duration(&self) -> bool {
        self.duration == -1
    }

    pub fn ends_within(&self, ticks: i32) -> bool {
        !self.is_infinite_duration() && self.duration <= ticks
    }

    /// `withScaledDuration(scale)`.
    pub fn with_scaled_duration(&self, scale: f32) -> Self {
        let mut copy = self.copy();
        copy.duration = copy.map_duration(|d| mth::floor_f32(d as f32 * scale).max(1));
        copy
    }

    /// `mapDuration(mapper)`.
    pub fn map_duration(&self, mapper: impl FnOnce(i32) -> i32) -> i32 {
        if !self.is_infinite_duration() && self.duration != 0 {
            mapper(self.duration)
        } else {
            self.duration
        }
    }

    /// `hasRemainingDuration()`.
    pub fn has_remaining_duration(&self) -> bool {
        self.is_infinite_duration() || self.duration > 0
    }

    /// `tickDownDuration()` (also ticks the hidden effects).
    pub fn tick_down_duration(&mut self) {
        if let Some(h) = &mut self.hidden_effect {
            h.tick_down_duration();
        }
        self.duration = self.map_duration(|d| d.wrapping_sub(1));
    }

    /// `downgradeToHiddenEffect()`: when this effect ran out and a hidden one exists, it takes over.
    pub fn downgrade_to_hidden_effect(&mut self) -> bool {
        if self.duration == 0
            && let Some(h) = self.hidden_effect.take()
        {
            self.set_details_from(&h);
            self.hidden_effect = h.hidden_effect;
            return true;
        }
        false
    }

    /// `compareTo(o)`.
    pub fn compare_to(&self, o: &MobEffectInstance) -> Ordering {
        const UPDATE_CUT_OFF: i32 = 32147;
        let color = |e: &Self| e.effect.data().color;
        if (self.duration <= UPDATE_CUT_OFF || o.duration <= UPDATE_CUT_OFF)
            && (!self.ambient || !o.ambient)
        {
            self.ambient
                .cmp(&o.ambient)
                .then(self.is_infinite_duration().cmp(&o.is_infinite_duration()))
                .then(self.duration.cmp(&o.duration))
                .then(color(self).cmp(&color(o)))
        } else {
            self.ambient
                .cmp(&o.ambient)
                .then(color(self).cmp(&color(o)))
        }
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
        Some(Self::new(
            effect,
            duration,
            amplifier & 0xFF,
            ambient,
            visible,
            show_icon,
            hidden,
        ))
    }
}
