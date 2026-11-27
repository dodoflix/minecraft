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
        Self {
            invulnerable: false,
            flying: false,
            mayfly: false,
            instabuild: false,
            may_build: true,
            flying_speed: 0.05,
            walking_speed: 0.1,
        }
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

    /// `Abilities.Packed.CODEC`.
    pub fn codec() -> mc_dfu::Codec<Abilities> {
        use mc_dfu::extra::optional_always_present_field_of as field;
        use mc_dfu::{BOOL, FLOAT};
        mc_dfu::record!(Abilities {
            invulnerable: field(&BOOL, "invulnerable", false),
            flying: field(&BOOL, "flying", false),
            mayfly: field(&BOOL, "mayfly", false),
            instabuild: field(&BOOL, "instabuild", false),
            may_build: field(&BOOL, "mayBuild", true),
            flying_speed: field(&FLOAT, "flySpeed", 0.05),
            walking_speed: field(&FLOAT, "walkSpeed", 0.1),
        })
        .codec()
    }
}
