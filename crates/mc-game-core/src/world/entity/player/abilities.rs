//! `net.minecraft.world.entity.player.Abilities`.
use mc_nbt::CompoundTag;

#[derive(Clone, Debug, PartialEq)]
pub struct Abilities {
    pub invulnerable: bool,
    pub flying: bool,
    pub mayfly: bool,
    pub instabuild: bool,
    pub may_build: bool,
    flying_speed: f32,
    walking_speed: f32,
}

impl Default for Abilities {
    fn default() -> Self {
        Self { invulnerable: false, flying: false, mayfly: false, instabuild: false, may_build: true, flying_speed: 0.05, walking_speed: 0.1 }
    }
}

impl Abilities {
    pub fn get_flying_speed(&self) -> f32 {
        self.flying_speed
    }
    pub fn set_flying_speed(&mut self, value: f32) {
        self.flying_speed = value;
    }
    pub fn get_walking_speed(&self) -> f32 {
        self.walking_speed
    }
    pub fn set_walking_speed(&mut self, value: f32) {
        self.walking_speed = value;
    }

    /// `Abilities.Packed.CODEC` encode (every field is always written).
    pub fn pack(&self) -> CompoundTag {
        let mut t = CompoundTag::new();
        t.put_boolean("invulnerable", self.invulnerable);
        t.put_boolean("flying", self.flying);
        t.put_boolean("mayfly", self.mayfly);
        t.put_boolean("instabuild", self.instabuild);
        t.put_boolean("mayBuild", self.may_build);
        t.put_float("flySpeed", self.flying_speed);
        t.put_float("walkSpeed", self.walking_speed);
        t
    }

    /// `apply(Packed)` from the `abilities` tag; a tag with a wrongly typed field is ignored as a whole (codec error).
    pub fn apply(&mut self, tag: &CompoundTag) {
        let flag = |key: &str, default: bool| -> Option<bool> { tag.get(key).map_or(Some(default), |t| t.as_number().map(|n| n.int_value() != 0)) };
        let float = |key: &str, default: f32| -> Option<f32> { tag.get(key).map_or(Some(default), |t| t.as_number().map(|n| n.float_value())) };
        let (Some(invulnerable), Some(flying), Some(mayfly), Some(instabuild), Some(may_build), Some(fly), Some(walk)) = (
            flag("invulnerable", false),
            flag("flying", false),
            flag("mayfly", false),
            flag("instabuild", false),
            flag("mayBuild", true),
            float("flySpeed", 0.05),
            float("walkSpeed", 0.1),
        ) else {
            return;
        };
        *self = Self { invulnerable, flying, mayfly, instabuild, may_build, flying_speed: fly, walking_speed: walk };
    }
}
