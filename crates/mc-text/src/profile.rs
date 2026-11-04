//! `net.minecraft.world.item.component.ResolvableProfile` as far as chat components need it
//! (the `player` object sprite): codec shapes and normalisation, no profile resolution.

use crate::codec::{err, identifier, map_of, opt, req, string, CodecError, Fields, R};
use crate::java_util::is_valid_player_name;
use crate::ops::{MapView, Ops};
use mc_core::ident::Identifier;

/// A GameProfile property (`name`, `value`, optional `signature`).
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProfileContents {
    /// `Dynamic` by name (what a bare player-name string decodes to).
    Name(String),
    /// `Dynamic` by UUID.
    Id(u128),
    /// `Static` with a full `GameProfile` (id + name + properties).
    Full { id: u128, name: String, properties: Vec<ProfileProperty> },
    /// `Static` with a `Partial` (any combination not covered above).
    Partial { name: Option<String>, id: Option<u128>, properties: Vec<ProfileProperty> },
}

/// `PlayerSkin.Patch`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkinPatch {
    pub texture: Option<Identifier>,
    pub cape: Option<Identifier>,
    pub elytra: Option<Identifier>,
    /// `slim` or `wide`.
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvableProfile {
    pub contents: ProfileContents,
    pub skin_patch: SkinPatch,
}

impl ResolvableProfile {
    pub fn create_unresolved(name: impl Into<String>) -> ResolvableProfile {
        ResolvableProfile { contents: ProfileContents::Name(name.into()), skin_patch: SkinPatch::default() }
    }

    /// `ResolvableProfile.name()`.
    pub fn name(&self) -> Option<&str> {
        match &self.contents {
            ProfileContents::Name(n) | ProfileContents::Full { name: n, .. } => Some(n),
            ProfileContents::Id(_) => None,
            ProfileContents::Partial { name, .. } => name.as_deref(),
        }
    }
}

fn player_name<O: Ops>(v: &O) -> R<String> {
    let s = string(v)?;
    if s.encode_utf16().count() > 16 {
        return err(format!("String too long (max 16): {s}"));
    }
    if !is_valid_player_name(&s) {
        return err(format!("Player name contained disallowed characters: '{s}'"));
    }
    Ok(s)
}

/// `UUIDUtil.CODEC`: exactly four ints (`[I;..]` or a list of numbers).
pub(crate) fn uuid_from_ints<O: Ops>(v: &O) -> R<u128> {
    let items = v.get_list().ok_or_else(|| CodecError("Not a list".into()))?;
    let ints: Vec<i32> = items.iter().map(|i| i.get_int().ok_or_else(|| CodecError("Not a number".into()))).collect::<R<_>>()?;
    let [a, b, c, d]: [i32; 4] = ints.try_into().map_err(|_| CodecError("Input is not a list of 4 ints".into()))?;
    let most = ((a as u64) << 32) | (b as u32 as u64);
    let least = ((c as u64) << 32) | (d as u32 as u64);
    Ok(((most as u128) << 64) | least as u128)
}

pub(crate) fn uuid_to_ints(u: u128) -> [i32; 4] {
    [(u >> 96) as i32, (u >> 64) as i32, (u >> 32) as i32, u as i32]
}

fn sized_string<O: Ops>(v: &O, max: usize) -> R<String> {
    let s = string(v)?;
    if s.encode_utf16().count() > max {
        return err(format!("String too long (max {max}): {s}"));
    }
    Ok(s)
}

fn decode_property<O: Ops>(v: &O) -> R<ProfileProperty> {
    let m = map_of(v)?;
    Ok(ProfileProperty {
        name: sized_string(req(&m, "name")?, 64)?,
        value: sized_string(req(&m, "value")?, 32767)?,
        signature: opt(&m, "signature", |s| sized_string(s, 1024))?,
    })
}

/// Groups properties by name in order of first appearance (`ImmutableMultimap` iteration order).
fn group_by_name(props: Vec<ProfileProperty>) -> Vec<ProfileProperty> {
    let mut names: Vec<String> = Vec::new();
    for p in &props {
        if !names.contains(&p.name) {
            names.push(p.name.clone());
        }
    }
    names.into_iter().flat_map(|n| props.iter().filter(move |p| p.name == n).cloned().collect::<Vec<_>>()).collect()
}

/// `ExtraCodecs.PROPERTY_MAP`: a `name -> [values]` map (<= 16 names) or a list (<= 16) of properties.
fn decode_property_map<O: Ops>(v: &O) -> R<Vec<ProfileProperty>> {
    let from_map = || -> R<Vec<ProfileProperty>> {
        let m = map_of(v)?;
        if m.len() > 16 {
            return err(format!("Cannot have more than 16 properties, but was {}", m.len()));
        }
        let mut out = Vec::new();
        for (name, values) in m {
            let items = values.get_list().ok_or_else(|| CodecError("Not a list".into()))?;
            for item in items {
                out.push(ProfileProperty { name: name.to_owned(), value: string(item.as_ref())?, signature: None });
            }
        }
        Ok(out)
    };
    if let Ok(p) = from_map() {
        return Ok(group_by_name(p));
    }
    let items = v.get_list().ok_or_else(|| CodecError("Not a list".into()))?;
    if items.len() > 16 {
        return err(format!("List is too long: {}, expected range [0-16]", items.len()));
    }
    Ok(group_by_name(items.iter().map(|i| decode_property(i.as_ref())).collect::<R<_>>()?))
}

fn encode_properties<O: Ops>(props: &[ProfileProperty], f: &mut Fields<O>) {
    if props.is_empty() {
        return;
    }
    let items = props
        .iter()
        .map(|p| {
            let mut e = vec![("name".to_owned(), O::create_string(&p.name)), ("value".to_owned(), O::create_string(&p.value))];
            if let Some(s) = &p.signature {
                e.push(("signature".to_owned(), O::create_string(s)));
            }
            O::create_map(e)
        })
        .collect();
    f.push(("properties".to_owned(), O::create_list(items)));
}

fn decode_full_profile<O: Ops>(v: &O) -> R<ResolvableProfile> {
    let m = map_of(v)?;
    let properties = |m: &MapView<'_, O>| opt(m, "properties", |p| decode_property_map(p)).map(Option::unwrap_or_default);
    // Codec.mapEither(STORED_GAME_PROFILE, Partial.MAP_CODEC)
    let stored = (|| -> R<ProfileContents> {
        Ok(ProfileContents::Full {
            id: uuid_from_ints(req(&m, "id")?)?,
            name: player_name(req(&m, "name")?)?,
            properties: properties(&m)?,
        })
    })();
    let contents = match stored {
        Ok(c) => c,
        Err(_) => {
            let name = opt(&m, "name", |n| player_name(n))?;
            let id = opt(&m, "id", |i| uuid_from_ints(i))?;
            let properties = properties(&m)?;
            match (properties.is_empty() && name.is_some() != id.is_some(), name, id) {
                (true, Some(name), _) => ProfileContents::Name(name),
                (true, None, Some(id)) => ProfileContents::Id(id),
                (_, name, id) => ProfileContents::Partial { name, id, properties },
            }
        }
    };
    let model = opt(&m, "model", |s| match string(s)?.as_str() {
        m @ ("slim" | "wide") => Ok(m.to_owned()),
        other => err(format!("Unknown element name:{other}")),
    })?;
    let skin_patch = SkinPatch {
        texture: opt(&m, "texture", |t| identifier(t))?,
        cape: opt(&m, "cape", |t| identifier(t))?,
        elytra: opt(&m, "elytra", |t| identifier(t))?,
        model,
    };
    Ok(ResolvableProfile { contents, skin_patch })
}

/// `ResolvableProfile.CODEC`: the full record, or (alternatively) a bare player name.
pub(crate) fn decode_profile<O: Ops>(v: &O) -> R<ResolvableProfile> {
    match decode_full_profile(v) {
        Ok(p) => Ok(p),
        Err(e) => player_name(v).map(ResolvableProfile::create_unresolved).map_err(|_| e),
    }
}

pub(crate) fn encode_profile<O: Ops>(p: &ResolvableProfile) -> O {
    let mut f: Fields<O> = Vec::new();
    match &p.contents {
        ProfileContents::Name(n) => f.push(("name".into(), O::create_string(n))),
        ProfileContents::Id(id) => f.push(("id".into(), O::create_int_list(&uuid_to_ints(*id)))),
        ProfileContents::Full { id, name, properties } => {
            f.push(("id".into(), O::create_int_list(&uuid_to_ints(*id))));
            f.push(("name".into(), O::create_string(name)));
            encode_properties(properties, &mut f);
        }
        ProfileContents::Partial { name, id, properties } => {
            if let Some(n) = name {
                f.push(("name".into(), O::create_string(n)));
            }
            if let Some(id) = id {
                f.push(("id".into(), O::create_int_list(&uuid_to_ints(*id))));
            }
            encode_properties(properties, &mut f);
        }
    }
    let s = &p.skin_patch;
    for (key, v) in [("texture", &s.texture), ("cape", &s.cape), ("elytra", &s.elytra)] {
        if let Some(id) = v {
            f.push((key.into(), O::create_string(&id.to_string())));
        }
    }
    if let Some(m) = &s.model {
        f.push(("model".into(), O::create_string(m)));
    }
    O::create_map(f)
}
