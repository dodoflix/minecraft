//! `play` state: all 144 clientbound / 69 serverbound packets of vanilla 26.3 (`GameProtocols`).
//!
//! The packets are split over group modules (re-exported here, so every packet is `packets::play::<VanillaClassName>`);
//! `common`/`cookie`/`ping` packets are stamped here with the play ids. Type names are the vanilla class names
//! (`ClientboundAddEntityPacket`); nested packet classes become `<Outer><Inner>Packet`
//! (`ClientboundMoveEntityPosPacket`, `ServerboundMovePlayerPosRotPacket`). Registry-holder fields of *data-driven*
//! registries (enchantments, biomes, dimension types, variants, ...) carry the raw registry id (`i32`), static registries use
//! the typed `mc_data` ids. Codec-backed NBT payloads whose Rust model lives in a later layer are raw [`mc_nbt::Tag`]s.

use super::common::{common_packets, cookie_packets};
use crate::codec::Holder;
use crate::{Decode, Encode, Packet, packet_enum};

pub mod chat;
pub mod debug;
pub mod entity;
pub mod entity_data;
pub mod inventory;
pub mod level;
pub mod misc;
pub mod serverbound;
pub mod types;

pub use chat::*;
pub use debug::*;
pub use entity::*;
pub use entity_data::*;
pub use inventory::*;
pub use level::*;
pub use misc::*;
pub use serverbound::*;
pub use types::*;

cookie_packets!(play);
common_packets!(play);

/// `ClientboundBundleDelimiterPacket`: marks the start/end of a bundle (the pipeline, not the codec, groups packets).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "bundle_delimiter")]
pub struct ClientboundBundleDelimiterPacket;

/// `ClientboundPongResponsePacket` (play instance of the ping packet).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "pong_response")]
pub struct ClientboundPongResponsePacket {
    pub time: i64,
}

/// `ServerboundPingRequestPacket` (play instance of the ping packet).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "ping_request")]
pub struct ServerboundPingRequestPacket {
    pub time: i64,
}

/// `ClientboundShowDialogPacket.STREAM_CODEC`: `Holder<Dialog>` — registry id + 1, or `0` + the dialog's NBT
/// (`ByteBufCodecs.holder` with the trusted NBT codec). A reference is the raw registry id.
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "show_dialog")]
pub struct ClientboundShowDialogPacket {
    pub dialog: Holder<crate::dialog::Dialog>,
}

packet_enum! {
    /// Clientbound play packets, in packet-id order of `packets.json`.
    pub enum ClientboundPlayPacket {
        BundleDelimiter(ClientboundBundleDelimiterPacket),
        AddEntity(ClientboundAddEntityPacket),
        Animate(ClientboundAnimatePacket),
        AwardStats(ClientboundAwardStatsPacket),
        BlockChangedAck(ClientboundBlockChangedAckPacket),
        BlockDestruction(ClientboundBlockDestructionPacket),
        BlockEntityData(ClientboundBlockEntityDataPacket),
        BlockEvent(ClientboundBlockEventPacket),
        BlockUpdate(ClientboundBlockUpdatePacket),
        BossEvent(ClientboundBossEventPacket),
        ChangeDifficulty(ClientboundChangeDifficultyPacket),
        ChunkBatchFinished(ClientboundChunkBatchFinishedPacket),
        ChunkBatchStart(ClientboundChunkBatchStartPacket),
        ChunksBiomes(ClientboundChunksBiomesPacket),
        ClearTitles(ClientboundClearTitlesPacket),
        CommandSuggestions(ClientboundCommandSuggestionsPacket),
        Commands(ClientboundCommandsPacket),
        ContainerClose(ClientboundContainerClosePacket),
        ContainerSetContent(ClientboundContainerSetContentPacket),
        ContainerSetData(ClientboundContainerSetDataPacket),
        ContainerSetSlot(ClientboundContainerSetSlotPacket),
        CookieRequest(ClientboundCookieRequestPacket),
        Cooldown(ClientboundCooldownPacket),
        CustomChatCompletions(ClientboundCustomChatCompletionsPacket),
        CustomPayload(ClientboundCustomPayloadPacket),
        DamageEvent(ClientboundDamageEventPacket),
        DebugBlockValue(ClientboundDebugBlockValuePacket),
        DebugChunkValue(ClientboundDebugChunkValuePacket),
        DebugEntityValue(ClientboundDebugEntityValuePacket),
        DebugEvent(ClientboundDebugEventPacket),
        DebugSample(ClientboundDebugSamplePacket),
        DeleteChat(ClientboundDeleteChatPacket),
        Disconnect(ClientboundDisconnectPacket),
        DisguisedChat(ClientboundDisguisedChatPacket),
        EntityEvent(ClientboundEntityEventPacket),
        EntityPositionSync(ClientboundEntityPositionSyncPacket),
        Explode(ClientboundExplodePacket),
        AddTransientBlock(ClientboundAddTransientBlockPacket),
        ForgetLevelChunk(ClientboundForgetLevelChunkPacket),
        GameEvent(ClientboundGameEventPacket),
        GameRuleValues(ClientboundGameRuleValuesPacket),
        GameTestHighlightPos(ClientboundGameTestHighlightPosPacket),
        MountScreenOpen(ClientboundMountScreenOpenPacket),
        HurtAnimation(ClientboundHurtAnimationPacket),
        InitializeBorder(ClientboundInitializeBorderPacket),
        KeepAlive(ClientboundKeepAlivePacket),
        LevelChunkWithLight(ClientboundLevelChunkWithLightPacket),
        LevelEvent(ClientboundLevelEventPacket),
        LevelParticles(ClientboundLevelParticlesPacket),
        LightUpdate(ClientboundLightUpdatePacket),
        Login(ClientboundLoginPacket),
        LowDiskSpaceWarning(ClientboundLowDiskSpaceWarningPacket),
        MapItemData(ClientboundMapItemDataPacket),
        MerchantOffers(ClientboundMerchantOffersPacket),
        MoveEntityPos(ClientboundMoveEntityPosPacket),
        MoveEntityPosRot(ClientboundMoveEntityPosRotPacket),
        MoveMinecart(ClientboundMoveMinecartPacket),
        MoveEntityRot(ClientboundMoveEntityRotPacket),
        MoveVehicle(ClientboundMoveVehiclePacket),
        OpenBook(ClientboundOpenBookPacket),
        OpenScreen(ClientboundOpenScreenPacket),
        OpenSignEditor(ClientboundOpenSignEditorPacket),
        Ping(ClientboundPingPacket),
        PongResponse(ClientboundPongResponsePacket),
        PlaceGhostRecipe(ClientboundPlaceGhostRecipePacket),
        PlayerAbilities(ClientboundPlayerAbilitiesPacket),
        PlayerChat(ClientboundPlayerChatPacket),
        PlayerCombatEnd(ClientboundPlayerCombatEndPacket),
        PlayerCombatEnter(ClientboundPlayerCombatEnterPacket),
        PlayerCombatKill(ClientboundPlayerCombatKillPacket),
        PlayerInfoRemove(ClientboundPlayerInfoRemovePacket),
        PlayerInfoUpdate(ClientboundPlayerInfoUpdatePacket),
        PlayerLookAt(ClientboundPlayerLookAtPacket),
        PlayerPosition(ClientboundPlayerPositionPacket),
        PlayerRotation(ClientboundPlayerRotationPacket),
        RecipeBookAdd(ClientboundRecipeBookAddPacket),
        RecipeBookRemove(ClientboundRecipeBookRemovePacket),
        RecipeBookSettings(ClientboundRecipeBookSettingsPacket),
        RemoveEntities(ClientboundRemoveEntitiesPacket),
        RemoveMobEffect(ClientboundRemoveMobEffectPacket),
        ResetScore(ClientboundResetScorePacket),
        ResourcePackPop(ClientboundResourcePackPopPacket),
        ResourcePackPush(ClientboundResourcePackPushPacket),
        PostEffects(ClientboundPostEffectsPacket),
        Respawn(ClientboundRespawnPacket),
        RotateHead(ClientboundRotateHeadPacket),
        SectionBlocksUpdate(ClientboundSectionBlocksUpdatePacket),
        SelectAdvancementsTab(ClientboundSelectAdvancementsTabPacket),
        ServerData(ClientboundServerDataPacket),
        SetActionBarText(ClientboundSetActionBarTextPacket),
        SetBorderCenter(ClientboundSetBorderCenterPacket),
        SetBorderLerpSize(ClientboundSetBorderLerpSizePacket),
        SetBorderSize(ClientboundSetBorderSizePacket),
        SetBorderWarningDelay(ClientboundSetBorderWarningDelayPacket),
        SetBorderWarningDistance(ClientboundSetBorderWarningDistancePacket),
        SetCamera(ClientboundSetCameraPacket),
        SetChunkCacheCenter(ClientboundSetChunkCacheCenterPacket),
        SetChunkCacheRadius(ClientboundSetChunkCacheRadiusPacket),
        SetCursorItem(ClientboundSetCursorItemPacket),
        SetDefaultSpawnPosition(ClientboundSetDefaultSpawnPositionPacket),
        SetDisplayObjective(ClientboundSetDisplayObjectivePacket),
        SetEntityData(ClientboundSetEntityDataPacket),
        SetEntityLink(ClientboundSetEntityLinkPacket),
        SetEntityMotion(ClientboundSetEntityMotionPacket),
        SetEquipment(ClientboundSetEquipmentPacket),
        SetExperience(ClientboundSetExperiencePacket),
        SetHealth(ClientboundSetHealthPacket),
        SetHeldSlot(ClientboundSetHeldSlotPacket),
        SetObjective(ClientboundSetObjectivePacket),
        SetPassengers(ClientboundSetPassengersPacket),
        SetPlayerInventory(ClientboundSetPlayerInventoryPacket),
        SetPlayerTeam(ClientboundSetPlayerTeamPacket),
        SetScore(ClientboundSetScorePacket),
        SetSimulationDistance(ClientboundSetSimulationDistancePacket),
        SetSubtitleText(ClientboundSetSubtitleTextPacket),
        SetTime(ClientboundSetTimePacket),
        SetTitleText(ClientboundSetTitleTextPacket),
        SetTitlesAnimation(ClientboundSetTitlesAnimationPacket),
        SoundEntity(ClientboundSoundEntityPacket),
        Sound(ClientboundSoundPacket),
        StartConfiguration(ClientboundStartConfigurationPacket),
        StopSound(ClientboundStopSoundPacket),
        StoreCookie(ClientboundStoreCookiePacket),
        SwingAnimation(ClientboundSwingAnimationPacket),
        SystemChat(ClientboundSystemChatPacket),
        TabList(ClientboundTabListPacket),
        TagQuery(ClientboundTagQueryPacket),
        TakeItemEntity(ClientboundTakeItemEntityPacket),
        TeleportEntity(ClientboundTeleportEntityPacket),
        TestInstanceBlockStatus(ClientboundTestInstanceBlockStatus),
        TickingState(ClientboundTickingStatePacket),
        TickingStep(ClientboundTickingStepPacket),
        Transfer(ClientboundTransferPacket),
        UpdateAdvancements(ClientboundUpdateAdvancementsPacket),
        UpdateAttributes(ClientboundUpdateAttributesPacket),
        UpdateMobEffect(ClientboundUpdateMobEffectPacket),
        UpdateRecipes(ClientboundUpdateRecipesPacket),
        UpdateTags(ClientboundUpdateTagsPacket),
        ProjectilePower(ClientboundProjectilePowerPacket),
        CustomReportDetails(ClientboundCustomReportDetailsPacket),
        ServerLinks(ClientboundServerLinksPacket),
        TrackedWaypoint(ClientboundTrackedWaypointPacket),
        ClearDialog(ClientboundClearDialogPacket),
        ShowDialog(ClientboundShowDialogPacket),
    }
}

packet_enum! {
    /// Serverbound play packets, in packet-id order of `packets.json`.
    pub enum ServerboundPlayPacket {
        AcceptTeleportation(ServerboundAcceptTeleportationPacket),
        Attack(ServerboundAttackPacket),
        BlockEntityTagQuery(ServerboundBlockEntityTagQueryPacket),
        SelectBundleItem(ServerboundSelectBundleItemPacket),
        ChangeDifficulty(ServerboundChangeDifficultyPacket),
        ChangeGameMode(ServerboundChangeGameModePacket),
        ChatAck(ServerboundChatAckPacket),
        ChatCommand(ServerboundChatCommandPacket),
        ChatCommandSigned(ServerboundChatCommandSignedPacket),
        Chat(ServerboundChatPacket),
        ChatSessionUpdate(ServerboundChatSessionUpdatePacket),
        ChunkBatchReceived(ServerboundChunkBatchReceivedPacket),
        ClientCommand(ServerboundClientCommandPacket),
        ClientTickEnd(ServerboundClientTickEndPacket),
        ClientInformation(ServerboundClientInformationPacket),
        CommandSuggestion(ServerboundCommandSuggestionPacket),
        ConfigurationAcknowledged(ServerboundConfigurationAcknowledgedPacket),
        ContainerButtonClick(ServerboundContainerButtonClickPacket),
        ContainerClick(ServerboundContainerClickPacket),
        ContainerClose(ServerboundContainerClosePacket),
        ContainerSlotStateChanged(ServerboundContainerSlotStateChangedPacket),
        CookieResponse(ServerboundCookieResponsePacket),
        CustomPayload(ServerboundCustomPayloadPacket),
        DebugSubscriptionRequest(ServerboundDebugSubscriptionRequestPacket),
        EditBook(ServerboundEditBookPacket),
        EntityTagQuery(ServerboundEntityTagQueryPacket),
        Interact(ServerboundInteractPacket),
        JigsawGenerate(ServerboundJigsawGeneratePacket),
        KeepAlive(ServerboundKeepAlivePacket),
        LockDifficulty(ServerboundLockDifficultyPacket),
        MovePlayerPos(ServerboundMovePlayerPosPacket),
        MovePlayerPosRot(ServerboundMovePlayerPosRotPacket),
        MovePlayerRot(ServerboundMovePlayerRotPacket),
        MovePlayerStatusOnly(ServerboundMovePlayerStatusOnlyPacket),
        MoveVehicle(ServerboundMoveVehiclePacket),
        PaddleBoat(ServerboundPaddleBoatPacket),
        PickItemFromBlock(ServerboundPickItemFromBlockPacket),
        PickItemFromEntity(ServerboundPickItemFromEntityPacket),
        PingRequest(ServerboundPingRequestPacket),
        PlaceRecipe(ServerboundPlaceRecipePacket),
        PlayerAbilities(ServerboundPlayerAbilitiesPacket),
        PlayerAction(ServerboundPlayerActionPacket),
        PlayerCommand(ServerboundPlayerCommandPacket),
        PlayerInput(ServerboundPlayerInputPacket),
        PlayerLoaded(ServerboundPlayerLoadedPacket),
        Pong(ServerboundPongPacket),
        Punch(ServerboundPunchPacket),
        RecipeBookChangeSettings(ServerboundRecipeBookChangeSettingsPacket),
        RecipeBookSeenRecipe(ServerboundRecipeBookSeenRecipePacket),
        RenameItem(ServerboundRenameItemPacket),
        ResourcePack(ServerboundResourcePackPacket),
        SeenAdvancements(ServerboundSeenAdvancementsPacket),
        SelectTrade(ServerboundSelectTradePacket),
        SetBeacon(ServerboundSetBeaconPacket),
        SetCarriedItem(ServerboundSetCarriedItemPacket),
        SetCommandBlock(ServerboundSetCommandBlockPacket),
        SetCommandMinecart(ServerboundSetCommandMinecartPacket),
        SetCreativeModeSlot(ServerboundSetCreativeModeSlotPacket),
        SetGameRule(ServerboundSetGameRulePacket),
        SetJigsawBlock(ServerboundSetJigsawBlockPacket),
        SetStructureBlock(ServerboundSetStructureBlockPacket),
        SetTestBlock(ServerboundSetTestBlockPacket),
        SignUpdate(ServerboundSignUpdatePacket),
        SpectatorAction(ServerboundSpectatorActionPacket),
        TeleportToEntity(ServerboundTeleportToEntityPacket),
        TestInstanceBlockAction(ServerboundTestInstanceBlockActionPacket),
        UseItemOn(ServerboundUseItemOnPacket),
        UseItem(ServerboundUseItemPacket),
        CustomClickAction(ServerboundCustomClickActionPacket),
    }
}
