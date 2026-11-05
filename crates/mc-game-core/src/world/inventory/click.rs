//! `ContainerInput` (the click type) and `ClickAction`.

pub use mc_protocol::packets::play::ContainerInput;

/// `ClickAction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClickAction {
    Primary,
    Secondary,
}
