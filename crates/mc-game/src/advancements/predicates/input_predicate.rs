//! `net.minecraft.advancements.predicates.InputPredicate`.

use mc_dfu::{Codec, BOOL};
use mc_protocol::packets::play::Input;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct InputPredicate {
    pub forward: Option<bool>,
    pub backward: Option<bool>,
    pub left: Option<bool>,
    pub right: Option<bool>,
    pub jump: Option<bool>,
    pub sneak: Option<bool>,
    pub sprint: Option<bool>,
}

impl InputPredicate {
    pub fn codec() -> Codec<InputPredicate> {
        static C: LazyLock<Codec<InputPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(InputPredicate {
                forward: BOOL.optional_field_of("forward"),
                backward: BOOL.optional_field_of("backward"),
                left: BOOL.optional_field_of("left"),
                right: BOOL.optional_field_of("right"),
                jump: BOOL.optional_field_of("jump"),
                sneak: BOOL.optional_field_of("sneak"),
                sprint: BOOL.optional_field_of("sprint"),
            })
            .codec()
        });
        C.clone()
    }

    pub fn matches(&self, input: &Input) -> bool {
        let m = |want: Option<bool>, value: bool| want.is_none_or(|b| b == value);
        m(self.forward, input.forward)
            && m(self.backward, input.backward)
            && m(self.left, input.left)
            && m(self.right, input.right)
            && m(self.jump, input.jump)
            && m(self.sneak, input.shift)
            && m(self.sprint, input.sprint)
    }
}
