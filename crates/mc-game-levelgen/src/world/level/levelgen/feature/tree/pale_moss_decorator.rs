//! `treedecorators.PaleMossDecorator`.

use super::TREE_DECORATOR_TYPES;
use mc_game_core::world::level::levelgen::feature::tree::shuffle;
use super::tree_decorator::{Context, TreeDecorator};
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_float_range};
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::props;
use std::sync::Arc;

/// `VegetationFeatures.PALE_MOSS_PATCH`.
const PALE_MOSS_PATCH: &str = "minecraft:pale_moss_patch";

pub struct PaleMossDecorator {
    leaves_probability: f32,
    trunk_probability: f32,
    ground_probability: f32,
}

fn add_moss_hanger(mut pos: BlockPos, context: &mut Context<'_>) {
    while context.is_air(pos.below()) && !(context.random.next_float() < 0.5f32) {
        context.set_block(pos, Block::PALE_HANGING_MOSS.default_state().set(props::TIP, false));
        pos = pos.below();
    }
    context.set_block(pos, Block::PALE_HANGING_MOSS.default_state().set(props::TIP, true));
}

impl TreeDecorator for PaleMossDecorator {
    fn place(&self, context: &mut Context<'_>) {
        let mut logs = context.logs.clone();
        shuffle(&mut logs, context.random);
        if logs.is_empty() {
            return;
        }
        // Collections.min: the first of the minimal elements
        let origin = logs.iter().copied().reduce(|min, pos| if pos.y() < min.y() { pos } else { min }).unwrap();
        if context.random.next_float() < self.ground_probability {
            let env = context.level.env().clone();
            let id = Identifier::parse(PALE_MOSS_PATCH).expect("valid id");
            if let Some(moss_patch) = env.registries.feature(&id) {
                moss_patch.value().place(context.level, &env, context.random, origin.above());
            }
        }

        for pos in context.logs.clone() {
            if context.random.next_float() < self.trunk_probability {
                let down = pos.below();
                if context.is_air(down) {
                    add_moss_hanger(down, context);
                }
            }
        }
        for pos in context.leaves.clone() {
            if context.random.next_float() < self.leaves_probability {
                let down = pos.below();
                if context.is_air(down) {
                    add_moss_hanger(down, context);
                }
            }
        }
    }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static PALE_MOSS: TypeEntry<dyn TreeDecorator> = TypeEntry {
    name: "pale_moss",
    decode: |fields, _| {
        let chance = |name: &str| fields.required(name, |v| parse_float_range(v, 0.0, 1.0));
        Ok(Arc::new(PaleMossDecorator {
            leaves_probability: chance("leaves_probability")?,
            trunk_probability: chance("trunk_probability")?,
            ground_probability: chance("ground_probability")?,
        }))
    },
};
