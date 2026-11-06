//! `net.minecraft.world.entity.decoration.Mannequin` (an `Avatar`: a living entity wearing a player model).

use crate::define_accessor;
use crate::world::entity::dimensions::EntityDimensions;
use crate::world::entity::entity::{EntityData, TOTAL_AIR_SUPPLY};
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::living::{self, LivingEntityData};
use crate::world::entity::player::{
    DATA_PLAYER_MAIN_HAND, DATA_PLAYER_MODE_CUSTOMISATION, avatar_dimensions,
};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use linkme::distributed_slice;
use mc_data::EntityType;
use mc_data::enums::Pose;
use mc_dfu::NBT;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::components::{PROFILE, ResolvableProfile};
use mc_protocol::packets::common::HumanoidArm;
use mc_text::Component;
use std::any::Any;

define_accessor!(pub DATA_PROFILE: ResolvableProfile = Mannequin);
define_accessor!(pub DATA_IMMOVABLE: Boolean = Mannequin);
define_accessor!(pub DATA_DESCRIPTION: OptionalComponent = Mannequin);

/// `PlayerModelPart` names by mask bit (`CAPE` 0 ... `HAT` 6).
const MODEL_PARTS: [&str; 7] = [
    "cape",
    "jacket",
    "left_sleeve",
    "right_sleeve",
    "left_pants_leg",
    "right_pants_leg",
    "hat",
];
/// `ALL_LAYERS`.
const ALL_LAYERS: i8 = 0x7f;
/// `VALID_POSES` (name by pose).
const VALID_POSES: [(Pose, &str); 5] = [
    (Pose::Standing, "standing"),
    (Pose::Crouching, "crouching"),
    (Pose::Swimming, "swimming"),
    (Pose::FallFlying, "fall_flying"),
    (Pose::Sleeping, "sleeping"),
];

fn default_description() -> Component {
    Component::translatable("entity.minecraft.mannequin.label", vec![])
}

/// `ResolvableProfile.Static.EMPTY`.
fn default_profile() -> ResolvableProfile {
    ResolvableProfile::create_unresolved("")
}

pub struct Mannequin {
    living: LivingEntityData,
    description: Component,
    hide_description: bool,
}

impl Mannequin {
    /// `Mannequin(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut living = LivingEntityData::new(ty, id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_PLAYER_MAIN_HAND, HumanoidArm::Right);
            b.define(DATA_PLAYER_MODE_CUSTOMISATION, 0);
            b.define(DATA_PROFILE, default_profile());
            b.define(DATA_IMMOVABLE, false);
            b.define(DATA_DESCRIPTION, Some(default_description()));
        });
        living
            .entity
            .entity_data
            .set(DATA_PLAYER_MODE_CUSTOMISATION, ALL_LAYERS);
        Self {
            living,
            description: default_description(),
            hide_description: false,
        }
    }

    pub fn get_profile(&self) -> ResolvableProfile {
        self.living.entity.entity_data.get(DATA_PROFILE)
    }

    fn get_immovable(&self) -> bool {
        self.living.entity.entity_data.get(DATA_IMMOVABLE)
    }

    fn update_description(&mut self) {
        let value = if self.hide_description {
            None
        } else {
            Some(self.description.clone())
        };
        self.living.entity.entity_data.set(DATA_DESCRIPTION, value);
    }

    /// `LAYERS_CODEC` encode: the *hidden* parts as names.
    fn hidden_layers_tag(mask: i8) -> Tag {
        let mut list = ListTag::new();
        for (bit, name) in MODEL_PARTS.iter().enumerate() {
            if mask & (1 << bit) == 0 {
                list.push(Tag::String(name.to_string()));
            }
        }
        Tag::List(list)
    }

    /// `LAYERS_CODEC` decode.
    fn hidden_layers_from_tag(tag: &Tag) -> Option<i8> {
        let list = tag.as_list()?;
        let mut mask = ALL_LAYERS;
        for t in list.iter() {
            let bit = MODEL_PARTS.iter().position(|n| Some(*n) == t.as_string())?;
            mask &= !(1 << bit);
        }
        Some(mask)
    }
}

impl Entity for Mannequin {
    fn base(&self) -> &EntityData {
        &self.living.entity
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.living.entity
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }
    fn living(&self) -> Option<&LivingEntityData> {
        Some(&self.living)
    }
    fn living_mut(&mut self) -> Option<&mut LivingEntityData> {
        Some(&mut self.living)
    }
    /// `getDefaultDimensions(pose)` (`Avatar.POSES`) scaled.
    fn get_dimensions(&self, pose: Pose) -> EntityDimensions {
        avatar_dimensions(pose)
            .scale(self.living.get_attribute_value(mc_data::Attribute::SCALE) as f32)
    }
    fn is_immobile(&self) -> bool {
        self.get_immovable() || self.living.is_dead_or_dying()
    }
    fn is_effective_ai(&self) -> bool {
        !self.get_immovable()
    }
    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(profile) = stack.get(&PROFILE) {
            self.living
                .entity
                .entity_data
                .set(DATA_PROFILE, profile.clone());
        }
        crate::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }
    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        living::add_additional_save_data(
            &self.living,
            self.living.absorption_amount,
            &self.living.equipment,
            out,
        );
        if let Ok(tag) = ResolvableProfile::codec()
            .encode_start(&NBT, &self.get_profile())
            .into_result()
            .and_then(|d| d.to_tag())
        {
            out.put("profile", tag);
        }
        out.put(
            "hidden_layers",
            Self::hidden_layers_tag(
                self.living
                    .entity
                    .entity_data
                    .get(DATA_PLAYER_MODE_CUSTOMISATION),
            ),
        );
        out.put_string(
            "main_hand",
            if self.living.entity.entity_data.get(DATA_PLAYER_MAIN_HAND) == HumanoidArm::Left {
                "left"
            } else {
                "right"
            },
        );
        let pose = self.living.entity.get_pose();
        if let Some((_, name)) = VALID_POSES.iter().find(|(p, _)| *p == pose) {
            out.put_string("pose", *name);
        }
        out.put_boolean("immovable", self.get_immovable());
        match self.living.entity.entity_data.get(DATA_DESCRIPTION) {
            Some(description) => {
                if description != default_description()
                    && let Ok(tag) = description.to_nbt()
                {
                    out.put("description", tag);
                }
            }
            None => out.put_boolean("hide_description", true),
        }
    }
    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        let (absorption, equipment) = living::read_additional_save_data(&mut self.living, input);
        self.living.absorption_amount = absorption;
        self.living.equipment = equipment;
        if let Some(profile) = input.get("profile").and_then(|t| {
            ResolvableProfile::codec()
                .parse(&NBT, &mc_dfu::Dyn::from(t))
                .into_result()
                .ok()
        }) {
            self.living.entity.entity_data.set(DATA_PROFILE, profile);
        }
        let layers = input
            .get("hidden_layers")
            .and_then(Self::hidden_layers_from_tag)
            .unwrap_or(ALL_LAYERS);
        self.living
            .entity
            .entity_data
            .set(DATA_PLAYER_MODE_CUSTOMISATION, layers);
        let arm = if input.get_string("main_hand") == Some("left") {
            HumanoidArm::Left
        } else {
            HumanoidArm::Right
        };
        self.living
            .entity
            .entity_data
            .set(DATA_PLAYER_MAIN_HAND, arm);
        let pose = input
            .get_string("pose")
            .and_then(|n| VALID_POSES.iter().find(|(_, name)| *name == n))
            .map_or(Pose::Standing, |(p, _)| *p);
        self.set_pose(pose);
        let immovable = input.get_boolean_or("immovable", false);
        self.living
            .entity
            .entity_data
            .set(DATA_IMMOVABLE, immovable);
        self.hide_description = input.get_boolean_or("hide_description", false);
        self.update_description();
        self.description = input
            .get("description")
            .and_then(|t| Component::from_nbt(t).ok())
            .unwrap_or_else(default_description);
        self.update_description();
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static MANNEQUIN_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::MANNEQUIN,
    create: |ty, id, _registries| Box::new(Mannequin::new(ty, id)),
};
