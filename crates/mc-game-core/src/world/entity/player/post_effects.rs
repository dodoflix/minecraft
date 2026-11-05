//! `ServerPlayer` post effects (`/posteffect`): the list is sent on the next tick after a change.
use super::server_player::ServerPlayer;
use mc_core::ident::Identifier;

impl ServerPlayer {
    /// `addPostEffect(postEffect)`: false when already present.
    pub fn add_post_effect(&mut self, post_effect: &Identifier) -> bool {
        if self.player.post_effects.contains(post_effect) {
            false
        } else {
            self.player.post_effects.push(post_effect.clone());
            self.post_effects_dirty = true;
            true
        }
    }

    /// `clearPostEffects()`: false when there was none.
    pub fn clear_post_effects(&mut self) -> bool {
        if self.player.post_effects.is_empty() {
            false
        } else {
            self.player.post_effects.clear();
            self.post_effects_dirty = true;
            true
        }
    }

    /// `getPostEffects()`.
    pub fn get_post_effects(&self) -> &[Identifier] {
        &self.player.post_effects
    }

    /// `removePostEffect(postEffect)`: false when it was not present.
    pub fn remove_post_effect(&mut self, post_effect: &Identifier) -> bool {
        match self.player.post_effects.iter().position(|p| p == post_effect) {
            Some(index) => {
                self.player.post_effects.remove(index);
                self.post_effects_dirty = true;
                true
            }
            None => false,
        }
    }
}
