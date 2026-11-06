//! NBT for the vanilla 26.3 protocol and Anvil files: `net.minecraft.nbt` in Rust.
//!
//! * [`Tag`], [`CompoundTag`], [`ListTag`]: the tag model (`Optional` getters become `Option`).
//! * [`nbt_io`]: binary IO (named / network roots, accounter limits); [`compress`]: gzip & zlib.
//! * [`snbt`]: SNBT parsing (vanilla's packrat `SnbtGrammar`, same errors and cursor positions)
//!   and printing (compact `StringTagVisitor` and pretty `SnbtPrinterTagVisitor`).

pub mod accounter;
mod compound;
pub mod compress;
pub mod error;
mod file;
mod io;
mod list;
pub mod nbt_utils;
pub mod snbt;
pub mod stream;
mod tag;

pub use accounter::NbtAccounter;
pub use compound::CompoundTag;
pub use error::NbtError;
pub use list::ListTag;
pub use tag::*;

/// Vanilla's `NbtIo` (same function names, byte-slice based).
pub mod nbt_io {
    pub use crate::compress::{read_compressed, write_compressed};
    pub use crate::file::{read_compressed_file, read_file, write_compressed_file, write_file};
    pub use crate::io::{
        read, read_any_tag, read_unnamed_tag, to_bytes, write, write_any_tag, write_unnamed_tag,
        write_unnamed_tag_with_fallback,
    };
}

/// File root: `read_named(&mut bytes, &mut accounter)` (see [`nbt_io::read`]).
pub use io::read as read_named;
/// Network root: type id + payload (see [`nbt_io::read_any_tag`]).
pub use io::read_any_tag as read_network;
/// File root: named (empty name) compound.
pub use io::write as write_named;
/// Network root writer (see [`nbt_io::write_any_tag`]).
pub use io::write_any_tag as write_network;
