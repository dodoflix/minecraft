use crate::components::component::{ComponentDef, ComponentKey};
use mc_data::DataComponentType;
use mc_dfu::extra::{NON_NEGATIVE_INT, POSITIVE_INT};
use mc_dfu::INT;

pub const MAX_STACK_SIZE: ComponentKey<i32> = ComponentKey::new(DataComponentType::MAX_STACK_SIZE);
pub const MAX_DAMAGE: ComponentKey<i32> = ComponentKey::new(DataComponentType::MAX_DAMAGE);
pub const DAMAGE: ComponentKey<i32> = ComponentKey::new(DataComponentType::DAMAGE);
pub const UNBREAKABLE: ComponentKey<()> = ComponentKey::new(DataComponentType::UNBREAKABLE);

pub(crate) fn defs(out: &mut Vec<ComponentDef>) {
    let _ = INT.clone();
    out.push(
        ComponentDef::builder(MAX_STACK_SIZE)
            .persistent(mc_dfu::extra::extra_int_range(1, 99))
            .network_with(|v, out| crate::codec::write_varint(out, *v), |i| crate::codec::read_varint(i))
            .build(),
    );
    out.push(
        ComponentDef::builder(MAX_DAMAGE)
            .persistent(POSITIVE_INT.clone())
            .network_with(|v, out| crate::codec::write_varint(out, *v), |i| crate::codec::read_varint(i))
            .build(),
    );
    out.push(
        ComponentDef::builder(DAMAGE)
            .persistent(NON_NEGATIVE_INT.clone())
            .network_with(|v, out| crate::codec::write_varint(out, *v), |i| crate::codec::read_varint(i))
            .build(),
    );
    out.push(
        ComponentDef::builder(UNBREAKABLE).persistent(mc_dfu::UNIT.clone()).network_with(|_, _| {}, |_| Ok(())).build(),
    );
}
