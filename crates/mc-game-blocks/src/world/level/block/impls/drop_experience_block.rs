//! `net.minecraft.world.level.block.DropExperienceBlock`: ores drop experience after being broken.
use super::prelude::*;
use crate::registry::typed::int_provider::IntProvider;
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;

pub struct DropExperienceBlock {
    xp_range: IntProvider,
}

/// The `xpRange` constructor argument of every `DropExperienceBlock` in `Blocks`.
fn xp_range(block: Block) -> IntProvider {
    let uniform = |min_inclusive, max_inclusive| IntProvider::Uniform { min_inclusive, max_inclusive };
    match block.data().name.trim_start_matches("minecraft:").trim_start_matches("deepslate_") {
        "coal_ore" => uniform(0, 2),
        "nether_gold_ore" => uniform(0, 1),
        "lapis_ore" | "nether_quartz_ore" => uniform(2, 5),
        "diamond_ore" | "emerald_ore" => uniform(3, 7),
        // gold, iron and copper ore
        _ => IntProvider::Constant(0),
    }
}

impl BlockBehaviour for DropExperienceBlock {
    fn spawn_after_break(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, tool: &ItemStack, drop_experience: bool) {
        if drop_experience {
            helpers::try_drop_experience(level, pos, tool, &self.xp_range);
        }
    }
}

register_block_class!("DropExperienceBlock", |block| Box::new(DropExperienceBlock { xp_range: xp_range(block) }));
