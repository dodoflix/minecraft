//! `MapItem` (`filled_map` and every map-class item) and `EmptyMapItem` (`map`): world rendering of a map
//! (`MapItem.update`), map creation, crafting post-processing, banner markers and the held-map ticking.

use super::{
    InteractionHand, InteractionResult, ItemBehaviour, ItemStack, ItemStackExt, PlainItem,
    UseOnContext,
};
use crate::register_item_class;
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::level::block::BlockStateExt;
use crate::world::level::material::map_color::{self, Brightness, get_packed_id};
use crate::world::level::saveddata::maps::{MapItemSavedData, MapTickEnv, PlayerView};
use crate::world::level::{BlockGetter, DimensionKey};
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_data::block::Block;
use mc_protocol::components::{MAP_ID, MAP_POST_PROCESSING, MapPostProcessing};
use mc_protocol::packets::play::level::SoundSource;
use mc_world::heightmap::HeightmapType;

pub struct MapItem;
pub struct EmptyMapItem;

register_item_class!("MapItem", |_| Box::new(MapItem));
register_item_class!("EmptyMapItem", |_| Box::new(EmptyMapItem));

/// `MapItem.create(level, originX, originZ, scale, trackPosition, unlimitedTracking)`.
pub fn create(
    level: &mut ServerLevel,
    origin_x: i32,
    origin_z: i32,
    scale: i8,
    track_position: bool,
    unlimited_tracking: bool,
) -> ItemStack {
    let mut map = ItemStack::new(Item::FILLED_MAP);
    apply_new_saved_data(
        level,
        &mut map,
        origin_x,
        origin_z,
        scale,
        track_position,
        unlimited_tracking,
    );
    map
}

/// `MapItem.applyNewSavedData(level, map, ...)`.
pub fn apply_new_saved_data(
    level: &mut ServerLevel,
    map: &mut ItemStack,
    origin_x: i32,
    origin_z: i32,
    scale: i8,
    track_position: bool,
    unlimited_tracking: bool,
) {
    let new_data = MapItemSavedData::create_fresh(
        f64::from(origin_x),
        f64::from(origin_z),
        scale,
        track_position,
        unlimited_tracking,
        level.dimension.clone(),
    );
    let id = level.get_free_map_id();
    level.set_map_data(id, new_data);
    map.set(MAP_ID, id);
}

/// `MapItem.getSavedData(itemStack, level)`: runs `f` over the saved data of the map `stack` refers to.
pub fn with_saved_data<R>(
    stack: &ItemStack,
    level: &mut ServerLevel,
    f: impl FnOnce(&mut ServerLevel, &mut MapItemSavedData) -> R,
) -> Option<R> {
    let id = *stack.get(&MAP_ID)?;
    level.with_map_data(id, f)
}

/// `MapItem.getSavedData(map, level).scale` for `MapExtendingRecipe` (installed with the server's map storage).
pub fn map_scale_lookup(level: &ServerLevel, stack: &ItemStack) -> Option<i32> {
    level.read_map_data(*stack.get(&MAP_ID)?, |data| i32::from(data.scale))
}

/// The facts of a map's saved data that menus read (`locked`, `scale`), `None` without saved data.
pub fn saved_data_info(stack: &ItemStack, level: &mut ServerLevel) -> Option<(bool, i8)> {
    with_saved_data(stack, level, |_, data| (data.locked, data.scale))
}

/// `LivingEntity.hasMapInvisibilityItemEquipped` input: whether `item` is in `#map_invisibility_equipment`.
fn is_map_invisibility_item(item: Item) -> bool {
    mc_data::tags::get::<Item>("minecraft:map_invisibility_equipment")
        .is_some_and(|tag| tag.contains(item))
}

/// The `Player` facts `tickCarriedBy` reads from the player entity `e` (`None` for non-players). `matcher` is the
/// `mapMatcher` of `getInventory().contains(..)`.
pub fn player_view(
    e: &dyn Entity,
    dimension: &DimensionKey,
    matcher: &dyn Fn(&ItemStack) -> bool,
) -> Option<PlayerView> {
    let player = e.player()?;
    let map_invisibility = EquipmentSlot::VALUES
        .into_iter()
        .filter(|s| *s != EquipmentSlot::MainHand && *s != EquipmentSlot::OffHand)
        .any(|s| is_map_invisibility_item(player.inventory.equipment_get(s).item()));
    Some(PlayerView {
        id: e.id(),
        name: player.game_profile.name.clone(),
        dimension: dimension.clone(),
        x: e.base().x(),
        z: e.base().z(),
        y_rot: e.base().y_rot(),
        removed: e.is_removed(),
        has_map: player.inventory.contains_matching(matcher),
        map_invisibility,
    })
}

/// `data.tickCarriedBy(player, stack, placedInFrame)` for the player `ticking` (a player entity that is taken out of
/// the level while it ticks); `ticking_has_map` is `getInventory().contains(mapMatcher)` for it.
pub fn tick_carried_by(
    level: &ServerLevel,
    data: &mut MapItemSavedData,
    ticking: &dyn Entity,
    ticking_has_map: bool,
    stack: &ItemStack,
    placed_in_frame: Option<crate::world::level::saveddata::maps::FrameInfo>,
) {
    let Some(mut view) = player_view(ticking, &level.dimension, &|_| false) else {
        return;
    };
    view.has_map = ticking_has_map;
    let other = |id, matcher: &dyn Fn(&ItemStack) -> bool| {
        level
            .get_entity(id)
            .and_then(|e| player_view(e, &level.dimension, matcher))
    };
    let env = MapTickEnv {
        game_time: level.game_time(),
        tick_count: level.shared.tick_count(),
        ticking: view,
        other: &other,
    };
    data.tick_carried_by(&env, stack, placed_in_frame);
}

/// `getCorrectStateForFluidBlock(level, state, pos)`.
fn get_correct_state_for_fluid_block(
    level: &ServerLevel,
    state: mc_data::block::BlockState,
    pos: BlockPos,
) -> mc_data::block::BlockState {
    let fluid_state = state.get_fluid_state();
    if !fluid_state.is_empty()
        && !state.is_face_sturdy_at(level, pos, mc_core::direction::Direction::Up)
    {
        fluid_state.create_legacy_block()
    } else {
        state
    }
}

/// `MapItem.update(level, player, data)`: renders the part of the world around the player into the map.
pub fn update(level: &mut ServerLevel, player: &dyn Entity, data: &mut MapItemSavedData) {
    if level.dimension != data.dimension || !player.is_player() {
        return;
    }
    let scale = 1i32 << data.scale;
    let center_x = data.center_x;
    let center_z = data.center_z;
    let player_img_x = (player.base().x() - f64::from(center_x)).floor() as i32 / scale + 64;
    let player_img_y = (player.base().z() - f64::from(center_z)).floor() as i32 / scale + 64;
    let mut radius = 128 / scale;
    let has_ceiling = level.has_ceiling;
    if has_ceiling {
        radius /= 2;
    }
    let min_y = level.get_min_y();

    let holding_player = data.get_holding_player(player.id());
    holding_player.step += 1;
    let step = holding_player.step;
    let mut found_consecutive_changes = false;

    for img_x in (player_img_x - radius + 1)..(player_img_x + radius) {
        if (img_x & 15) == (step & 15) || found_consecutive_changes {
            found_consecutive_changes = false;
            let mut previous_average_area_height = 0.0f64;

            for img_y in (player_img_y - radius - 1)..(player_img_y + radius) {
                if !(img_x >= 0 && img_y >= -1 && img_x < 128 && img_y < 128) {
                    continue;
                }
                let distance_to_player_sqr = (img_x - player_img_x)
                    .wrapping_mul(img_x - player_img_x)
                    + (img_y - player_img_y).wrapping_mul(img_y - player_img_y);
                let dither_black = distance_to_player_sqr > (radius - 2) * (radius - 2);
                let averaging_area_min_x = (center_x / scale + img_x - 64) * scale;
                let averaging_area_min_z = (center_z / scale + img_y - 64) * scale;
                // `LinkedHashMultiset<MapColor>`: color id -> count in first-insertion order
                let mut color_count: Vec<(u8, i32)> = Vec::new();
                let mut add =
                    |color: u8, n: i32| match color_count.iter_mut().find(|(c, _)| *c == color) {
                        Some((_, count)) => *count += n,
                        None => color_count.push((color, n)),
                    };
                let (chunk_x, chunk_z) = (averaging_area_min_x >> 4, averaging_area_min_z >> 4);
                let chunk = level.chunk_blocking_mut(chunk_x, chunk_z);
                if !chunk
                    .data
                    .access
                    .heightmaps
                    .contains_key(&HeightmapType::WorldSurface)
                {
                    chunk
                        .data
                        .access
                        .prime_heightmaps(true, &[HeightmapType::WorldSurface]);
                }
                let mut water_depth = 0i32;
                let mut average_area_height = 0.0f64;
                if has_ceiling {
                    let mut ceiling_noise = averaging_area_min_x
                        .wrapping_add(averaging_area_min_z.wrapping_mul(231871));
                    ceiling_noise = ceiling_noise
                        .wrapping_mul(ceiling_noise)
                        .wrapping_mul(31287121)
                        .wrapping_add(ceiling_noise.wrapping_mul(11));
                    if (ceiling_noise >> 20 & 1) == 0 {
                        add(Block::DIRT.default_state().get_map_color(), 10);
                    } else {
                        add(Block::STONE.default_state().get_map_color(), 100);
                    }
                    average_area_height = 100.0;
                } else {
                    let level_ref: &ServerLevel = level;
                    let chunk = level_ref
                        .chunk_now(chunk_x, chunk_z)
                        .expect("chunk loaded above");
                    for averaging_area_delta_x in 0..scale {
                        for averaging_area_delta_z in 0..scale {
                            let (x, z) = (
                                averaging_area_min_x + averaging_area_delta_x,
                                averaging_area_min_z + averaging_area_delta_z,
                            );
                            let heightmap = chunk
                                .data
                                .access
                                .heightmaps
                                .get(&HeightmapType::WorldSurface)
                                .expect("primed above");
                            // `chunk.getHeight(WORLD_SURFACE, x, z) + 1`
                            let mut column_y = heightmap.get_first_available(x & 15, z & 15);
                            let mut state;
                            if column_y <= min_y {
                                state = Block::BEDROCK.default_state();
                            } else {
                                let mut pos;
                                loop {
                                    column_y -= 1;
                                    pos = BlockPos::new(x, column_y, z);
                                    state = chunk.data.get_block_state(pos);
                                    if !(state.get_map_color() == map_color::NONE
                                        && column_y > min_y)
                                    {
                                        break;
                                    }
                                }
                                if column_y > min_y && !state.get_fluid_state().is_empty() {
                                    let mut solid_y = column_y - 1;
                                    loop {
                                        let below_pos = BlockPos::new(x, solid_y, z);
                                        solid_y -= 1;
                                        let below_block = chunk.data.get_block_state(below_pos);
                                        water_depth += 1;
                                        if !(solid_y > min_y
                                            && !below_block.get_fluid_state().is_empty())
                                        {
                                            break;
                                        }
                                    }
                                    state =
                                        get_correct_state_for_fluid_block(level_ref, state, pos);
                                }
                            }
                            data.check_banners(level_ref, x, z);
                            average_area_height += f64::from(column_y) / f64::from(scale * scale);
                            add(state.get_map_color(), 1);
                        }
                    }
                }

                water_depth /= scale * scale;
                // `Iterables.getFirst(Multisets.copyHighestCountFirst(colorCount), NONE)`: stable, so ties keep insertion order
                let color = color_count
                    .iter()
                    .fold(None::<(u8, i32)>, |best, &(c, n)| {
                        if best.is_none_or(|(_, bn)| n > bn) {
                            Some((c, n))
                        } else {
                            best
                        }
                    })
                    .map_or(map_color::NONE, |(c, _)| c);
                let parity = (img_x + img_y) & 1;
                let brightness = if color == map_color::WATER {
                    let diff = f64::from(water_depth) * 0.1 + f64::from(parity) * 0.2;
                    if diff < 0.5 {
                        Brightness::High
                    } else if diff > 0.9 {
                        Brightness::Low
                    } else {
                        Brightness::Normal
                    }
                } else {
                    let diff = (average_area_height - previous_average_area_height) * 4.0
                        / f64::from(scale + 4)
                        + (f64::from(parity) - 0.5) * 0.4;
                    if diff > 0.6 {
                        Brightness::High
                    } else if diff < -0.6 {
                        Brightness::Low
                    } else {
                        Brightness::Normal
                    }
                };
                previous_average_area_height = average_area_height;
                if img_y >= 0
                    && distance_to_player_sqr < radius * radius
                    && (!dither_black || parity != 0)
                {
                    found_consecutive_changes |=
                        data.update_color(img_x, img_y, get_packed_id(color, brightness));
                }
            }
        }
    }
}

/// `lockMap(map, level)`.
fn lock_map(map: &mut ItemStack, level: &mut ServerLevel) {
    if let Some(new_data) = with_saved_data(map, level, |_, data| data.locked()) {
        let id = level.get_free_map_id();
        level.set_map_data(id, new_data);
        map.set(MAP_ID, id);
    }
}

/// `scaleMap(itemStack, level)`.
fn scale_map(stack: &mut ItemStack, level: &mut ServerLevel) {
    if let Some(scaled) = with_saved_data(stack, level, |_, data| data.scaled()) {
        let id = level.get_free_map_id();
        level.set_map_data(id, scaled);
        stack.set(MAP_ID, id);
    }
}

impl ItemBehaviour for MapItem {
    fn inventory_tick(
        &self,
        stack: &mut ItemStack,
        level: &mut ServerLevel,
        owner: &mut dyn Entity,
        slot: Option<EquipmentSlot>,
    ) {
        let Some(id) = stack.get(&MAP_ID).copied() else {
            return;
        };
        let owner: &dyn Entity = owner;
        level.with_map_data(id, |level, data| {
            if owner.is_player() {
                // the ticking stack is in the owner's inventory: `contains(mapMatcher)` holds
                tick_carried_by(level, data, owner, true, stack, None);
            }
            if !data.locked
                && matches!(slot, Some(EquipmentSlot::MainHand | EquipmentSlot::OffHand))
            {
                update(level, owner, data);
            }
        });
    }

    fn on_crafted_post_process(&self, stack: &mut ItemStack, level: &mut ServerLevel) {
        let post_processing = stack.get(&MAP_POST_PROCESSING).copied();
        if post_processing.is_some() {
            stack.remove(mc_data::DataComponentType::MAP_POST_PROCESSING);
        }
        match post_processing {
            Some(MapPostProcessing::Lock) => lock_map(stack, level),
            Some(MapPostProcessing::Scale) => scale_map(stack, level),
            None => {}
        }
    }

    fn use_on(
        &self,
        context: &mut UseOnContext,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let pos = context.get_clicked_pos();
        if level.get_block_state(pos).is_tag("banners") {
            let game_time = level.game_time();
            let toggled = with_saved_data(&context.item_stack.clone(), level, |level, data| {
                data.toggle_banner(&*level, game_time, pos)
            });
            if toggled == Some(false) {
                return InteractionResult::FAIL;
            }
            InteractionResult::SUCCESS
        } else {
            PlainItem.use_on(context, level, player)
        }
    }
}

impl ItemBehaviour for EmptyMapItem {
    fn use_item(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let mut stack = player.get_item_in_hand(hand);
        let infinite = player.player.abilities.instabuild;
        stack.consume(1, infinite);
        // `player.awardStat(Stats.ITEM_USED.get(this))`: not implemented (statistics)
        let position = player.position();
        level.play_sound_at(
            None,
            position.x,
            position.y,
            position.z,
            mc_data::SoundEvent::UI_CARTOGRAPHY_TABLE_TAKE_RESULT,
            SoundSource::Players,
            1.0,
            1.0,
        );
        let (block_x, block_z) = (position.x.floor() as i32, position.z.floor() as i32);
        let map = create(level, block_x, block_z, 0, true, false);
        player.set_item_in_hand(hand, stack.clone());
        if stack.is_empty() {
            match InteractionResult::SUCCESS {
                InteractionResult::Success(success) => {
                    InteractionResult::Success(success.held_item_transformed_to(map))
                }
                other => other,
            }
        } else {
            let mut copy = map.copy();
            if !player.player.inventory.add(&mut copy, infinite) {
                player.drop(
                    level,
                    map,
                    false,
                    crate::world::inventory::Prediction::ServerOnly,
                );
            }
            InteractionResult::SUCCESS
        }
    }
}

impl ServerPlayer {
    /// The `synchronizeSpecialItemUpdates(itemStack)` loop of `ServerPlayer.doTick`: sends the pending map data of every
    /// map in the inventory.
    pub fn synchronize_special_item_updates(&mut self, level: &mut ServerLevel) {
        use crate::world::container::Container;
        let player_id = self.base_id();
        for i in 0..self.player.inventory.get_container_size() {
            let Some(id) = self.player.inventory.get_item(i).get(&MAP_ID).copied() else {
                continue;
            };
            if let Some(Some(packet)) =
                level.with_map_data(id, |_, data| data.get_update_packet(id, player_id))
            {
                self.send(&packet);
            }
        }
    }
}
