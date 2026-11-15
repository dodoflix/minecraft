//! The non-mob entities worldgen structures place (`ServerLevelAccessor.addFreshEntity`) as the entity NBT
//! `ProtoChunk.addEntity` stores.

use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, generate_unique_seed};
use mc_core::uuid_util::uuid_to_int_array;
use mc_nbt::{CompoundTag, ListTag};

/// `Entity.UUID`: random in vanilla as well (`Mth.createInsecureUUID` over a fresh random).
pub fn random_uuid() -> Vec<i32> {
    let mut random = LegacyRandomSource::new(generate_unique_seed());
    uuid_to_int_array(&mth::create_insecure_uuid(&mut random)).to_vec()
}

fn double_list(values: [f64; 3]) -> ListTag {
    let mut list = ListTag::new();
    for v in values {
        list.push(v);
    }
    list
}

/// `new ItemFrame(level, pos, direction)` + `setItem(new ItemStack(Items.ELYTRA), false)`, as the saved NBT
/// (`HangingEntity`/`ItemFrame.addAdditionalSaveData`; `direction` is horizontal).
pub fn elytra_item_frame(pos: BlockPos, direction: Direction) -> CompoundTag {
    // `createBoundingBox`: `AABB.ofSize(atCenterOf(pos).relative(direction, -0.46875), ...)`, its centre is the entity position.
    let center = |c: i32, step: i32, size: f64| {
        let middle = c as f64 + 0.5 + f64::from(step) * -0.46875;
        let (min, max) = (middle - size / 2.0, middle + size / 2.0);
        min + (max - min) * 0.5
    };
    let size = |axis: Axis| {
        if direction.get_axis() == axis {
            0.0625
        } else {
            f64::from(0.75f32)
        }
    };
    let mut tag = CompoundTag::new();
    tag.put_string("id", "minecraft:item_frame");
    tag.put(
        "Pos",
        double_list([
            center(pos.x(), direction.get_step_x(), size(Axis::X)),
            center(pos.y(), direction.get_step_y(), size(Axis::Y)),
            center(pos.z(), direction.get_step_z(), size(Axis::Z)),
        ]),
    );
    tag.put("Motion", double_list([0.0; 3]));
    let mut rotation = ListTag::new();
    rotation.push((direction.get_2d_data_value() * 90) as f32);
    rotation.push(0.0f32);
    tag.put("Rotation", rotation);
    tag.put_double("fall_distance", 0.0);
    tag.put_short("Fire", 0);
    tag.put_short("Air", 300);
    tag.put_boolean("OnGround", false);
    tag.put_boolean("Invulnerable", false);
    tag.put_int("PortalCooldown", 0);
    tag.put_int_array("UUID", random_uuid());
    tag.put_int_array("block_pos", vec![pos.x(), pos.y(), pos.z()]);
    let mut item = CompoundTag::new();
    item.put_int("count", 1);
    item.put_string("id", "minecraft:elytra");
    tag.put("Item", item);
    tag.put_byte("ItemRotation", 0);
    tag.put_float("ItemDropChance", 1.0);
    tag.put_byte("Facing", direction.get_3d_data_value() as i8);
    tag.put_boolean("Invisible", false);
    tag.put_boolean("Fixed", false);
    tag
}
