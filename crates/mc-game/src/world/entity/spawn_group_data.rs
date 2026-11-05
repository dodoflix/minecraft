//! `net.minecraft.world.entity.SpawnGroupData`: state shared by the mobs of one natural spawn group
//! (`finalizeSpawn` receives the group data of the previous mob of the group and returns the one for the next).

use std::any::Any;

pub trait SpawnGroupData: Any + Send {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Implements [`SpawnGroupData`] for a concrete group data type.
#[macro_export]
macro_rules! impl_spawn_group_data {
    ($ty:ty) => {
        impl $crate::world::entity::spawn_group_data::SpawnGroupData for $ty {
            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
                self
            }
        }
    };
}

/// `new SpawnGroupData() {}`: the plain marker group data several mobs use.
#[derive(Default)]
pub struct PlainSpawnGroupData;

impl_spawn_group_data!(PlainSpawnGroupData);
