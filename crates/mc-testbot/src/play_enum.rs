//! TEMPORARY stand-in for `mc_protocol::packets::play::{ClientboundPlayPacket, ServerboundPlayPacket}`
//! (replaced by a re-export once PlayPacketsAgent lands the real enums).
use mc_protocol::packet_enum;
use mc_protocol::packets::play::*;

packet_enum! {
    pub enum ClientboundPlayPacket {
        BlockChangedAck(ClientboundBlockChangedAckPacket),
        BlockUpdate(ClientboundBlockUpdatePacket),
        ChunkBatchFinished(ClientboundChunkBatchFinishedPacket),
        ChunkBatchStart(ClientboundChunkBatchStartPacket),
        CustomPayload(ClientboundCustomPayloadPacket),
        Disconnect(ClientboundDisconnectPacket),
        ForgetLevelChunk(ClientboundForgetLevelChunkPacket),
        GameEvent(ClientboundGameEventPacket),
        KeepAlive(ClientboundKeepAlivePacket),
        LevelChunkWithLight(ClientboundLevelChunkWithLightPacket),
        Login(ClientboundLoginPacket),
        Ping(ClientboundPingPacket),
        PlayerAbilities(ClientboundPlayerAbilitiesPacket),
        PlayerPosition(ClientboundPlayerPositionPacket),
        PlayerRotation(ClientboundPlayerRotationPacket),
        Respawn(ClientboundRespawnPacket),
        SectionBlocksUpdate(ClientboundSectionBlocksUpdatePacket),
        SetChunkCacheCenter(ClientboundSetChunkCacheCenterPacket),
        SetTime(ClientboundSetTimePacket),
        StartConfiguration(ClientboundStartConfigurationPacket),
        Transfer(ClientboundTransferPacket),
    }
}

packet_enum! {
    pub enum ServerboundPlayPacket {
        AcceptTeleportation(ServerboundAcceptTeleportationPacket),
        Attack(ServerboundAttackPacket),
        ChunkBatchReceived(ServerboundChunkBatchReceivedPacket),
        ClientCommand(ServerboundClientCommandPacket),
        ClientInformation(ServerboundClientInformationPacket),
        ClientTickEnd(ServerboundClientTickEndPacket),
        ConfigurationAcknowledged(ServerboundConfigurationAcknowledgedPacket),
        CustomPayload(ServerboundCustomPayloadPacket),
        Interact(ServerboundInteractPacket),
        KeepAlive(ServerboundKeepAlivePacket),
        MovePlayerPos(ServerboundMovePlayerPosPacket),
        MovePlayerPosRot(ServerboundMovePlayerPosRotPacket),
        MovePlayerRot(ServerboundMovePlayerRotPacket),
        MovePlayerStatusOnly(ServerboundMovePlayerStatusOnlyPacket),
        PingRequest(ServerboundPingRequestPacket),
        PlayerAbilities(ServerboundPlayerAbilitiesPacket),
        PlayerAction(ServerboundPlayerActionPacket),
        PlayerCommand(ServerboundPlayerCommandPacket),
        PlayerInput(ServerboundPlayerInputPacket),
        PlayerLoaded(ServerboundPlayerLoadedPacket),
        Pong(ServerboundPongPacket),
        UseItem(ServerboundUseItemPacket),
        UseItemOn(ServerboundUseItemOnPacket),
    }
}

crate::wire_packet!(ClientboundPlayPacket, Play, Clientbound);
crate::wire_packet!(ServerboundPlayPacket, Play, Serverbound);
