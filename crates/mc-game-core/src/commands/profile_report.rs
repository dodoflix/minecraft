//! `FetchProfileCommand.reportResolvedProfile` / `printForAvatar`: the clickable profile line of `/fetchprofile` and of
//! a creative pick-block on an avatar.
use super::CommandSourceStack;
use crate::translatable;
use crate::world::entity::decoration::mannequin::Mannequin;
use crate::world::entity::{Entity, ServerPlayer};
use mc_protocol::codec::{Either, GameProfile};
use mc_protocol::components::{PlayerModelType, PlayerSkinPatch, ResolvableProfile};
use mc_text::component::ObjectInfo;
use mc_text::profile::{
    ProfileContents, ProfileProperty, ResolvableProfile as TextProfile, SkinPatch,
};
use mc_text::util::{format_list, space, wrap_in_square_brackets};
use mc_text::{ChatFormatting, ClickEvent, Component};

fn text_properties(properties: &[mc_protocol::codec::Property]) -> Vec<ProfileProperty> {
    properties
        .iter()
        .map(|p| ProfileProperty {
            name: p.name.clone(),
            value: p.value.clone(),
            signature: p.signature.clone(),
        })
        .collect()
}

/// The chat component's profile (`PlayerSprite.profile`) of a `ResolvableProfile`; a partial with exactly one of
/// name/id and no properties is `Dynamic`, as `ResolvableProfile.create` makes it.
fn text_profile(profile: &ResolvableProfile) -> TextProfile {
    let contents = match &profile.contents {
        Either::Left(p) => ProfileContents::Full {
            id: p.id.as_u128(),
            name: p.name.clone(),
            properties: text_properties(&p.properties),
        },
        Either::Right(p) => match (&p.name, p.id, p.properties.is_empty()) {
            (Some(name), None, true) => ProfileContents::Name(name.clone()),
            (None, Some(id), true) => ProfileContents::Id(id.as_u128()),
            _ => ProfileContents::Partial {
                name: p.name.clone(),
                id: p.id.map(|id| id.as_u128()),
                properties: text_properties(&p.properties),
            },
        },
    };
    let PlayerSkinPatch {
        body,
        cape,
        elytra,
        model,
    } = profile.skin_patch.clone();
    TextProfile {
        contents,
        skin_patch: SkinPatch {
            texture: body,
            cape,
            elytra,
            model: model.map(|m: PlayerModelType| m.name().to_owned()),
        },
    }
}

/// `reportResolvedProfile(sender, gameProfile, messageId, argument)`: `ResolvableProfile.createResolved(gameProfile)`.
pub fn report_resolved_game_profile(
    source: &CommandSourceStack,
    profile: &GameProfile,
    message_id: &'static str,
    argument: Component,
) {
    report_resolved_profile(
        source,
        message_id,
        argument,
        &ResolvableProfile {
            contents: Either::Left(profile.clone()),
            skin_patch: PlayerSkinPatch::default(),
        },
    );
}

/// `reportResolvedProfile(sender, messageId, argument, profile)`.
pub fn report_resolved_profile(
    source: &CommandSourceStack,
    message_id: &'static str,
    argument: Component,
    profile: &ResolvableProfile,
) {
    // `ResolvableProfile.CODEC.encodeStart(NbtOps.INSTANCE, profile)`
    let encoded_profile = match ResolvableProfile::codec()
        .encode_start(&mc_dfu::NBT, profile)
        .into_result()
        .and_then(|d| d.to_tag())
    {
        Ok(tag) => tag.to_string(),
        Err(error) => {
            return source.send_failure(translatable!(
                "commands.fetchprofile.failed_to_serialize",
                error
            ));
        }
    };
    let head = Component::object(
        ObjectInfo::PlayerSprite {
            player: Box::new(text_profile(profile)),
            hat: true,
        },
        None,
    );
    // `ComponentSerialization.CODEC.encodeStart(NbtOps.INSTANCE, headComponent)` (before the white style is applied)
    let encoded_component = match mc_text::codec::encode_component::<mc_nbt::Tag>(&head) {
        Ok(tag) => tag.to_string(),
        Err(error) => {
            return source.send_failure(translatable!(
                "commands.fetchprofile.failed_to_serialize",
                error.0
            ));
        }
    };
    source.send_success(
        || {
            let click = |key: &'static str, event: ClickEvent| {
                translatable!(key).with_style_fn(|s| s.with_click_event(Some(event)))
            };
            let entries = vec![
                click(
                    "commands.fetchprofile.copy_component",
                    ClickEvent::CopyToClipboard(encoded_profile.clone()),
                ),
                click(
                    "commands.fetchprofile.give_item",
                    ClickEvent::RunCommand(format!(
                        "give @s minecraft:player_head[profile={encoded_profile}]"
                    )),
                ),
                click(
                    "commands.fetchprofile.summon_mannequin",
                    ClickEvent::RunCommand(format!(
                        "summon minecraft:mannequin ~ ~ ~ {{profile:{encoded_profile}}}"
                    )),
                ),
                translatable!(
                    "commands.fetchprofile.copy_text",
                    head.clone().with_format(ChatFormatting::White)
                )
                .with_style_fn(|s| {
                    s.with_click_event(Some(ClickEvent::CopyToClipboard(encoded_component.clone())))
                }),
            ];
            let clickable = format_list(entries, &space(), |c| {
                wrap_in_square_brackets(c.with_format(ChatFormatting::Green))
            });
            translatable!(message_id, argument.clone(), clickable)
        },
        false,
    );
}

/// `Avatar.getProfile()` (`None`: the entity is not an `Avatar`, which is a player or a mannequin).
pub fn avatar_profile(entity: &dyn Entity) -> Option<ResolvableProfile> {
    let any = entity.as_any();
    if let Some(player) = any.downcast_ref::<ServerPlayer>() {
        Some(ResolvableProfile {
            contents: Either::Left(player.player.game_profile.clone()),
            skin_patch: PlayerSkinPatch::default(),
        })
    } else {
        any.downcast_ref::<Mannequin>().map(Mannequin::get_profile)
    }
}

/// `FetchProfileCommand.printForAvatar(source, avatar)` for an entity `avatar_profile` accepted.
pub fn print_for_avatar(
    source: &CommandSourceStack,
    display_name: Component,
    profile: &ResolvableProfile,
) {
    report_resolved_profile(
        source,
        "commands.fetchprofile.entity.success",
        display_name,
        profile,
    );
}
