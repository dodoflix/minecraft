//! `ClientboundCommandsPacket`: the command tree and the `ArgumentTypeInfo` templates of every argument type.
use crate::codec::EncodeResult;
use mc_core::ident::Identifier;
use mc_data::CommandArgumentType as T;

use crate::codec::{
    DecodeError, DecodeResult, read_count, read_registry_id, read_varint, take, write_varint,
};
use crate::{Decode, Encode, Packet};

/// `ClientboundCommandsPacket` node flag bits.
pub const COMMAND_NODE_TYPE_MASK: u8 = 3;
pub const COMMAND_NODE_TYPE_ROOT: u8 = 0;
pub const COMMAND_NODE_TYPE_LITERAL: u8 = 1;
pub const COMMAND_NODE_TYPE_ARGUMENT: u8 = 2;
pub const COMMAND_NODE_EXECUTABLE: u8 = 4;
pub const COMMAND_NODE_REDIRECT: u8 = 8;
pub const COMMAND_NODE_CUSTOM_SUGGESTIONS: u8 = 16;
pub const COMMAND_NODE_RESTRICTED: u8 = 32;

/// Brigadier `StringArgumentType.StringType` (`readEnum`: VarInt ordinal, out of range = Java `ArrayIndexOutOfBoundsException`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StringArgumentType {
    SingleWord,
    QuotablePhrase,
    GreedyPhrase,
}

impl Encode for StringArgumentType {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, *self as i32);
        Ok(())
    }
}
impl Decode for StringArgumentType {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        match read_varint(input)? {
            0 => Ok(Self::SingleWord),
            1 => Ok(Self::QuotablePhrase),
            2 => Ok(Self::GreedyPhrase),
            i => Err(DecodeError::Message(format!(
                "Index {i} out of bounds for length 3"
            ))),
        }
    }
}

/// `ArgumentTypeInfo.Template`: an argument type with the properties that go on the wire. The variants are the
/// `minecraft:command_argument_type` registry entries; those with properties carry them, the rest are bare.
/// Registry-bound arguments carry the registry key's identifier (`ResourceKey.registry()` = `readRegistryKey`).
///
/// Number types: a bound equal to the type's extreme (`Integer.MIN_VALUE`, `-Float.MAX_VALUE`, ...) is "absent" and not sent,
/// exactly like vanilla; use `i32::MIN`/`-f32::MAX`/... for "unbounded".
#[derive(Debug, Clone, PartialEq)]
pub enum ArgumentTypeTemplate {
    Bool,
    Float { min: f32, max: f32 },
    Double { min: f64, max: f64 },
    Integer { min: i32, max: i32 },
    Long { min: i64, max: i64 },
    String(StringArgumentType),
    Entity { single: bool, players_only: bool },
    GameProfile,
    BlockPos,
    ColumnPos,
    Vec3,
    Vec2,
    BlockState,
    BlockPredicate,
    ItemStack,
    ItemPredicate,
    TeamColor,
    HexColor,
    Component,
    Style,
    Message,
    NbtCompoundTag,
    NbtTag,
    NbtPath,
    Objective,
    ObjectiveCriteria,
    Operation,
    Particle,
    Angle,
    Rotation,
    ScoreboardSlot,
    ScoreHolder { multiple: bool },
    Swizzle,
    Team,
    ItemSlot,
    ItemSlots,
    ResourceLocation,
    Function,
    EntityAnchor,
    IntRange,
    FloatRange,
    Dimension,
    Gamemode,
    Time { min: i32 },
    ResourceOrTag(Identifier),
    ResourceOrTagKey(Identifier),
    Resource(Identifier),
    ResourceKey(Identifier),
    ResourceSelector(Identifier),
    TemplateMirror,
    TemplateRotation,
    Heightmap,
    LootTable,
    LootPredicate,
    LootModifier,
    ContextFloatProvider,
    ContextIntProvider,
    SlotSource,
    Dialog,
    Feature,
    SwingAnimation,
    Uuid,
}

/// The argument types without properties (`SingletonArgumentInfo`), by registry entry.
const BARE: [(T, ArgumentTypeTemplate); 49] = {
    use ArgumentTypeTemplate as A;
    [
        (T::BRIGADIER_BOOL, A::Bool),
        (T::GAME_PROFILE, A::GameProfile),
        (T::BLOCK_POS, A::BlockPos),
        (T::COLUMN_POS, A::ColumnPos),
        (T::VEC3, A::Vec3),
        (T::VEC2, A::Vec2),
        (T::BLOCK_STATE, A::BlockState),
        (T::BLOCK_PREDICATE, A::BlockPredicate),
        (T::ITEM_STACK, A::ItemStack),
        (T::ITEM_PREDICATE, A::ItemPredicate),
        (T::TEAM_COLOR, A::TeamColor),
        (T::HEX_COLOR, A::HexColor),
        (T::COMPONENT, A::Component),
        (T::STYLE, A::Style),
        (T::MESSAGE, A::Message),
        (T::NBT_COMPOUND_TAG, A::NbtCompoundTag),
        (T::NBT_TAG, A::NbtTag),
        (T::NBT_PATH, A::NbtPath),
        (T::OBJECTIVE, A::Objective),
        (T::OBJECTIVE_CRITERIA, A::ObjectiveCriteria),
        (T::OPERATION, A::Operation),
        (T::PARTICLE, A::Particle),
        (T::ANGLE, A::Angle),
        (T::ROTATION, A::Rotation),
        (T::SCOREBOARD_SLOT, A::ScoreboardSlot),
        (T::SWIZZLE, A::Swizzle),
        (T::TEAM, A::Team),
        (T::ITEM_SLOT, A::ItemSlot),
        (T::ITEM_SLOTS, A::ItemSlots),
        (T::RESOURCE_LOCATION, A::ResourceLocation),
        (T::FUNCTION, A::Function),
        (T::ENTITY_ANCHOR, A::EntityAnchor),
        (T::INT_RANGE, A::IntRange),
        (T::FLOAT_RANGE, A::FloatRange),
        (T::DIMENSION, A::Dimension),
        (T::GAMEMODE, A::Gamemode),
        (T::TEMPLATE_MIRROR, A::TemplateMirror),
        (T::TEMPLATE_ROTATION, A::TemplateRotation),
        (T::HEIGHTMAP, A::Heightmap),
        (T::LOOT_TABLE, A::LootTable),
        (T::LOOT_PREDICATE, A::LootPredicate),
        (T::LOOT_MODIFIER, A::LootModifier),
        (T::CONTEXT_FLOAT_PROVIDER, A::ContextFloatProvider),
        (T::CONTEXT_INT_PROVIDER, A::ContextIntProvider),
        (T::SLOT_SOURCE, A::SlotSource),
        (T::DIALOG, A::Dialog),
        (T::FEATURE, A::Feature),
        (T::SWING_ANIMATION, A::SwingAnimation),
        (T::UUID, A::Uuid),
    ]
};

/// `ArgumentUtils.createNumberFlags` + the present bounds.
fn encode_bounds<N: Encode + PartialEq>(
    out: &mut Vec<u8>,
    min: &N,
    max: &N,
    lowest: N,
    highest: N,
) -> EncodeResult<()> {
    let (has_min, has_max) = (*min != lowest, *max != highest);
    out.push(u8::from(has_min) | u8::from(has_max) << 1);
    if has_min {
        min.encode(out)?;
    }
    if has_max {
        max.encode(out)?;
    };
    Ok(())
}

fn decode_bounds<N: Decode>(input: &mut &[u8], lowest: N, highest: N) -> DecodeResult<(N, N)> {
    let flags = take(input, 1)?[0];
    let min = if flags & 1 != 0 {
        N::decode(input)?
    } else {
        lowest
    };
    let max = if flags & 2 != 0 {
        N::decode(input)?
    } else {
        highest
    };
    Ok((min, max))
}

impl ArgumentTypeTemplate {
    /// The registry entry (the id sent before the properties).
    pub fn argument_type(&self) -> T {
        use ArgumentTypeTemplate as A;
        match self {
            A::Float { .. } => T::BRIGADIER_FLOAT,
            A::Double { .. } => T::BRIGADIER_DOUBLE,
            A::Integer { .. } => T::BRIGADIER_INTEGER,
            A::Long { .. } => T::BRIGADIER_LONG,
            A::String(_) => T::BRIGADIER_STRING,
            A::Entity { .. } => T::ENTITY,
            A::ScoreHolder { .. } => T::SCORE_HOLDER,
            A::Time { .. } => T::TIME,
            A::ResourceOrTag(_) => T::RESOURCE_OR_TAG,
            A::ResourceOrTagKey(_) => T::RESOURCE_OR_TAG_KEY,
            A::Resource(_) => T::RESOURCE,
            A::ResourceKey(_) => T::RESOURCE_KEY,
            A::ResourceSelector(_) => T::RESOURCE_SELECTOR,
            bare => {
                BARE.iter()
                    .find(|(_, t)| t == bare)
                    .expect("every property-free variant is in BARE")
                    .0
            }
        }
    }

    /// `ArgumentTypeInfo.deserializeFromNetwork` for the given registry entry.
    pub fn decode_properties(ty: T, input: &mut &[u8]) -> DecodeResult<Self> {
        use ArgumentTypeTemplate as A;
        Ok(match ty {
            T::BRIGADIER_FLOAT => {
                let (min, max) = decode_bounds(input, -f32::MAX, f32::MAX)?;
                A::Float { min, max }
            }
            T::BRIGADIER_DOUBLE => {
                let (min, max) = decode_bounds(input, -f64::MAX, f64::MAX)?;
                A::Double { min, max }
            }
            T::BRIGADIER_INTEGER => {
                let (min, max) = decode_bounds(input, i32::MIN, i32::MAX)?;
                A::Integer { min, max }
            }
            T::BRIGADIER_LONG => {
                let (min, max) = decode_bounds(input, i64::MIN, i64::MAX)?;
                A::Long { min, max }
            }
            T::BRIGADIER_STRING => A::String(StringArgumentType::decode(input)?),
            T::ENTITY => {
                let flags = take(input, 1)?[0];
                A::Entity {
                    single: flags & 1 != 0,
                    players_only: flags & 2 != 0,
                }
            }
            T::SCORE_HOLDER => A::ScoreHolder {
                multiple: take(input, 1)?[0] & 1 != 0,
            },
            T::TIME => A::Time {
                min: i32::decode(input)?,
            },
            T::RESOURCE_OR_TAG => A::ResourceOrTag(Identifier::decode(input)?),
            T::RESOURCE_OR_TAG_KEY => A::ResourceOrTagKey(Identifier::decode(input)?),
            T::RESOURCE => A::Resource(Identifier::decode(input)?),
            T::RESOURCE_KEY => A::ResourceKey(Identifier::decode(input)?),
            T::RESOURCE_SELECTOR => A::ResourceSelector(Identifier::decode(input)?),
            bare => BARE
                .iter()
                .find(|(t, _)| *t == bare)
                .expect("every other registry entry is in BARE")
                .1
                .clone(),
        })
    }

    /// `ArgumentTypeInfo.serializeToNetwork` (the properties only, not the registry id).
    pub fn encode_properties(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        use ArgumentTypeTemplate as A;
        match self {
            A::Float { min, max } => encode_bounds(out, min, max, -f32::MAX, f32::MAX)?,
            A::Double { min, max } => encode_bounds(out, min, max, -f64::MAX, f64::MAX)?,
            A::Integer { min, max } => encode_bounds(out, min, max, i32::MIN, i32::MAX)?,
            A::Long { min, max } => encode_bounds(out, min, max, i64::MIN, i64::MAX)?,
            A::String(t) => t.encode(out)?,
            A::Entity {
                single,
                players_only,
            } => out.push(u8::from(*single) | u8::from(*players_only) << 1),
            A::ScoreHolder { multiple } => out.push(u8::from(*multiple)),
            A::Time { min } => min.encode(out)?,
            A::ResourceOrTag(r)
            | A::ResourceOrTagKey(r)
            | A::Resource(r)
            | A::ResourceKey(r)
            | A::ResourceSelector(r) => r.encode(out)?,
            _ => {}
        };
        Ok(())
    }
}

/// `ClientboundCommandsPacket.NodeStub`: the name part of a literal / argument node.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientboundCommandsNodeStub {
    Literal(String),
    Argument {
        id: String,
        argument_type: ArgumentTypeTemplate,
        suggestion_id: Option<Identifier>,
    },
}

impl Encode for ClientboundCommandsNodeStub {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        match self {
            Self::Literal(id) => id.encode(out)?,
            Self::Argument {
                id,
                argument_type,
                suggestion_id,
            } => {
                id.encode(out)?;
                argument_type.argument_type().encode(out)?;
                argument_type.encode_properties(out)?;
                if let Some(s) = suggestion_id {
                    s.encode(out)?;
                }
            }
        };
        Ok(())
    }
}

/// `ClientboundCommandsPacket.Entry`: one node of the tree, in vanilla's raw form (`flags` byte, redirect only meaningful
/// with [`COMMAND_NODE_REDIRECT`]). `stub` is `None` for the root and for what vanilla reads as such (node type `0`/`3`,
/// or an argument node whose argument type id is unknown: vanilla then does not read its properties).
#[derive(Debug, Clone, PartialEq)]
pub struct ClientboundCommandsEntry {
    pub stub: Option<ClientboundCommandsNodeStub>,
    pub flags: u8,
    pub redirect: i32,
    pub children: Vec<i32>,
}

impl ClientboundCommandsEntry {
    pub fn root(children: Vec<i32>) -> Self {
        Self {
            stub: None,
            flags: COMMAND_NODE_TYPE_ROOT,
            redirect: 0,
            children,
        }
    }

    pub fn literal(name: impl Into<String>, children: Vec<i32>) -> Self {
        Self {
            stub: Some(ClientboundCommandsNodeStub::Literal(name.into())),
            flags: COMMAND_NODE_TYPE_LITERAL,
            redirect: 0,
            children,
        }
    }

    pub fn argument(
        name: impl Into<String>,
        argument_type: ArgumentTypeTemplate,
        suggestion_id: Option<Identifier>,
        children: Vec<i32>,
    ) -> Self {
        let suggestions = if suggestion_id.is_some() {
            COMMAND_NODE_CUSTOM_SUGGESTIONS
        } else {
            0
        };
        Self {
            stub: Some(ClientboundCommandsNodeStub::Argument {
                id: name.into(),
                argument_type,
                suggestion_id,
            }),
            flags: COMMAND_NODE_TYPE_ARGUMENT | suggestions,
            redirect: 0,
            children,
        }
    }

    pub fn executable(mut self) -> Self {
        self.flags |= COMMAND_NODE_EXECUTABLE;
        self
    }

    pub fn restricted(mut self) -> Self {
        self.flags |= COMMAND_NODE_RESTRICTED;
        self
    }

    pub fn redirect_to(mut self, index: i32) -> Self {
        self.flags |= COMMAND_NODE_REDIRECT;
        self.redirect = index;
        self
    }
}

impl Encode for ClientboundCommandsEntry {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.flags.encode(out)?;
        write_varint(out, self.children.len() as i32);
        for c in &self.children {
            write_varint(out, *c);
        }
        if self.flags & COMMAND_NODE_REDIRECT != 0 {
            write_varint(out, self.redirect);
        }
        if let Some(stub) = &self.stub {
            stub.encode(out)?;
        };
        Ok(())
    }
}

impl Decode for ClientboundCommandsEntry {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let flags = u8::decode(input)?;
        // `readVarIntArray()`: at most as many elements as bytes are left.
        let max = input.len();
        let size = read_varint(input)?;
        if size as i64 > max as i64 {
            return Err(DecodeError::Message(format!(
                "VarIntArray with size {size} is bigger than allowed {max}"
            )));
        }
        let size = usize::try_from(size).map_err(|_| DecodeError::Message(size.to_string()))?;
        let mut children = Vec::with_capacity(size.min(65536));
        for _ in 0..size {
            children.push(read_varint(input)?);
        }
        let redirect = if flags & COMMAND_NODE_REDIRECT != 0 {
            read_varint(input)?
        } else {
            0
        };
        let stub = match flags & COMMAND_NODE_TYPE_MASK {
            COMMAND_NODE_TYPE_ARGUMENT => {
                let id = String::decode(input)?;
                match read_registry_id::<T>(input) {
                    Ok(ty) => {
                        let argument_type = ArgumentTypeTemplate::decode_properties(ty, input)?;
                        let suggestion_id = if flags & COMMAND_NODE_CUSTOM_SUGGESTIONS != 0 {
                            Some(Identifier::decode(input)?)
                        } else {
                            None
                        };
                        Some(ClientboundCommandsNodeStub::Argument {
                            id,
                            argument_type,
                            suggestion_id,
                        })
                    }
                    // `COMMAND_ARGUMENT_TYPE.byId` is null for an unknown id; vanilla keeps decoding with no stub.
                    Err(DecodeError::Message(_)) => None,
                    Err(e) => return Err(e),
                }
            }
            COMMAND_NODE_TYPE_LITERAL => {
                Some(ClientboundCommandsNodeStub::Literal(String::decode(input)?))
            }
            _ => None,
        };
        Ok(Self {
            stub,
            flags,
            redirect,
            children,
        })
    }
}

/// `ClientboundCommandsPacket`. Decoding runs vanilla's `validateEntries` (no redirect loops, no child loops); indices out of
/// range are accepted there and only fail when the tree is resolved.
#[derive(Debug, Clone, PartialEq, Packet)]
#[packet(state = play, flow = clientbound, name = "commands")]
pub struct ClientboundCommandsPacket {
    pub entries: Vec<ClientboundCommandsEntry>,
    pub root_index: i32,
}

impl Encode for ClientboundCommandsPacket {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.entries.encode(out)?;
        write_varint(out, self.root_index);
        Ok(())
    }
}

/// Whether the dependency graph given by `deps` (node -> nodes that must be handled first) has no cycle; dependencies
/// outside `0..n` are ignored.
fn is_acyclic(
    n: usize,
    deps: impl Fn(&ClientboundCommandsEntry, &mut dyn FnMut(usize)),
    entries: &[ClientboundCommandsEntry],
) -> bool {
    let mut pending = vec![0u32; n];
    let mut dependents = vec![Vec::new(); n];
    for (i, entry) in entries.iter().enumerate() {
        deps(entry, &mut |d| {
            if d < n {
                pending[i] += 1;
                dependents[d].push(i);
            }
        });
    }
    let mut ready: Vec<usize> = (0..n).filter(|&i| pending[i] == 0).collect();
    let mut done = 0;
    while let Some(d) = ready.pop() {
        done += 1;
        for &i in &dependents[d] {
            pending[i] -= 1;
            if pending[i] == 0 {
                ready.push(i);
            }
        }
    }
    done == n
}

impl Decode for ClientboundCommandsPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let count = read_count(input, i32::MAX as usize)?;
        let mut entries = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            entries.push(ClientboundCommandsEntry::decode(input)?);
        }
        let root_index = read_varint(input)?;
        let n = entries.len();
        // `Entry.canBuild`: a redirect must be built before the node using it.
        let buildable = is_acyclic(
            n,
            |e, f| {
                if e.flags & COMMAND_NODE_REDIRECT != 0 {
                    if let Ok(r) = usize::try_from(e.redirect) {
                        f(r)
                    }
                }
            },
            &entries,
        );
        // `Entry.canResolve`: children must be resolved before their parent.
        let resolvable = buildable
            && is_acyclic(
                n,
                |e, f| {
                    e.children
                        .iter()
                        .filter_map(|&c| usize::try_from(c).ok())
                        .for_each(f)
                },
                &entries,
            );
        if !resolvable {
            return Err(DecodeError::msg("Server sent an impossible command tree"));
        }
        Ok(Self {
            entries,
            root_index,
        })
    }
}
