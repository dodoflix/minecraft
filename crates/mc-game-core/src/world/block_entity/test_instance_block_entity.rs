//! `TestInstanceBlockEntity`: the block entity of the test instance block (position, size, rotation, status and error
//! of one game test). Vanilla's methods that need the level or registries are free functions here taking the server
//! (the level is a separate object that is taken out of its chunk while the block entity runs).

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::gametest::game_test_instance::{TestHolder, lookup_test};
use crate::gametest::structure_utils;
use crate::registry::RegistryAccess;
use crate::server::MinecraftServer;
use crate::translatable;
use crate::world::level::block::BlockStateExt;
use crate::world::level::{BlockGetter, DimensionKey, LevelAccessor};
use linkme::distributed_slice;
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::phys::AABB;
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::rotation::Rotation;
use mc_data::BlockEntityType;
use mc_data::block::{Block, BlockState};
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::packets::play::{
    ClientboundBlockEntityDataPacket, TestInstanceBlockEntityData, TestInstanceBlockEntityStatus,
};
use mc_text::{ChatFormatting, Component};

pub type Data = TestInstanceBlockEntityData;
pub type Status = TestInstanceBlockEntityStatus;

/// `STRUCTURE_OFFSET`.
const STRUCTURE_OFFSET: Vec3i = Vec3i::new(0, 1, 1);

/// `TestInstanceBlockEntity.ErrorMarker`.
#[derive(Clone, Debug, PartialEq)]
pub struct ErrorMarker {
    pub pos: BlockPos,
    pub text: Component,
}

#[derive(Clone)]
pub struct TestInstanceBlockEntity {
    base: BlockEntityData,
    data: Data,
    error_markers: Vec<ErrorMarker>,
}

fn default_data() -> Data {
    Data {
        test: None,
        size: Vec3i::ZERO,
        rotation: Rotation::None,
        ignore_entities: false,
        status: Status::Cleared,
        error_message: None,
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Cleared => "cleared",
        Status::Running => "running",
        Status::Finished => "finished",
    }
}

impl TestInstanceBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::TEST_INSTANCE_BLOCK, pos, state),
            data: default_data(),
            error_markers: Vec::new(),
        }
    }

    pub fn data(&self) -> &Data {
        &self.data
    }

    /// `set(data)` without the `setChanged()` side effects (see [`changed`]).
    pub fn set_data(&mut self, data: Data) {
        self.data = data;
    }

    pub fn error_markers(&self) -> &[ErrorMarker] {
        &self.error_markers
    }

    /// `Data.CODEC` encode.
    fn encode_data(data: &Data) -> Tag {
        let mut tag = CompoundTag::new();
        if let Some(test) = &data.test {
            tag.put_string("test", test.to_string());
        }
        tag.put(
            "size",
            Tag::IntArray(vec![data.size.x, data.size.y, data.size.z]),
        );
        tag.put_string("rotation", data.rotation.get_serialized_name());
        tag.put_boolean("ignore_entities", data.ignore_entities);
        tag.put_string("status", status_name(data.status));
        if let Some(error) = &data.error_message
            && let Ok(component) = error.to_nbt()
        {
            tag.put("error_message", component);
        }
        Tag::Compound(tag)
    }

    /// `Data.CODEC` decode (`None` when a required field is missing or malformed).
    fn decode_data(tag: &CompoundTag) -> Option<Data> {
        let test = match tag.get_string("test") {
            Some(s) => Some(Identifier::try_parse(s)?),
            None => None,
        };
        let size = tag
            .get_int_array("size")
            .filter(|a| a.len() == 3)
            .map(|a| Vec3i::new(a[0], a[1], a[2]))?;
        let rotation = Rotation::by_name(tag.get_string("rotation")?)?;
        let ignore_entities = tag.get_boolean("ignore_entities")?;
        let status = match tag.get_string("status")? {
            "cleared" => Status::Cleared,
            "running" => Status::Running,
            "finished" => Status::Finished,
            _ => return None,
        };
        let error_message = tag
            .get("error_message")
            .and_then(|t| Component::from_nbt(t).ok());
        Some(Data {
            test,
            size,
            rotation,
            ignore_entities,
            status,
            error_message,
        })
    }
}

impl BlockEntity for TestInstanceBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        if let Some(data) = tag.get_compound("data").and_then(Self::decode_data) {
            self.data = data;
        }
        self.error_markers.clear();
        for marker in tag.get_list_or_empty("errors").compounds() {
            let pos = marker
                .get_int_array("pos")
                .filter(|a| a.len() == 3)
                .map(|a| BlockPos::new(a[0], a[1], a[2]));
            let text = marker.get("text").and_then(|t| Component::from_nbt(t).ok());
            if let (Some(pos), Some(text)) = (pos, text) {
                self.error_markers.push(ErrorMarker { pos, text });
            }
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put("data", Self::encode_data(&self.data));
        if !self.error_markers.is_empty() {
            let mut list = ListTag::new();
            for marker in &self.error_markers {
                let mut entry = CompoundTag::new();
                entry.put(
                    "pos",
                    Tag::IntArray(vec![marker.pos.x(), marker.pos.y(), marker.pos.z()]),
                );
                if let Ok(text) = marker.text.to_nbt() {
                    entry.put("text", text);
                }
                list.push(Tag::Compound(entry));
            }
            tag.put("errors", Tag::List(list));
        }
    }

    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket {
            pos: self.get_block_pos(),
            r#type: self.get_type(),
            tag: self.get_update_tag(),
        })
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static TEST_INSTANCE_BLOCK: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::TEST_INSTANCE_BLOCK,
    create: |pos, state| Box::new(TestInstanceBlockEntity::new(pos, state)),
};

// ---------------------------------------------------------------------------------------------------------------
// the geometry of one test instance block (pure functions of its data and its test)
// ---------------------------------------------------------------------------------------------------------------

/// `AABB.of(BoundingBox)`.
pub fn aabb_of(bounding_box: &BoundingBox) -> AABB {
    AABB::of_box(
        bounding_box.min_x(),
        bounding_box.min_y(),
        bounding_box.min_z(),
        bounding_box.max_x() + 1,
        bounding_box.max_y() + 1,
        bounding_box.max_z() + 1,
    )
}

/// A snapshot of a test instance block entity together with its resolved test: every vanilla accessor that reads
/// `getTestHolder()` / the data lives here.
#[derive(Clone)]
pub struct TestInstanceView {
    pub pos: BlockPos,
    pub data: Data,
    pub test: Option<TestHolder>,
    pub error_markers: Vec<ErrorMarker>,
}

impl TestInstanceView {
    /// The block entity at `pos` of level `dimension`, with its test resolved against `registries`.
    pub fn read(
        server: &MinecraftServer,
        dimension: &DimensionKey,
        pos: BlockPos,
    ) -> Option<TestInstanceView> {
        let be = server
            .level(dimension)?
            .get_block_entity(pos)?
            .as_any()
            .downcast_ref::<TestInstanceBlockEntity>()?;
        let registries = server.registries.composite_access();
        let test = be
            .data
            .test
            .as_ref()
            .and_then(|key| lookup_test(registries, key));
        Some(TestInstanceView {
            pos,
            data: be.data.clone(),
            test,
            error_markers: be.error_markers.clone(),
        })
    }

    /// `test()`.
    pub fn test_key(&self) -> Option<&Identifier> {
        self.data.test.as_ref()
    }

    /// `getTestName()`.
    pub fn get_test_name(&self) -> Component {
        match &self.data.test {
            Some(key) => Component::literal(key.to_string()),
            None => translatable!("test_instance_block.invalid_test"),
        }
    }

    pub fn ignore_entities(&self) -> bool {
        self.data.ignore_entities
    }

    pub fn get_size(&self) -> Vec3i {
        self.data.size
    }

    /// `getRotation()`.
    pub fn get_rotation(&self) -> Rotation {
        self.test
            .as_ref()
            .map(|t| t.value.rotation())
            .unwrap_or(Rotation::None)
            .get_rotated(self.data.rotation)
    }

    /// `getPadding()`.
    pub fn get_padding(&self) -> i32 {
        self.test.as_ref().map(|t| t.value.padding()).unwrap_or(0)
    }

    /// `getStructurePos()`.
    pub fn get_structure_pos(&self) -> BlockPos {
        let padding = self.get_padding();
        structure_pos_of(self.pos.offset(padding, padding, padding))
    }

    /// `getTransformedSize()`.
    pub fn get_transformed_size(&self) -> Vec3i {
        let size = self.get_size();
        let rotation = self.get_rotation();
        let axes_switched =
            rotation == Rotation::Clockwise90 || rotation == Rotation::Counterclockwise90;
        Vec3i::new(
            if axes_switched { size.z } else { size.x },
            size.y,
            if axes_switched { size.x } else { size.z },
        )
    }

    /// `getStructureBoundingBox()`.
    pub fn get_structure_bounding_box(&self) -> BoundingBox {
        let corner1 = self.get_structure_pos();
        let corner2 = corner1
            .offset_vec(&self.get_transformed_size())
            .offset(-1, -1, -1);
        BoundingBox::from_corners(&corner1, &corner2)
    }

    /// `getTestBoundingBox()`.
    pub fn get_test_bounding_box(&self) -> BoundingBox {
        self.get_structure_bounding_box()
            .inflated_by(self.get_padding())
    }

    /// `getStructureBounds()`.
    pub fn get_structure_bounds(&self) -> AABB {
        aabb_of(&self.get_structure_bounding_box())
    }

    /// `getTestBounds()`.
    pub fn get_test_bounds(&self) -> AABB {
        self.get_structure_bounds()
            .inflate_scalar(f64::from(self.get_padding()))
    }

    /// `getStartCorner()`.
    pub fn get_start_corner(&self) -> BlockPos {
        let size = self.get_size();
        let north_west_corner = self.get_structure_pos();
        match self.get_rotation() {
            Rotation::None => north_west_corner,
            Rotation::Clockwise90 => north_west_corner.offset(size.z - 1, 0, 0),
            Rotation::Clockwise180 => north_west_corner.offset(size.x - 1, 0, size.z - 1),
            Rotation::Counterclockwise90 => north_west_corner.offset(0, 0, size.x - 1),
        }
    }

    /// `getBeamSections()` as the beam color (`ARGB`), empty when cleared.
    pub fn get_beam_color(&self) -> Option<i32> {
        match self.data.status {
            Status::Cleared => None,
            Status::Running => Some(argb(128, 128, 128)),
            Status::Finished => Some(if self.data.error_message.is_none() {
                argb(0, 255, 0)
            } else if self
                .test
                .as_ref()
                .map(|t| t.value.required())
                .unwrap_or(true)
            {
                argb(255, 0, 0)
            } else {
                argb(255, 128, 0)
            }),
        }
    }
}

fn argb(r: i32, g: i32, b: i32) -> i32 {
    (255 << 24) | (r << 16) | (g << 8) | b
}

/// `getStructurePos(blockPos)`.
pub fn structure_pos_of(block_pos: BlockPos) -> BlockPos {
    block_pos.offset_vec(&STRUCTURE_OFFSET)
}

// ---------------------------------------------------------------------------------------------------------------
// operations with side effects
// ---------------------------------------------------------------------------------------------------------------

/// `setChanged()`: marks the chunk dirty and sends the block entity update (`sendBlockUpdated(pos, AIR, state, 3)`).
pub fn changed(server: &mut MinecraftServer, dimension: &DimensionKey, pos: BlockPos) {
    if let Some(level) = server.level_mut(dimension) {
        level.block_entity_changed(pos);
        let state = level.get_block_state(pos);
        level.send_block_updated(pos, Block::AIR.default_state(), state, 3);
    }
}

/// Runs `f` on the block entity at `pos`; `None` when there is none.
fn modify<R>(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    f: impl FnOnce(&mut TestInstanceBlockEntity) -> R,
) -> Option<R> {
    let level = server.level_mut(dimension)?;
    let be = level
        .get_block_entity_immediate(pos)?
        .as_any_mut()
        .downcast_mut::<TestInstanceBlockEntity>()?;
    Some(f(be))
}

/// `set(data)`.
pub fn set(server: &mut MinecraftServer, dimension: &DimensionKey, pos: BlockPos, data: Data) {
    if modify(server, dimension, pos, |be| be.data = data).is_some() {
        changed(server, dimension, pos);
    }
}

fn set_status(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    status: Status,
) {
    if let Some(view) = TestInstanceView::read(server, dimension, pos) {
        set(
            server,
            dimension,
            pos,
            Data {
                status,
                error_message: None,
                ..view.data
            },
        );
    }
}

/// `setErrorMessage(errorMessage)` (`Data.withError`).
pub fn set_error_message(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    error: Component,
) {
    if let Some(view) = TestInstanceView::read(server, dimension, pos) {
        set(
            server,
            dimension,
            pos,
            Data {
                status: Status::Finished,
                error_message: Some(error),
                ..view.data
            },
        );
    }
}

/// `setSuccess()`.
pub fn set_success(server: &mut MinecraftServer, dimension: &DimensionKey, pos: BlockPos) {
    set_status(server, dimension, pos, Status::Finished);
}

/// `setRunning()`.
pub fn set_running(server: &mut MinecraftServer, dimension: &DimensionKey, pos: BlockPos) {
    set_status(server, dimension, pos, Status::Running);
}

/// `markError(pos, text)`.
pub fn mark_error(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    error_pos: BlockPos,
    text: Component,
) {
    if modify(server, dimension, pos, |be| {
        be.error_markers.push(ErrorMarker {
            pos: error_pos,
            text,
        })
    })
    .is_some()
    {
        changed(server, dimension, pos);
    }
}

/// `clearErrorMarkers()`.
pub fn clear_error_markers(server: &mut MinecraftServer, dimension: &DimensionKey, pos: BlockPos) {
    let cleared = modify(server, dimension, pos, |be| {
        let had = !be.error_markers.is_empty();
        be.error_markers.clear();
        had
    });
    if cleared == Some(true) {
        changed(server, dimension, pos);
    }
}

/// `getStructureSize(level, testKey)`.
pub fn get_structure_size(
    server: &MinecraftServer,
    dimension: &DimensionKey,
    test_key: &Identifier,
) -> Option<Vec3i> {
    get_structure_size_in(
        server.level(dimension)?,
        server.registries.composite_access(),
        test_key,
    )
}

/// `getStructureSize(level, testKey)` for a level that is not reachable through the server (the player's level while its
/// packet is handled).
pub fn get_structure_size_in(
    level: &crate::server::level::ServerLevel,
    registries: &RegistryAccess,
    test_key: &Identifier,
) -> Option<Vec3i> {
    let test = lookup_test(registries, test_key)?;
    level
        .structure_template_manager()
        .get(test.value.structure())
        .map(|t| t.get_size())
}

/// `getStructureTemplate(level, testKey)`.
fn structure_template_of(
    server: &MinecraftServer,
    dimension: &DimensionKey,
    registries: &RegistryAccess,
    test_key: &Identifier,
) -> Option<std::sync::Arc<crate::world::level::levelgen::structure::templatesystem::structure_template::StructureTemplate>>{
    let test = lookup_test(registries, test_key)?;
    server
        .level(dimension)?
        .structure_template_manager()
        .get(test.value.structure())
}

/// `placeStructure()`: whether the test's template exists (and was placed).
pub fn place_structure(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
) -> bool {
    let Some(view) = TestInstanceView::read(server, dimension, pos) else {
        return false;
    };
    let Some(test_key) = view.data.test.clone() else {
        return false;
    };
    let registries = server.registries.composite_access().clone();
    let Some(template) = structure_template_of(server, dimension, &registries, &test_key) else {
        return false;
    };
    let Some(level) = server.level_mut(dimension) else {
        return false;
    };
    use crate::world::level::levelgen::structure::templatesystem::structure_place_settings::StructurePlaceSettings;
    let mut settings = StructurePlaceSettings::new();
    settings
        .set_rotation(view.get_rotation())
        .set_ignore_entities(view.ignore_entities())
        .set_known_shape(true);
    let start = view.get_start_corner();
    // forceLoadChunks
    for chunk in view.get_structure_bounding_box().intersecting_chunks() {
        level.chunk_source.update_chunk_forced(chunk, true);
    }
    structure_utils::clear_space_for_structure(level, &view.get_test_bounding_box());
    structure_utils::discard_non_player_entities(level, &view.get_test_bounds());
    let mut random = mc_core::random::LegacyRandomSource::new(level.random().next_long());
    template.place_in_world(level, start, start, &settings, &mut random, 818);
    true
}

/// `encaseStructure()`.
pub fn encase_structure(server: &mut MinecraftServer, dimension: &DimensionKey, pos: BlockPos) {
    process_structure_boundary(server, dimension, pos, |level, block_pos| {
        if !level
            .get_block_state(block_pos)
            .is_block(Block::TEST_INSTANCE_BLOCK)
        {
            level.set_block_and_update(block_pos, Block::BARRIER.default_state());
        }
    });
}

/// `removeBarriers()`.
pub fn remove_barriers(server: &mut MinecraftServer, dimension: &DimensionKey, pos: BlockPos) {
    process_structure_boundary(server, dimension, pos, |level, block_pos| {
        if level.get_block_state(block_pos).is_block(Block::BARRIER) {
            level.set_block_and_update(block_pos, Block::AIR.default_state());
        }
    });
}

/// `processStructureBoundary(action)`.
fn process_structure_boundary(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    mut action: impl FnMut(&mut crate::server::level::ServerLevel, BlockPos),
) {
    let Some(view) = TestInstanceView::read(server, dimension, pos) else {
        return;
    };
    let Some(level) = server.level_mut(dimension) else {
        return;
    };
    let bounds = view.get_structure_bounds();
    let has_ceiling = !view
        .test
        .as_ref()
        .map(|t| t.value.sky_access())
        .unwrap_or(false);
    let low = BlockPos::containing(bounds.min_x, bounds.min_y, bounds.min_z).offset(-1, -1, -1);
    let high = BlockPos::containing(bounds.max_x, bounds.max_y, bounds.max_z);
    for block_pos in BlockPos::between_closed(&low, &high) {
        let is_non_ceiling_edge = block_pos.x() == low.x()
            || block_pos.x() == high.x()
            || block_pos.z() == low.z()
            || block_pos.z() == high.z()
            || block_pos.y() == low.y();
        let is_ceiling = block_pos.y() == high.y();
        if is_non_ceiling_edge || is_ceiling && has_ceiling {
            action(level, block_pos);
        }
    }
}

/// `resetTest(feedbackOutput)`.
pub fn reset_test(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    feedback: &mut dyn FnMut(Component),
) {
    remove_barriers(server, dimension, pos);
    clear_error_markers(server, dimension, pos);
    let placed = place_structure(server, dimension, pos);
    if let Some(view) = TestInstanceView::read(server, dimension, pos) {
        if placed {
            feedback(
                translatable!("test_instance_block.reset_success", view.get_test_name())
                    .with_format(ChatFormatting::Green),
            );
        }
        set(
            server,
            dimension,
            pos,
            Data {
                status: Status::Cleared,
                error_message: None,
                ..view.data
            },
        );
    }
}

/// `saveTest(feedbackOutput)`: the structure id the test was saved under.
pub fn save_test(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    feedback: &mut dyn FnMut(Component),
) -> Option<Identifier> {
    let view = TestInstanceView::read(server, dimension, pos)?;
    let identifier = match &view.test {
        Some(test) => Some(test.value.structure().clone()),
        None => view.data.test.clone(),
    };
    let Some(identifier) = identifier else {
        feedback(
            translatable!(
                "test_instance_block.error.unable_to_save",
                pos.x(),
                pos.y(),
                pos.z()
            )
            .with_format(ChatFormatting::Red),
        );
        return None;
    };
    if let Some(level) = server.level_mut(dimension) {
        save_structure(
            level,
            &identifier,
            view.get_structure_pos(),
            view.get_size(),
        );
    }
    Some(identifier)
}

/// `StructureBlockEntity.saveStructure(level, id, pos, size, ignoreEntities, "", true, [AIR])`: fills a fresh template from the
/// world and saves it. Entities are not collected (the structure block code owns `StructureBlockEntity.saveStructure`;
/// switch to it when it lands), so saved test structures carry blocks only.
fn save_structure(
    level: &mut crate::server::level::ServerLevel,
    id: &Identifier,
    pos: BlockPos,
    size: Vec3i,
) -> bool {
    use crate::world::level::levelgen::structure::templatesystem::structure_template::StructureTemplate;
    let mut template = StructureTemplate::new();
    template.fill_from_world(&*level, pos, size, None, &[Block::AIR]);
    template.set_author("");
    let manager = level.structure_template_manager().clone();
    manager.put(id, std::sync::Arc::new(template));
    manager.save(id)
}

/// `exportTest(feedbackOutput)`: `true` when it failed.
pub fn export_test(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    feedback: &mut dyn FnMut(Component),
) -> bool {
    match save_test(server, dimension, pos, feedback) {
        Some(saved) => export(server, dimension, &saved, feedback),
        None => false,
    }
}

/// `TestInstanceBlockEntity.export(level, structureId, feedbackOutput)`: the structure manager has no test template
/// directory on a dedicated server (`testTemplates()` is `null`), so this always reports that exporting is disabled;
/// returns `true` like vanilla (`export` answers `true` on every failure path).
pub fn export(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    _structure_id: &Identifier,
    feedback: &mut dyn FnMut(Component),
) -> bool {
    let _ = (server, dimension);
    feedback(
        Component::literal("Test structure exporting is disabled").with_format(ChatFormatting::Red),
    );
    true
}

/// `runTest(feedbackOutput)`.
pub fn run_test(
    server: &mut MinecraftServer,
    dimension: &DimensionKey,
    pos: BlockPos,
    feedback: &mut dyn FnMut(Component),
) {
    let Some(view) = TestInstanceView::read(server, dimension, pos) else {
        return;
    };
    let Some(test) = view.test.clone() else {
        feedback(
            translatable!(
                "test_instance_block.error.no_test",
                pos.x(),
                pos.y(),
                pos.z()
            )
            .with_format(ChatFormatting::Red),
        );
        return;
    };
    if !place_structure(server, dimension, pos) {
        feedback(
            translatable!(
                "test_instance_block.error.no_test_structure",
                pos.x(),
                pos.y(),
                pos.z()
            )
            .with_format(ChatFormatting::Red),
        );
        return;
    }
    clear_error_markers(server, dimension, pos);
    crate::gametest::game_test_ticker::clear(server);
    crate::gametest::failed_test_tracker::forget_failed_tests();
    feedback(translatable!(
        "test_instance_block.starting",
        test.registered_name()
    ));
    let info = crate::gametest::game_test_info::GameTestInfo::new(
        test,
        view.data.rotation,
        dimension.clone(),
        crate::gametest::retry_options::RetryOptions::no_retries(),
    );
    info.set_test_block_pos(Some(pos));
    let runner = crate::gametest::game_test_runner::Builder::from_info(vec![info]).build();
    let source = server.create_command_source_stack();
    crate::gametest::test_command::track_and_start_runner(server, source, &runner);
}
