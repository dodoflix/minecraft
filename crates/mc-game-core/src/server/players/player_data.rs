//! Player data (`players/data/<uuid>.dat`) glue of `PlayerList`/`PrepareSpawnTask`: loading the file and reading the
//! position part of it before the `ServerPlayer` exists. The player's NBT itself (`ServerPlayer.saveWithoutId` /
//! `load`) is written and read by the entity (`world::entity::player`); storage is `PlayerDataStorage`.

use super::name_and_id::NameAndId;
use super::player_list::PlayerList;
use crate::world::entity::respawn::SavedPosition;
use mc_nbt::CompoundTag;

/// What `PrepareSpawnTask.start` reads from the player file.
pub struct LoadedPlayerData {
    /// The whole compound (`None` for a first-time player).
    pub data: Option<CompoundTag>,
    /// `ServerPlayer.SavedPosition` (`EMPTY` for a first-time player or an unreadable position).
    pub position: SavedPosition,
}

impl PlayerList {
    /// `loadPlayerData(nameAndId)` + `tag.read(SavedPosition.MAP_CODEC).orElse(EMPTY)`.
    pub fn load_player_data_with_position(&self, name_and_id: &NameAndId) -> LoadedPlayerData {
        let data = self.load_player_data(name_and_id);
        let position = data.as_ref().map_or(SavedPosition::EMPTY, SavedPosition::read);
        LoadedPlayerData { data, position }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::notifications::EmptyNotificationService;
    use crate::server::players::player_list::PlayerListSettings;
    use mc_core::phys::Vec3;
    use mc_nbt::Tag;
    use mc_world::level_storage::PlayerDataStorage;
    use std::sync::Arc;

    #[test]
    fn first_join_has_no_data_and_saved_position_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let io = PlayerDataStorage::new(dir.path().join("players/data"));
        let list = PlayerList::new(
            PlayerListSettings::default(),
            io.clone(),
            dir.path().join("players/stats"),
            dir.path().join("players/advancements"),
            Arc::new(EmptyNotificationService),
        );
        let steve = NameAndId::create_offline("Steve");
        let first = list.load_player_data_with_position(&steve);
        assert!(first.data.is_none() && first.position == SavedPosition::EMPTY);

        let mut tag = CompoundTag::new();
        tag.put_string("Dimension", "minecraft:the_nether");
        tag.put("Pos", Tag::List([Tag::double(1.0), Tag::double(70.5), Tag::double(-8.25)].into_iter().collect()));
        tag.put("Rotation", Tag::List([Tag::float(90.0), Tag::float(-10.0)].into_iter().collect()));
        io.save(&steve.id, tag).unwrap();
        let again = list.load_player_data_with_position(&steve);
        assert_eq!(again.position.position, Some(Vec3::new(1.0, 70.5, -8.25)));
        assert_eq!(again.position.dimension.unwrap().to_string(), "minecraft:the_nether");
        assert_eq!(again.data.unwrap().get_int("DataVersion"), Some(5023));
    }
}
