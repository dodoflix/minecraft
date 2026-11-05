//! `net.minecraft.world.level.block.PoweredBlock` (redstone block): `isSignalSource` is static data.
use super::prelude::*;

pub struct PoweredBlock;

impl BlockBehaviour for PoweredBlock {
    fn own_signal(&self, _state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        15
    }
}

register_block_class!("PoweredBlock", |_| Box::new(PoweredBlock));
