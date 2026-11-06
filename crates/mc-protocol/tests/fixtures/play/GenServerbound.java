// Generates tests/fixtures/play/serverbound.golden (group `Serverbound`): real vanilla serverbound play packets.
// Usage: crates/mc-protocol/tests/fixtures/play/gen.sh GenServerbound.java
// Output: crates/mc-protocol/tests/fixtures/play/serverbound.golden
// Also emits, per decode-failure case, `<label>.in` (the malformed packet bytes) and `<label>.err` (hex of the UTF-8
// message of the exception vanilla's codec throws), and for lossy LpVec3 interactions `<label>.dec` (the three doubles
// vanilla decodes).
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.nio.charset.StandardCharsets;
import java.util.HexFormat;
import java.util.UUID;
import java.util.function.Consumer;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.PositionAndRotation;
import net.minecraft.core.Vec3i;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.Identifier;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.Difficulty;
import net.minecraft.world.effect.MobEffects;
import net.minecraft.world.entity.player.Input;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Mirror;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.entity.CommandBlockEntity;
import net.minecraft.world.level.block.entity.JigsawBlockEntity;
import net.minecraft.world.level.block.entity.SignTextSlot;
import net.minecraft.world.level.block.entity.StructureBlockEntity;
import net.minecraft.world.level.block.state.properties.StructureMode;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;

public class GenServerbound {
   static final BlockPos[] POSITIONS = {
      new BlockPos(0, 0, 0), new BlockPos(1, 2, 3), new BlockPos(-1, -2, -3), new BlockPos(33554431, 2047, 33554431),
      new BlockPos(-33554432, -2048, -33554432), new BlockPos(30000000, 319, -30000000)
   };
   static final String[] STRINGS = {"", "a", "hello world", "\u00e9\u4e2d\ud83d\ude00 \u00a7ctext", "x".repeat(300)};
   static final double[] DOUBLES = {0, -0.0, 1, -1, 0.5, 123456.789, -30000000.5, 1.0E300, Double.MIN_VALUE, Double.MAX_VALUE,
      Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, Double.NaN};
   static final float[] FLOATS = {0f, -0f, 1f, -1f, 90f, -180.5f, 359.99f, Float.MIN_VALUE, Float.MAX_VALUE,
      Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY, Float.NaN};
   static final int[] INTS = {0, 1, -1, 127, 128, 255, 16384, 1234567, Integer.MAX_VALUE, Integer.MIN_VALUE};

   public static void main(String[] args) throws Exception {
      PlayHarness.init(args[0]);
      InteractionHand[] hands = InteractionHand.values();

      // accept_teleportation
      for (int i = 0; i < INTS.length; i++) {
         PlayHarness.sb("accept_teleportation_" + i, new ServerboundAcceptTeleportationPacket(INTS[i], DOUBLES[i], DOUBLES[(i + 3) % DOUBLES.length],
            -DOUBLES[(i + 7) % 10], FLOATS[i], FLOATS[(i + 5) % 9]));
      }

      // attack / punch / tick end / player loaded / configuration ack
      for (int i = 0; i < INTS.length; i++) PlayHarness.sb("attack_" + i, new ServerboundAttackPacket(INTS[i]));
      PlayHarness.sb("punch", ServerboundPunchPacket.INSTANCE);
      PlayHarness.sb("client_tick_end", ServerboundClientTickEndPacket.INSTANCE);
      PlayHarness.sb("player_loaded", new ServerboundPlayerLoadedPacket());
      PlayHarness.sb("configuration_acknowledged", ServerboundConfigurationAcknowledgedPacket.INSTANCE);

      // change_difficulty / change_game_mode / lock_difficulty
      for (Difficulty d : Difficulty.values()) PlayHarness.sb("change_difficulty_" + d.name(), new ServerboundChangeDifficultyPacket(d));
      for (GameType g : GameType.values()) PlayHarness.sb("change_game_mode_" + g.name(), new ServerboundChangeGameModePacket(g));
      PlayHarness.sb("lock_difficulty_true", new ServerboundLockDifficultyPacket(true));
      PlayHarness.sb("lock_difficulty_false", new ServerboundLockDifficultyPacket(false));

      // chunk_batch_received
      for (int i = 0; i < FLOATS.length; i++) PlayHarness.sb("chunk_batch_received_" + i, new ServerboundChunkBatchReceivedPacket(FLOATS[i]));

      // client_command
      for (ServerboundClientCommandPacket.Action a : ServerboundClientCommandPacket.Action.values()) {
         PlayHarness.sb("client_command_" + a.name(), new ServerboundClientCommandPacket(a));
      }

      // interact: LpVec3 is lossy -> also record what vanilla decodes
      double[][] locations = {
         {0, 0, 0}, {1, 0, 0}, {0, 1, 0}, {0, 0, 1}, {-1, -1, -1}, {0.5, -0.25, 0.125}, {3, -3, 2}, {123.456, -7.5, 99.99},
         {1000.5, -2000.25, 3000.125}, {1.0E-6, 1.0E-6, 1.0E-6}, {3.0E-5, 0, 0}, {4, 0, 0}, {5, 5, 5}, {-17179869183.0, 17179869183.0, 0},
         {1.0E15, -1.0E15, 1.0E15}, {Double.NaN, 1, 2}, {Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, 0}
      };
      for (int i = 0; i < locations.length; i++) {
         for (InteractionHand h : hands) {
            for (boolean sec : new boolean[] {false, true}) {
               if (h == InteractionHand.OFF_HAND && !sec && i % 2 == 1) continue;
               ServerboundInteractPacket p = new ServerboundInteractPacket(INTS[i % INTS.length], h, new Vec3(locations[i][0], locations[i][1], locations[i][2]), sec);
               String label = "interact_" + i + "_" + h.name() + "_" + sec;
               PlayHarness.sb(label, p);
               ByteBuf b = Unpooled.buffer();
               ServerboundInteractPacket.STREAM_CODEC.encode(b, p);
               Vec3 d = ServerboundInteractPacket.STREAM_CODEC.decode(b).location();
               PlayHarness.raw(label + ".dec", w -> { w.writeDouble(d.x); w.writeDouble(d.y); w.writeDouble(d.z); });
            }
         }
      }

      // jigsaw_generate
      for (int i = 0; i < POSITIONS.length; i++) {
         PlayHarness.sb("jigsaw_generate_" + i, new ServerboundJigsawGeneratePacket(POSITIONS[i], INTS[i], i % 2 == 0));
      }

      // move_player_*: all flag combos x coordinates
      for (int fl = 0; fl < 4; fl++) {
         boolean og = (fl & 1) != 0, hc = (fl & 2) != 0;
         PlayHarness.sb("move_status_" + fl, new ServerboundMovePlayerPacket.StatusOnly(og, hc));
         for (int i = 0; i < DOUBLES.length; i++) {
            PlayHarness.sb("move_pos_" + fl + "_" + i, new ServerboundMovePlayerPacket.Pos(DOUBLES[i], DOUBLES[(i + 1) % DOUBLES.length], DOUBLES[(i + 2) % DOUBLES.length], og, hc));
            PlayHarness.sb("move_pos_rot_" + fl + "_" + i, new ServerboundMovePlayerPacket.PosRot(DOUBLES[i], DOUBLES[(i + 4) % DOUBLES.length],
               DOUBLES[(i + 9) % DOUBLES.length], FLOATS[i % FLOATS.length], FLOATS[(i + 3) % FLOATS.length], og, hc));
         }
         for (int i = 0; i < FLOATS.length; i++) {
            PlayHarness.sb("move_rot_" + fl + "_" + i, new ServerboundMovePlayerPacket.Rot(FLOATS[i], FLOATS[(i + 2) % FLOATS.length], og, hc));
         }
      }

      // move_vehicle
      for (int i = 0; i < DOUBLES.length; i++) {
         for (boolean og : new boolean[] {false, true}) {
            PlayHarness.sb("move_vehicle_" + i + "_" + og, new ServerboundMoveVehiclePacket(
               PositionAndRotation.of(new Vec3(DOUBLES[i], DOUBLES[(i + 5) % DOUBLES.length], DOUBLES[(i + 8) % DOUBLES.length]), FLOATS[i % FLOATS.length], FLOATS[(i + 7) % FLOATS.length]), og));
         }
      }

      // paddle_boat
      for (int fl = 0; fl < 4; fl++) PlayHarness.sb("paddle_boat_" + fl, new ServerboundPaddleBoatPacket((fl & 1) != 0, (fl & 2) != 0));

      // pick_item_from_block / pick_item_from_entity
      for (int i = 0; i < POSITIONS.length; i++) PlayHarness.sb("pick_item_from_block_" + i, new ServerboundPickItemFromBlockPacket(POSITIONS[i], i % 2 == 1));
      for (int i = 0; i < INTS.length; i++) PlayHarness.sb("pick_item_from_entity_" + i, new ServerboundPickItemFromEntityPacket(INTS[i], i % 2 == 1));

      // player_abilities: only `flying` is writable; crafted flag bytes cover the decode side (bits other than 2 ignored)
      PlayHarness.sb("player_abilities_false", new ServerboundPlayerAbilitiesPacket(abilities(false)));
      PlayHarness.sb("player_abilities_true", new ServerboundPlayerAbilitiesPacket(abilities(true)));
      for (int flags : new int[] {0, 1, 2, 3, 4, 0x7d, 0x7e, 0xff, 0x80}) {
         raw("player_abilities_byte_" + flags, 40, w -> w.writeByte(flags));
         decodeAs("player_abilities_byte_" + flags + ".flying", 40, "ServerboundPlayerAbilitiesPacket", (p) -> ((ServerboundPlayerAbilitiesPacket) p).isFlying());
      }

      // player_action: every action x every direction; position/sequence extremes
      int n = 0;
      for (ServerboundPlayerActionPacket.Action a : ServerboundPlayerActionPacket.Action.values()) {
         for (Direction d : Direction.values()) {
            PlayHarness.sb("player_action_" + a.name() + "_" + d.name(), new ServerboundPlayerActionPacket(a, POSITIONS[n % POSITIONS.length], d, INTS[n % INTS.length]));
            n++;
         }
      }
      // raw direction bytes (from3DDataValue folds anything mod 6), then expected decode as direction ids
      for (int db : new int[] {0, 5, 6, 7, 11, 12, 100, 127, 128, 200, 255}) {
         raw("player_action_dirbyte_" + db, 41, w -> { w.writeVarInt(0); w.writeBlockPos(POSITIONS[1]); w.writeByte(db); w.writeVarInt(7); });
         decodeAs("player_action_dirbyte_" + db + ".dir", 41, "ServerboundPlayerActionPacket", (p) -> ((ServerboundPlayerActionPacket) p).getDirection().get3DDataValue());
      }

      // player_command
      n = 0;
      for (ServerboundPlayerCommandPacket.Action a : ServerboundPlayerCommandPacket.Action.values()) {
         for (int data : new int[] {0, 1, 100, 1 << 20, -1, Integer.MAX_VALUE, Integer.MIN_VALUE}) {
            PlayHarness.sb("player_command_" + a.name() + "_" + n, playerCommand(INTS[n % INTS.length], a, data));
            n++;
         }
      }

      // player_input: all 128 combos; high bit crafted
      for (int m = 0; m < 128; m++) {
         PlayHarness.sb("player_input_" + m, new ServerboundPlayerInputPacket(new Input((m & 1) != 0, (m & 2) != 0, (m & 4) != 0, (m & 8) != 0, (m & 16) != 0, (m & 32) != 0, (m & 64) != 0)));
      }
      raw("player_input_byte_255", 43, w -> w.writeByte(255));
      raw("player_input_byte_128", 43, w -> w.writeByte(128));

      // set_beacon
      java.util.List<net.minecraft.core.Holder<net.minecraft.world.effect.MobEffect>> effects = java.util.List.of(MobEffects.SPEED, MobEffects.SLOWNESS, MobEffects.LUCK, MobEffects.INFESTED);
      for (int i = 0; i < effects.size(); i++) {
         for (int j = 0; j < effects.size(); j++) {
            PlayHarness.sb("set_beacon_" + i + "_" + j, new ServerboundSetBeaconPacket(java.util.Optional.of(effects.get(i)), java.util.Optional.of(effects.get(j))));
         }
         PlayHarness.sb("set_beacon_" + i + "_none", new ServerboundSetBeaconPacket(java.util.Optional.of(effects.get(i)), java.util.Optional.empty()));
         PlayHarness.sb("set_beacon_none_" + i, new ServerboundSetBeaconPacket(java.util.Optional.empty(), java.util.Optional.of(effects.get(i))));
      }
      PlayHarness.sb("set_beacon_none_none", new ServerboundSetBeaconPacket(java.util.Optional.empty(), java.util.Optional.empty()));

      // set_command_block: all modes x all 8 flag combos x strings
      n = 0;
      for (CommandBlockEntity.Mode m : CommandBlockEntity.Mode.values()) {
         for (int fl = 0; fl < 8; fl++) {
            PlayHarness.sb("set_command_block_" + m.name() + "_" + fl, new ServerboundSetCommandBlockPacket(POSITIONS[n % POSITIONS.length],
               STRINGS[n % STRINGS.length], m, (fl & 1) != 0, (fl & 2) != 0, (fl & 4) != 0));
            n++;
         }
      }
      PlayHarness.sb("set_command_block_max", new ServerboundSetCommandBlockPacket(POSITIONS[1], "c".repeat(32767), CommandBlockEntity.Mode.AUTO, true, true, true));
      for (int flags : new int[] {0, 7, 8, 0x78, 0xff}) {
         raw("set_command_block_byte_" + flags, 55, w -> { w.writeBlockPos(POSITIONS[1]); w.writeUtf("cmd"); w.writeVarInt(1); w.writeByte(flags); });
      }

      // set_command_minecart
      for (int i = 0; i < STRINGS.length; i++) {
         PlayHarness.sb("set_command_minecart_" + i, new ServerboundSetCommandMinecartPacket(INTS[i], STRINGS[i], i % 2 == 0));
      }
      PlayHarness.sb("set_command_minecart_max", new ServerboundSetCommandMinecartPacket(5, "c".repeat(32767), false));

      // set_jigsaw_block
      Identifier[] ids = {Identifier.parse("minecraft:empty"), Identifier.parse("my_pack:some/pool_1"), Identifier.parse("a.b:c-d/e_f")};
      n = 0;
      for (JigsawBlockEntity.JointType j : JigsawBlockEntity.JointType.values()) {
         for (int i = 0; i < 3; i++) {
            PlayHarness.sb("set_jigsaw_block_" + j.name() + "_" + i, new ServerboundSetJigsawBlockPacket(POSITIONS[(n++) % POSITIONS.length], ids[i], ids[(i + 1) % 3],
               ids[(i + 2) % 3], STRINGS[i], j, INTS[i + 3], INTS[(i * 3) % INTS.length]));
         }
      }
      // unknown joint names decode as ALIGNED
      for (String joint : new String[] {"rollable", "aligned", "ROLLABLE", "bogus", ""}) {
         raw("set_jigsaw_block_joint_" + joint, 59, w -> {
            w.writeBlockPos(POSITIONS[1]); w.writeIdentifier(ids[0]); w.writeIdentifier(ids[1]); w.writeIdentifier(ids[2]);
            w.writeUtf("minecraft:air"); w.writeUtf(joint); w.writeVarInt(3); w.writeVarInt(-4);
         });
         decodeAs("set_jigsaw_block_joint_" + joint + ".joint", 59, "ServerboundSetJigsawBlockPacket", (p) -> ((ServerboundSetJigsawBlockPacket) p).getJoint().ordinal());
      }

      // set_structure_block: every enum constant, flag combos, extremes
      n = 0;
      for (StructureBlockEntity.UpdateType u : StructureBlockEntity.UpdateType.values()) {
         for (StructureMode sm : StructureMode.values()) {
            Mirror mi = Mirror.values()[n % 3];
            Rotation ro = Rotation.values()[n % 4];
            int fl = n % 16;
            PlayHarness.sb("set_structure_block_" + u.name() + "_" + sm.name(), new ServerboundSetStructureBlockPacket(POSITIONS[n % POSITIONS.length], u, sm,
               STRINGS[n % STRINGS.length], new BlockPos(n - 8, 48 - n, 0), new Vec3i(n, 48 - n, 24), mi, ro, STRINGS[(n + 1) % 4],
               (fl & 1) != 0, (fl & 8) != 0, (fl & 2) != 0, (fl & 4) != 0, n / 15f, (long) n * 0x123456789L - 5));
            n++;
         }
      }
      for (Mirror mi : Mirror.values()) for (Rotation ro : Rotation.values()) {
         PlayHarness.sb("set_structure_block_" + mi.name() + "_" + ro.name(), new ServerboundSetStructureBlockPacket(POSITIONS[2], StructureBlockEntity.UpdateType.SAVE_AREA,
            StructureMode.SAVE, "s", new BlockPos(-48, 48, 0), new Vec3i(0, 48, 0), mi, ro, "", false, false, false, false, 0f, 0L));
      }
      for (int fl = 0; fl < 16; fl++) {
         PlayHarness.sb("set_structure_block_flags_" + fl, new ServerboundSetStructureBlockPacket(POSITIONS[0], StructureBlockEntity.UpdateType.LOAD_AREA,
            StructureMode.LOAD, "n", BlockPos.ZERO, new Vec3i(1, 2, 3), Mirror.NONE, Rotation.NONE, "d", (fl & 1) != 0, (fl & 8) != 0, (fl & 2) != 0, (fl & 4) != 0, 1f, Long.MIN_VALUE));
      }
      for (long seed : new long[] {0, 1, -1, 127, 128, Long.MAX_VALUE, Long.MIN_VALUE, 1L << 35}) {
         PlayHarness.sb("set_structure_block_seed_" + seed, new ServerboundSetStructureBlockPacket(POSITIONS[1], StructureBlockEntity.UpdateType.SCAN_AREA,
            StructureMode.DATA, "seed", new BlockPos(1, -1, 48), new Vec3i(48, 0, 1), Mirror.LEFT_RIGHT, Rotation.CLOCKWISE_180, "x".repeat(128), true, true, true, true, 0.5f, seed));
      }
      // decode clamping: crafted offset/size bytes and integrity floats
      for (int[] b : new int[][] {{0, 0, 0, 0, 0, 0}, {48, 48, 48, 48, 48, 48}, {49, 100, 127, 49, 100, 127}, {-48, -48, -48, -1, -2, -128}, {-49, -128, -100, -49, -128, 0}, {255, 129, 200, 255, 130, 131}}) {
         String l = "set_structure_block_bytes_" + b[0] + "_" + b[1] + "_" + b[2] + "_" + b[3] + "_" + b[4] + "_" + b[5];
         raw(l, 60, w -> structureBody(w, b, 0.5f, 0));
         decodeAs(l + ".dec", 60, "ServerboundSetStructureBlockPacket", p -> {
            ServerboundSetStructureBlockPacket q = (ServerboundSetStructureBlockPacket) p;
            return q.getOffset().getX() + "," + q.getOffset().getY() + "," + q.getOffset().getZ() + "," + q.getSize().getX() + "," + q.getSize().getY() + "," + q.getSize().getZ();
         });
      }
      for (float f : new float[] {-1f, -0f, 0f, 0.25f, 1f, 1.5f, Float.MAX_VALUE, Float.NEGATIVE_INFINITY, Float.POSITIVE_INFINITY, Float.NaN}) {
         String l = "set_structure_block_integrity_" + Float.floatToRawIntBits(f);
         raw(l, 60, w -> structureBody(w, new int[] {0, 0, 0, 0, 0, 0}, f, 0));
         decodeAs(l + ".dec", 60, "ServerboundSetStructureBlockPacket", p -> Integer.toHexString(Float.floatToRawIntBits(((ServerboundSetStructureBlockPacket) p).getIntegrity())));
      }
      for (int flags : new int[] {0, 15, 16, 0xf0, 0xff}) {
         String l = "set_structure_block_flagbyte_" + flags;
         raw(l, 60, w -> structureBody(w, new int[] {0, 0, 0, 0, 0, 0}, 1f, flags));
      }
      raw("set_structure_block_wrap_rotation_5", 60, w -> structureBody(w, new int[6], 1f, 0, 0, 5));
      raw("set_structure_block_wrap_rotation_neg1", 60, w -> structureBody(w, new int[6], 1f, 0, 0, -1));
      decodeAs("set_structure_block_wrap_rotation_5.rot", 60, "ServerboundSetStructureBlockPacket", p -> ((ServerboundSetStructureBlockPacket) p).getRotation().ordinal());
      decodeAs("set_structure_block_wrap_rotation_neg1.rot", 60, "ServerboundSetStructureBlockPacket", p -> ((ServerboundSetStructureBlockPacket) p).getRotation().ordinal());

      // sign_update
      n = 0;
      for (SignTextSlot s : SignTextSlot.values()) {
         for (int i = 0; i < 4; i++) {
            PlayHarness.sb("sign_update_" + s.name() + "_" + i, new ServerboundSignUpdatePacket(POSITIONS[n % POSITIONS.length],
               java.util.List.of(STRINGS[i % 5], STRINGS[(i + 1) % 5], STRINGS[(i + 2) % 5], STRINGS[(i + 3) % 5]), s));
            n++;
         }
      }
      String l384 = "\ud83d\ude00".repeat(192);
      PlayHarness.sb("sign_update_max", new ServerboundSignUpdatePacket(POSITIONS[1], java.util.List.of("m".repeat(384), l384, "", "z"), SignTextSlot.FRONT));
      raw("sign_update_slot_2", 62, w -> { w.writeBlockPos(POSITIONS[1]); for (int i = 0; i < 4; i++) w.writeUtf(""); w.writeVarInt(2); });
      raw("sign_update_slot_neg1", 62, w -> { w.writeBlockPos(POSITIONS[1]); for (int i = 0; i < 4; i++) w.writeUtf(""); w.writeVarInt(-1); });
      decodeAs("sign_update_slot_2.slot", 62, "ServerboundSignUpdatePacket", p -> ((ServerboundSignUpdatePacket) p).slot().ordinal());
      decodeAs("sign_update_slot_neg1.slot", 62, "ServerboundSignUpdatePacket", p -> ((ServerboundSignUpdatePacket) p).slot().ordinal());

      // spectator_action
      for (java.util.OptionalInt o : new java.util.OptionalInt[] {java.util.OptionalInt.empty(), java.util.OptionalInt.of(0), java.util.OptionalInt.of(1), java.util.OptionalInt.of(-1),
         java.util.OptionalInt.of(300), java.util.OptionalInt.of(Integer.MAX_VALUE), java.util.OptionalInt.of(Integer.MIN_VALUE)}) {
         PlayHarness.sb("spectator_action_" + (o.isPresent() ? o.getAsInt() : "none"), new ServerboundSpectatorActionPacket(o));
      }

      // teleport_to_entity
      for (UUID u : new UUID[] {new UUID(0, 0), new UUID(-1, -1), UUID.fromString("069a79f4-44e9-4726-a5be-fca90e38aaf5"), new UUID(Long.MIN_VALUE, Long.MAX_VALUE)}) {
         PlayHarness.sb("teleport_to_entity_" + u, new ServerboundTeleportToEntityPacket(u));
      }

      // use_item_on: every direction x hand x flags; click offsets (float) and positions
      float[] clicks = {0f, 0.5f, 1f, -0.25f, 0.999f, 1.5f, Float.NaN, 1.0E-30f};
      n = 0;
      for (Direction d : Direction.values()) {
         for (InteractionHand h : hands) {
            for (int fl = 0; fl < 4; fl++) {
               BlockPos pos = POSITIONS[n % POSITIONS.length];
               Vec3 loc = new Vec3(pos.getX() + clicks[n % 6], pos.getY() + clicks[(n + 1) % 6], pos.getZ() + clicks[(n + 2) % 6]);
               PlayHarness.sb("use_item_on_" + d.name() + "_" + h.name() + "_" + fl, new ServerboundUseItemOnPacket(h,
                  new BlockHitResult(loc, d, pos, (fl & 1) != 0, (fl & 2) != 0), INTS[n % INTS.length]));
               n++;
            }
         }
      }
      for (int i = 0; i < clicks.length; i++) {
         BlockPos pos = POSITIONS[i % POSITIONS.length];
         PlayHarness.sb("use_item_on_click_" + i, new ServerboundUseItemOnPacket(InteractionHand.MAIN_HAND,
            new BlockHitResult(new Vec3(pos.getX() + clicks[i], pos.getY() + clicks[(i + 3) % 8], pos.getZ() + clicks[(i + 5) % 8]), Direction.UP, pos, false), 0));
      }
      for (int db : new int[] {6, 7, 11, -1, -7, 600}) {
         raw("use_item_on_dir_" + db, 66, w -> { w.writeVarInt(0); w.writeBlockPos(POSITIONS[1]); w.writeVarInt(db); w.writeFloat(0.5f); w.writeFloat(0.5f); w.writeFloat(0.5f); w.writeBoolean(false); w.writeBoolean(false); w.writeVarInt(0); });
         decodeAs("use_item_on_dir_" + db + ".dir", 66, "ServerboundUseItemOnPacket", p -> ((ServerboundUseItemOnPacket) p).hitResult().getDirection().get3DDataValue());
      }
      for (int hv : new int[] {2, 3, -1, 100}) {
         raw("use_item_on_hand_" + hv, 66, w -> { w.writeVarInt(hv); w.writeBlockPos(POSITIONS[1]); w.writeVarInt(1); w.writeFloat(0.5f); w.writeFloat(0.5f); w.writeFloat(0.5f); w.writeBoolean(false); w.writeBoolean(false); w.writeVarInt(0); });
         decodeAs("use_item_on_hand_" + hv + ".hand", 66, "ServerboundUseItemOnPacket", p -> ((ServerboundUseItemOnPacket) p).hand().ordinal());
      }

      // use_item
      for (InteractionHand h : hands) {
         for (int i = 0; i < FLOATS.length; i++) {
            PlayHarness.sb("use_item_" + h.name() + "_" + i, new ServerboundUseItemPacket(h, INTS[i % INTS.length], FLOATS[i], FLOATS[(i + 4) % FLOATS.length]));
         }
      }

      // ---- decode failures: bytes + vanilla's exception message ----
      err("err_client_command_3", 12, w -> w.writeVarInt(3));
      err("err_client_command_neg", 12, w -> w.writeVarInt(-1));
      err("err_client_command_big", 12, w -> w.writeVarInt(Integer.MAX_VALUE));
      err("err_player_action_9", 41, w -> { w.writeVarInt(9); w.writeBlockPos(BlockPos.ZERO); w.writeByte(0); w.writeVarInt(0); });
      err("err_player_action_neg", 41, w -> { w.writeVarInt(-5); w.writeBlockPos(BlockPos.ZERO); w.writeByte(0); w.writeVarInt(0); });
      err("err_player_command_7", 42, w -> { w.writeVarInt(1); w.writeVarInt(7); w.writeVarInt(0); });
      err("err_player_command_neg", 42, w -> { w.writeVarInt(1); w.writeVarInt(Integer.MIN_VALUE); w.writeVarInt(0); });
      err("err_set_command_block_mode_3", 55, w -> { w.writeBlockPos(BlockPos.ZERO); w.writeUtf("x"); w.writeVarInt(3); w.writeByte(0); });
      err("err_set_command_block_long", 55, w -> { w.writeBlockPos(BlockPos.ZERO); w.writeUtf("x".repeat(32768), 40000); w.writeVarInt(0); w.writeByte(0); });
      err("err_set_command_block_bytes_long", 55, w -> { w.writeBlockPos(BlockPos.ZERO); w.writeVarInt(32767 * 3 + 1); w.writeBytes(new byte[10]); });
      err("err_set_command_minecart_long", 56, w -> { w.writeVarInt(1); w.writeUtf("x".repeat(32768), 40000); w.writeBoolean(false); });
      err("err_set_jigsaw_block_bad_id", 59, w -> { w.writeBlockPos(BlockPos.ZERO); w.writeUtf("Not A Valid:ID"); });
      err("err_set_jigsaw_block_state_long", 59, w -> { w.writeBlockPos(BlockPos.ZERO); w.writeIdentifier(ids[0]); w.writeIdentifier(ids[0]); w.writeIdentifier(ids[0]); w.writeUtf("x".repeat(32768), 40000); });
      err("err_set_structure_block_update_4", 60, w -> { w.writeBlockPos(BlockPos.ZERO); w.writeVarInt(4); });
      err("err_set_structure_block_mode_4", 60, w -> { w.writeBlockPos(BlockPos.ZERO); w.writeVarInt(0); w.writeVarInt(4); });
      err("err_set_structure_block_mirror_3", 60, w -> structureBody(w, new int[6], 1f, 0, 3, 0));
      err("err_set_structure_block_mirror_neg", 60, w -> structureBody(w, new int[6], 1f, 0, -1, 0));
      err("err_set_structure_block_data_129", 60, w -> structureBody(w, new int[6], 1f, 0, 0, 0, "d".repeat(129)));
      err("err_set_structure_block_data_bytes", 60, w -> structureBody(w, new int[6], 1f, 0, 0, 0, "d".repeat(128 * 3 + 1)));
      err("err_sign_update_385", 62, w -> { w.writeBlockPos(BlockPos.ZERO); w.writeUtf("a"); w.writeUtf("b"); w.writeUtf("c".repeat(385), 400); });
      err("err_sign_update_emoji_385", 62, w -> { w.writeBlockPos(BlockPos.ZERO); w.writeUtf("a"); w.writeUtf("\ud83d\ude00".repeat(193), 400); });
      err("err_sign_update_bytes", 62, w -> { w.writeBlockPos(BlockPos.ZERO); w.writeVarInt(384 * 3 + 1); });
      err("err_set_beacon_effect_unknown", 53, w -> { w.writeBoolean(true); w.writeVarInt(100000); });
      err("err_set_beacon_effect_neg", 53, w -> { w.writeBoolean(true); w.writeVarInt(-1); });
      err("err_set_beacon_second_unknown", 53, w -> { w.writeBoolean(false); w.writeBoolean(true); w.writeVarInt(1000); });
      err("err_truncated_move_pos", 30, w -> w.writeDouble(1.0));
      err("err_teleport_to_entity_short", 64, w -> w.writeLong(1));

      PlayHarness.close();
   }

   static net.minecraft.world.entity.player.Abilities abilities(boolean flying) {
      net.minecraft.world.entity.player.Abilities a = new net.minecraft.world.entity.player.Abilities();
      a.flying = flying;
      return a;
   }

   /** `ServerboundPlayerCommandPacket` only has an `Entity` constructor: build the packet through its stream codec instead. */
   static ServerboundPlayerCommandPacket playerCommand(int id, ServerboundPlayerCommandPacket.Action a, int data) {
      FriendlyByteBuf b = new FriendlyByteBuf(Unpooled.buffer());
      b.writeVarInt(id);
      b.writeEnum(a);
      b.writeVarInt(data);
      return ServerboundPlayerCommandPacket.STREAM_CODEC.decode(b);
   }

   static void structureBody(RegistryFriendlyByteBuf w, int[] b, float integrity, int flags) {
      structureBody(w, b, integrity, flags, 0, 0);
   }

   static void structureBody(RegistryFriendlyByteBuf w, int[] b, float integrity, int flags, int mirror, int rotation) {
      structureBody(w, b, integrity, flags, mirror, rotation, "data");
   }

   static void structureBody(RegistryFriendlyByteBuf w, int[] b, float integrity, int flags, int mirror, int rotation, String data) {
      w.writeBlockPos(BlockPos.ZERO);
      w.writeVarInt(2);
      w.writeVarInt(1);
      w.writeUtf("name");
      for (int v : b) w.writeByte(v);
      w.writeVarInt(mirror);
      w.writeVarInt(rotation);
      w.writeUtf(data, 1000);
      w.writeFloat(integrity);
      w.writeVarLong(42);
      w.writeByte(flags);
   }

   static final java.util.Map<String, byte[]> lastRaw = new java.util.HashMap<>();

   /** Emits `label` = packet id + the body written by `writer` (remembered for `decodeAs`). */
   static void raw(String label, int id, Consumer<RegistryFriendlyByteBuf> writer) {
      RegistryFriendlyByteBuf b = new RegistryFriendlyByteBuf(Unpooled.buffer(), PlayHarness.access);
      writer.accept(b);
      byte[] body = new byte[b.readableBytes()];
      b.getBytes(0, body);
      lastRaw.put(label, body);
      PlayHarness.raw(label, w -> { w.writeVarInt(id); w.writeBytes(body); });
   }

   static net.minecraft.network.protocol.Packet<?> decode(int id, Consumer<RegistryFriendlyByteBuf> writer) {
      RegistryFriendlyByteBuf buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), PlayHarness.access);
      buf.writeVarInt(id);
      writer.accept(buf);
      @SuppressWarnings({"rawtypes", "unchecked"})
      net.minecraft.network.protocol.Packet<?> p = (net.minecraft.network.protocol.Packet<?>) ((net.minecraft.network.codec.StreamCodec) PlayHarness.serverbound.codec()).decode(buf);
      return p;
   }

   /** The bytes written for `rawLabel` are decoded by vanilla and `extract(packet)` is recorded as `label` (utf-8 text hex). */
   static void decodeAs(String label, int id, String type, java.util.function.Function<Object, Object> extract) {
      String rawLabel = label.substring(0, label.lastIndexOf('.'));
      byte[] bytes = lastRaw.get(rawLabel);
      Object v = extract.apply(decode(id, w -> w.writeBytes(bytes)));
      PlayHarness.raw(label, w -> w.writeBytes(String.valueOf(v).getBytes(StandardCharsets.UTF_8)));
   }

   /** Emits `label.in` (id + body) and `label.err` (hex of the root-cause exception message vanilla's codec throws on it). */
   static void err(String label, int id, Consumer<RegistryFriendlyByteBuf> writer) {
      raw(label + ".in", id, writer);
      String message;
      try {
         decode(id, writer);
         throw new AssertionError("vanilla accepted " + label);
      } catch (AssertionError e) {
         throw e;
      } catch (Throwable t) {
         while (t.getCause() != null) t = t.getCause();
         message = String.valueOf(t.getMessage());
      }
      String m = message;
      PlayHarness.raw(label + ".err", w -> w.writeBytes(m.getBytes(StandardCharsets.UTF_8)));
   }
}
