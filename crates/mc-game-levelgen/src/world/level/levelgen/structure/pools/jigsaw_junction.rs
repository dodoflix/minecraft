//! `JigsawJunction`.

use mc_game_core::world::level::levelgen::structure::pools::structure_template_pool::Projection;
use mc_nbt::CompoundTag;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JigsawJunction {
    pub source_x: i32,
    pub source_ground_y: i32,
    pub source_z: i32,
    pub delta_y: i32,
    pub dest_projection: Projection,
}

impl JigsawJunction {
    pub fn new(source_x: i32, source_ground_y: i32, source_z: i32, delta_y: i32, dest_projection: Projection) -> JigsawJunction {
        JigsawJunction { source_x, source_ground_y, source_z, delta_y, dest_projection }
    }

    /// `serialize(NbtOps)`.
    pub fn serialize(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.put_int("source_x", self.source_x);
        tag.put_int("source_ground_y", self.source_ground_y);
        tag.put_int("source_z", self.source_z);
        tag.put_int("delta_y", self.delta_y);
        tag.put_string("dest_proj", self.dest_projection.get_name());
        tag
    }

    /// `deserialize(Dynamic)`: absent ints are 0; an unknown `dest_proj` is `null` in vanilla (it is never read back), `Rigid`
    /// here.
    pub fn deserialize(tag: &CompoundTag) -> JigsawJunction {
        JigsawJunction {
            source_x: tag.get_int_or("source_x", 0),
            source_ground_y: tag.get_int_or("source_ground_y", 0),
            source_z: tag.get_int_or("source_z", 0),
            delta_y: tag.get_int_or("delta_y", 0),
            dest_projection: Projection::by_name(tag.get_string_or("dest_proj", "")).unwrap_or(Projection::Rigid),
        }
    }
}
