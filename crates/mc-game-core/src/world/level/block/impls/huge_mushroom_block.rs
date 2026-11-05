//! `net.minecraft.world.level.block.HugeMushroomBlock`.
use super::prelude::*;

pub struct HugeMushroomBlock;

fn map_faces(state: BlockState, map: impl Fn(Direction) -> Direction) -> BlockState {
    use helpers::pipe_property as p;
    state
        .set(p(map(Direction::North)), state.get(props::NORTH))
        .set(p(map(Direction::South)), state.get(props::SOUTH))
        .set(p(map(Direction::East)), state.get(props::EAST))
        .set(p(map(Direction::West)), state.get(props::WEST))
        .set(p(map(Direction::Up)), state.get(props::UP))
        .set(p(map(Direction::Down)), state.get(props::DOWN))
}

impl BlockBehaviour for HugeMushroomBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if neighbour_state.is_block(state.block()) { state.set(helpers::pipe_property(direction), false) } else { state }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        map_faces(state, |d| rotation.rotate(d))
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        map_faces(state, |d| mirror.mirror(d))
    }
}

register_block_class!("HugeMushroomBlock", |_| Box::new(HugeMushroomBlock));
