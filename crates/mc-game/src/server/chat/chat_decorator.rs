//! `net.minecraft.network.chat.ChatDecorator`: 26.3 only has the identity decorator (`PLAIN`).

use mc_text::Component;

/// `ChatDecorator.PLAIN.decorate(player, plain)`.
pub fn plain(_player: Option<&crate::world::entity::ServerPlayer>, plain: Component) -> Component {
    plain
}
