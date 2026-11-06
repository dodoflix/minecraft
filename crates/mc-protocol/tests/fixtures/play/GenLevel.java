// Golden generator for the `level` play packet group (tests/golden/play_level.rs).
// Usage: crates/mc-protocol/tests/fixtures/play/gen.sh GenLevel.java
// Output: crates/mc-protocol/tests/fixtures/play/level.golden
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.GlobalPos;
import net.minecraft.core.Holder;
import net.minecraft.core.SectionPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.StringTag;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.Difficulty;
import net.minecraft.world.clock.ClockNetworkState;
import net.minecraft.world.clock.WorldClock;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.storage.LevelData;

public class GenLevel {
   /** Instance of `c` without running a constructor, with the given private fields set (name, value, ...). */
   @SuppressWarnings("unchecked")
   static <T> T make(Class<T> c, Object... kv) throws Exception {
      Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
      uf.setAccessible(true);
      T o = (T) ((sun.misc.Unsafe) uf.get(null)).allocateInstance(c);
      for (int i = 0; i < kv.length; i += 2) {
         Field f = c.getDeclaredField((String) kv[i]);
         f.setAccessible(true);
         f.set(o, kv[i + 1]);
      }
      return o;
   }

   static BlockPos[] POSITIONS = {
      new BlockPos(0, 0, 0), new BlockPos(1, 64, -1), new BlockPos(-30000000, -64, 29999999), new BlockPos(33554431, 2047, -33554432)
   };

   static Holder<SoundEvent> sound(int id) {
      return BuiltInRegistries.SOUND_EVENT.wrapAsHolder(BuiltInRegistries.SOUND_EVENT.byIdOrThrow(id));
   }

   static CommonPlayerSpawnInfo spawn(int dimType, ResourceKey<Level> dim, long seed, GameType gt, Optional<GameType> prev, boolean dbg, boolean flat,
         Optional<GlobalPos> death, int cooldown, int sea) {
      var reg = PlayHarness.access.lookupOrThrow(Registries.DIMENSION_TYPE);
      Holder<DimensionType> h = reg.wrapAsHolder(reg.byIdOrThrow(dimType));
      return new CommonPlayerSpawnInfo(h, dim, seed, gt, prev, dbg, flat, death, cooldown, sea);
   }

   static ResourceKey<Level> dim(String id) {
      return ResourceKey.create(Registries.DIMENSION, Identifier.parse(id));
   }

   static byte[] bytes(int n, int seed) {
      byte[] b = new byte[n];
      for (int i = 0; i < n; i++) b[i] = (byte) (i * 31 + seed * 7 + (i >> 8) * 13);
      return b;
   }

   static long[] longs(int n, long seed) {
      long[] l = new long[n];
      for (int i = 0; i < n; i++) l[i] = seed * 0x9E3779B97F4A7C15L + i * 0x632BE59BD9B4E019L;
      return l;
   }

   public static void main(String[] args) throws Exception {
      PlayHarness.init(args[0]);

      // ---- blocks
      for (int s : new int[]{0, 1, 127, 128, 16384, 2147483647, -1})
         PlayHarness.cb("block_changed_ack/" + s, new ClientboundBlockChangedAckPacket(s));
      for (int i = 0; i < POSITIONS.length; i++) {
         PlayHarness.cb("block_destruction/" + i, new ClientboundBlockDestructionPacket(i * 1000 - 1, POSITIONS[i], i == 3 ? 255 : i * 3));
      }
      PlayHarness.cb("block_destruction/progress_clear", new ClientboundBlockDestructionPacket(7, POSITIONS[1], 255));
      for (int id : new int[]{0, 1, 100, 5000, 20000})
         PlayHarness.cb("block_update/" + id, new ClientboundBlockUpdatePacket(POSITIONS[id % 4], Block.BLOCK_STATE_REGISTRY.byIdOrThrow(id)));
      int lastState = Block.BLOCK_STATE_REGISTRY.size() - 1;
      System.out.println("last block state id " + lastState + ", blocks " + BuiltInRegistries.BLOCK.size() + ", block entity types "
         + BuiltInRegistries.BLOCK_ENTITY_TYPE.size() + ", sounds " + BuiltInRegistries.SOUND_EVENT.size());
      PlayHarness.cb("block_update/last", new ClientboundBlockUpdatePacket(POSITIONS[2], Block.BLOCK_STATE_REGISTRY.byIdOrThrow(lastState)));
      for (int id : new int[]{0, 1, 99, lastState})
         PlayHarness.cb("add_transient_block/" + id, new ClientboundAddTransientBlockPacket(POSITIONS[id % 4], Block.BLOCK_STATE_REGISTRY.byIdOrThrow(id)));
      for (int id : new int[]{0, 1, 200, BuiltInRegistries.BLOCK.size() - 1})
         PlayHarness.cb("block_event/" + id, new ClientboundBlockEventPacket(POSITIONS[id % 4], BuiltInRegistries.BLOCK.byIdOrThrow(id), id % 256, 255 - id % 256));
      PlayHarness.cb("block_event/max", new ClientboundBlockEventPacket(POSITIONS[1], BuiltInRegistries.BLOCK.byIdOrThrow(5), 255, 255));

      CompoundTag sign = new CompoundTag();
      sign.putBoolean("is_waxed", true);
      sign.putString("name", "\uD83D\uDE00 sign \u00e9");
      ListTag lines = new ListTag();
      lines.add(StringTag.valueOf("a"));
      lines.add(StringTag.valueOf(""));
      sign.put("lines", lines);
      sign.putLong("big", Long.MIN_VALUE);
      sign.putDouble("d", -0.0);
      for (int id : new int[]{0, 7, BuiltInRegistries.BLOCK_ENTITY_TYPE.size() - 1}) {
         BlockEntityType<?> t = BuiltInRegistries.BLOCK_ENTITY_TYPE.byIdOrThrow(id);
         Constructor<ClientboundBlockEntityDataPacket> k =
            ClientboundBlockEntityDataPacket.class.getDeclaredConstructor(BlockPos.class, BlockEntityType.class, CompoundTag.class);
         k.setAccessible(true);
         PlayHarness.cb("block_entity_data/" + id, k.newInstance(POSITIONS[id % 4], t, sign));
         PlayHarness.cb("block_entity_data/" + id + "_empty", k.newInstance(POSITIONS[id % 4], t, new CompoundTag()));
      }

      // ---- section blocks update (private fields; one `positions[i]` = x << 8 | z << 4 | y)
      PlayHarness.cb("section_blocks_update/empty", make(ClientboundSectionBlocksUpdatePacket.class,
         "sectionPos", SectionPos.of(0, 0, 0), "positions", new short[0], "states", new net.minecraft.world.level.block.state.BlockState[0]));
      PlayHarness.cb("section_blocks_update/one", make(ClientboundSectionBlocksUpdatePacket.class,
         "sectionPos", SectionPos.of(-5, 3, 2000000), "positions", new short[]{(short) 0x123},
         "states", new net.minecraft.world.level.block.state.BlockState[]{Block.BLOCK_STATE_REGISTRY.byIdOrThrow(1)}));
      short[] pos = {0, 0xfff, 0x0f0, 0x00f, 0xf00, 0x456};
      int[] ids = {0, 1, 127, 128, lastState, 30000};
      var states = new net.minecraft.world.level.block.state.BlockState[pos.length];
      for (int i = 0; i < pos.length; i++) states[i] = Block.BLOCK_STATE_REGISTRY.byIdOrThrow(ids[i]);
      PlayHarness.cb("section_blocks_update/many", make(ClientboundSectionBlocksUpdatePacket.class,
         "sectionPos", SectionPos.of(-2097152, -2048, 2097151), "positions", pos, "states", states));
      int n = 300;
      short[] bp = new short[n];
      var bs = new net.minecraft.world.level.block.state.BlockState[n];
      for (int i = 0; i < n; i++) {
         bp[i] = (short) (i * 13 % 4096);
         bs[i] = Block.BLOCK_STATE_REGISTRY.byIdOrThrow(i * 37 % lastState);
      }
      PlayHarness.cb("section_blocks_update/large", make(ClientboundSectionBlocksUpdatePacket.class,
         "sectionPos", SectionPos.of(10, -4, -10), "positions", bp, "states", bs));

      // ---- difficulty, batches, events
      for (Difficulty d : Difficulty.values()) {
         PlayHarness.cb("change_difficulty/" + d.name() + "/open", new ClientboundChangeDifficultyPacket(d, false));
         PlayHarness.cb("change_difficulty/" + d.name() + "/locked", new ClientboundChangeDifficultyPacket(d, true));
      }
      for (int s : new int[]{0, 1, 5, 300, 2147483647})
         PlayHarness.cb("chunk_batch_finished/" + s, new ClientboundChunkBatchFinishedPacket(s));
      PlayHarness.cb("chunk_batch_start", ClientboundChunkBatchStartPacket.INSTANCE);
      PlayHarness.cb("low_disk_space_warning", ClientboundLowDiskSpaceWarningPacket.INSTANCE);
      PlayHarness.cb("start_configuration", ClientboundStartConfigurationPacket.INSTANCE);

      PlayHarness.cb("chunks_biomes/empty", new ClientboundChunksBiomesPacket(List.of()));
      PlayHarness.cb("chunks_biomes/one_empty_buffer", new ClientboundChunksBiomesPacket(List.of(new ClientboundChunksBiomesPacket.ChunkBiomeData(new ChunkPos(0, 0), new byte[0]))));
      PlayHarness.cb("chunks_biomes/many", new ClientboundChunksBiomesPacket(List.of(
         new ClientboundChunksBiomesPacket.ChunkBiomeData(new ChunkPos(-1, 1), bytes(40, 1)),
         new ClientboundChunksBiomesPacket.ChunkBiomeData(new ChunkPos(Integer.MIN_VALUE, Integer.MAX_VALUE), bytes(300, 2)),
         new ClientboundChunksBiomesPacket.ChunkBiomeData(new ChunkPos(1875016, -1875016), bytes(70000, 3)))));
      for (ChunkPos cp : new ChunkPos[]{new ChunkPos(0, 0), new ChunkPos(-1, 1), new ChunkPos(Integer.MIN_VALUE, Integer.MAX_VALUE)})
         PlayHarness.cb("forget_level_chunk/" + cp.x() + "_" + cp.z(), new ClientboundForgetLevelChunkPacket(cp));

      // game events: every constant, a few params
      ClientboundGameEventPacket.Type[] types = {
         ClientboundGameEventPacket.NO_RESPAWN_BLOCK_AVAILABLE, ClientboundGameEventPacket.START_RAINING, ClientboundGameEventPacket.STOP_RAINING,
         ClientboundGameEventPacket.CHANGE_GAME_MODE, ClientboundGameEventPacket.WIN_GAME, ClientboundGameEventPacket.DEMO_EVENT,
         ClientboundGameEventPacket.PLAY_ARROW_HIT_SOUND, ClientboundGameEventPacket.RAIN_LEVEL_CHANGE, ClientboundGameEventPacket.THUNDER_LEVEL_CHANGE,
         ClientboundGameEventPacket.PUFFER_FISH_STING, ClientboundGameEventPacket.GUARDIAN_ELDER_EFFECT, ClientboundGameEventPacket.IMMEDIATE_RESPAWN,
         ClientboundGameEventPacket.LIMITED_CRAFTING, ClientboundGameEventPacket.LEVEL_CHUNKS_LOAD_START};
      float[] params = {0f, 1f, 0.5f, -3.25f, 101f, 104f, Float.MAX_VALUE, Float.NaN};
      for (int i = 0; i < types.length; i++)
         PlayHarness.cb("game_event/" + i, new ClientboundGameEventPacket(types[i], params[i % params.length]));
      PlayHarness.cb("game_event/demo_hint_4", new ClientboundGameEventPacket(ClientboundGameEventPacket.DEMO_EVENT, ClientboundGameEventPacket.DEMO_PARAM_HINT_4));

      // level event
      int[][] ev = {{1000, 0}, {1010, 2147483647}, {-1, -2147483648}, {3000, 5}};
      for (int i = 0; i < ev.length; i++)
         PlayHarness.cb("level_event/" + i, new ClientboundLevelEventPacket(ev[i][0], POSITIONS[i], ev[i][1], i % 2 == 0));

      // ---- world border (private-field packets: set fields directly)
      PlayHarness.cb("initialize_border/default", make(ClientboundInitializeBorderPacket.class,
         "newCenterX", 0.0, "newCenterZ", 0.0, "oldSize", 5.9999968E7, "newSize", 5.9999968E7, "lerpTime", 0L,
         "newAbsoluteMaxSize", 29999984, "warningBlocks", 5, "warningTime", 15));
      PlayHarness.cb("initialize_border/lerping", make(ClientboundInitializeBorderPacket.class,
         "newCenterX", -123.5, "newCenterZ", 1.0e9, "oldSize", 100.0, "newSize", 3.0, "lerpTime", 1234567890123L,
         "newAbsoluteMaxSize", 1, "warningBlocks", 0, "warningTime", 2147483647));
      PlayHarness.cb("initialize_border/extreme", make(ClientboundInitializeBorderPacket.class,
         "newCenterX", Double.NaN, "newCenterZ", -0.0, "oldSize", Double.POSITIVE_INFINITY, "newSize", Double.MIN_VALUE, "lerpTime", Long.MAX_VALUE,
         "newAbsoluteMaxSize", 2147483647, "warningBlocks", -1, "warningTime", -2147483648));
      PlayHarness.cb("set_border_center/0", make(ClientboundSetBorderCenterPacket.class, "newCenterX", 0.0, "newCenterZ", 0.0));
      PlayHarness.cb("set_border_center/1", make(ClientboundSetBorderCenterPacket.class, "newCenterX", -29999984.25, "newCenterZ", 29999984.75));
      PlayHarness.cb("set_border_lerp_size/0", make(ClientboundSetBorderLerpSizePacket.class, "oldSize", 1.0, "newSize", 59999968.0, "lerpTime", 0L));
      PlayHarness.cb("set_border_lerp_size/1", make(ClientboundSetBorderLerpSizePacket.class, "oldSize", 59999968.0, "newSize", 0.125, "lerpTime", 9007199254740993L));
      for (double d : new double[]{0.0, 1.0, 5.9999968E7, -1.5})
         PlayHarness.cb("set_border_size/" + d, make(ClientboundSetBorderSizePacket.class, "size", d));
      for (int v : new int[]{0, 15, 300, 2147483647, -1}) {
         PlayHarness.cb("set_border_warning_delay/" + v, make(ClientboundSetBorderWarningDelayPacket.class, "warningDelay", v));
         PlayHarness.cb("set_border_warning_distance/" + v, make(ClientboundSetBorderWarningDistancePacket.class, "warningBlocks", v));
      }

      // ---- chunk data
      var bei = Class.forName("net.minecraft.network.protocol.game.ClientboundLevelChunkPacketData$BlockEntityInfo");
      Constructor<?> beiK = bei.getDeclaredConstructors()[0];
      beiK.setAccessible(true);
      Constructor<ClientboundLevelChunkPacketData> cdK =
         ClientboundLevelChunkPacketData.class.getDeclaredConstructor(Map.class, byte[].class, List.class);
      cdK.setAccessible(true);
      EnumMap<Heightmap.Types, long[]> allMaps = new EnumMap<>(Heightmap.Types.class);
      int ht = 0;
      for (Heightmap.Types t : Heightmap.Types.values()) allMaps.put(t, longs(37 + ht * 3, 100 + ht++));
      EnumMap<Heightmap.Types, long[]> clientMaps = new EnumMap<>(Heightmap.Types.class);
      clientMaps.put(Heightmap.Types.WORLD_SURFACE, longs(37, 7));
      clientMaps.put(Heightmap.Types.MOTION_BLOCKING, new long[0]);
      List<Object> bes = List.of(
         beiK.newInstance((byte) 0x00, (short) 0, BuiltInRegistries.BLOCK_ENTITY_TYPE.byIdOrThrow(0), Optional.empty()),
         beiK.newInstance((byte) 0xf5, (short) -64, BuiltInRegistries.BLOCK_ENTITY_TYPE.byIdOrThrow(3), Optional.of(sign)),
         beiK.newInstance((byte) 0x7f, (short) 319, BuiltInRegistries.BLOCK_ENTITY_TYPE.byIdOrThrow(BuiltInRegistries.BLOCK_ENTITY_TYPE.size() - 1), Optional.of(new CompoundTag())),
         beiK.newInstance((byte) 0x81, (short) 32767, BuiltInRegistries.BLOCK_ENTITY_TYPE.byIdOrThrow(9), Optional.of(sign)));

      var emptyLight = new ClientboundLightUpdatePacketData(new BitSet(), new BitSet(), new BitSet(), new BitSet(), List.of(), List.of());
      var light = new ClientboundLightUpdatePacketData(
         BitSet.valueOf(new long[]{0b1011}), BitSet.valueOf(new long[]{0xff00ff00ff00ff00L, 3L}),
         BitSet.valueOf(new long[]{1L << 63}), BitSet.valueOf(new long[]{0x10L}),
         List.of(bytes(2048, 11), bytes(2048, 12), bytes(2048, 13)), List.of(bytes(2048, 14), bytes(2048, 15)));
      var shortLayers = new ClientboundLightUpdatePacketData(
         BitSet.valueOf(new long[]{0b1}), BitSet.valueOf(new long[]{0b10}), new BitSet(), BitSet.valueOf(new long[]{1L << 20}),
         List.of(new byte[0]), List.of(bytes(5, 1), bytes(2048, 2)));
      var manyLight = new ClientboundLightUpdatePacketData(
         BitSet.valueOf(new long[]{-1L, -1L}), BitSet.valueOf(new long[]{-1L}), BitSet.valueOf(new long[]{0b110}), BitSet.valueOf(new long[]{0b1}),
         Collections.nCopies(20, bytes(2048, 21)), Collections.nCopies(24, bytes(2048, 22)));

      PlayHarness.cb("level_chunk_with_light/empty", new ClientboundLevelChunkWithLightPacket(0, 0,
         cdK.newInstance(new EnumMap<Heightmap.Types, long[]>(Heightmap.Types.class), new byte[0], List.of()), emptyLight));
      PlayHarness.cb("level_chunk_with_light/client_maps", new ClientboundLevelChunkWithLightPacket(-3, 7,
         cdK.newInstance(clientMaps, bytes(1500, 5), List.of()), light));
      PlayHarness.cb("level_chunk_with_light/all_maps_block_entities", new ClientboundLevelChunkWithLightPacket(Integer.MIN_VALUE, Integer.MAX_VALUE,
         cdK.newInstance(allMaps, bytes(100000, 6), bes), shortLayers));
      PlayHarness.cb("level_chunk_with_light/big_light", new ClientboundLevelChunkWithLightPacket(1875000, -1875000,
         cdK.newInstance(clientMaps, bytes(70, 8), bes.subList(0, 2)), manyLight));
      PlayHarness.cb("light_update/empty", new ClientboundLightUpdatePacket(0, 0, emptyLight));
      PlayHarness.cb("light_update/light", new ClientboundLightUpdatePacket(-1, 300, light));
      PlayHarness.cb("light_update/short_layers", new ClientboundLightUpdatePacket(Integer.MAX_VALUE, Integer.MIN_VALUE, shortLayers));
      PlayHarness.cb("light_update/many", new ClientboundLightUpdatePacket(2000000, -2000000, manyLight));

      for (int[] c : new int[][]{{0, 0}, {-1, 1}, {1875000, -1875000}, {Integer.MIN_VALUE, Integer.MAX_VALUE}})
         PlayHarness.cb("set_chunk_cache_center/" + c[0] + "_" + c[1], new ClientboundSetChunkCacheCenterPacket(c[0], c[1]));
      for (int v : new int[]{0, 2, 32, 127, 128, 2147483647, -1}) {
         PlayHarness.cb("set_chunk_cache_radius/" + v, new ClientboundSetChunkCacheRadiusPacket(v));
         PlayHarness.cb("set_simulation_distance/" + v, new ClientboundSetSimulationDistancePacket(v));
      }

      // ---- login / respawn
      var overworld = Optional.of(GlobalPos.of(Level.OVERWORLD, new BlockPos(100, 64, -100)));
      var nether = Optional.of(GlobalPos.of(dim("minecraft:the_nether"), new BlockPos(-33554432, -2048, 33554431)));
      CommonPlayerSpawnInfo[] infos = {
         spawn(0, Level.OVERWORLD, 0L, GameType.SURVIVAL, Optional.empty(), false, false, Optional.empty(), 0, 63),
         spawn(1, Level.NETHER, -1L, GameType.CREATIVE, Optional.of(GameType.SURVIVAL), true, false, overworld, 10, 32),
         spawn(2, Level.END, Long.MAX_VALUE, GameType.ADVENTURE, Optional.of(GameType.CREATIVE), false, true, nether, 300, -64),
         spawn(PlayHarness.access.lookupOrThrow(Registries.DIMENSION_TYPE).size() - 1, dim("custom:my_dim"), Long.MIN_VALUE, GameType.SPECTATOR,
            Optional.of(GameType.SPECTATOR), true, true, Optional.of(GlobalPos.of(dim("a:b/c"), BlockPos.ZERO)), 2147483647, 2147483647),
         spawn(0, Level.OVERWORLD, 12345678901234L, GameType.SURVIVAL, Optional.of(GameType.ADVENTURE), false, false, Optional.empty(), 1, 0)};
      System.out.println("dimension types " + PlayHarness.access.lookupOrThrow(Registries.DIMENSION_TYPE).size());
      for (int i = 0; i < infos.length; i++) {
         PlayHarness.cb("respawn/" + i, new ClientboundRespawnPacket(infos[i], (byte) (i % 4)));
      }
      PlayHarness.cb("respawn/keep_all", new ClientboundRespawnPacket(infos[0], ClientboundRespawnPacket.KEEP_ALL_DATA));
      PlayHarness.cb("respawn/keep_neg", new ClientboundRespawnPacket(infos[1], (byte) -1));
      PlayHarness.cb("login/0", new ClientboundLoginPacket(0, false, new LinkedHashSet<>(), 0, 0, 0, false, false, false, infos[0], false, false));
      PlayHarness.cb("login/1", new ClientboundLoginPacket(1, true,
         new LinkedHashSet<>(List.of(Level.OVERWORLD, Level.NETHER, Level.END)), 20, 10, 8, true, true, true, infos[1], true, true));
      PlayHarness.cb("login/2", new ClientboundLoginPacket(-1, false,
         new LinkedHashSet<>(List.of(dim("custom:my_dim"), dim("a:b/c"))), 2147483647, 32, 127, false, true, false, infos[2], true, false));
      PlayHarness.cb("login/3", new ClientboundLoginPacket(2147483647, true,
         new LinkedHashSet<>(List.of(Level.OVERWORLD)), 100, 128, 1000, true, false, true, infos[3], false, true));
      PlayHarness.cb("login/4", new ClientboundLoginPacket(123456, false,
         new LinkedHashSet<>(List.of(Level.END)), 8, 2, 2, false, false, true, infos[4], true, true));

      // ---- default spawn
      for (int i = 0; i < 4; i++) {
         PlayHarness.cb("set_default_spawn_position/" + i, new ClientboundSetDefaultSpawnPositionPacket(
            new LevelData.RespawnData(GlobalPos.of(i % 2 == 0 ? Level.OVERWORLD : dim("custom:x"), POSITIONS[i]), -180f + i * 90f, 90f - i * 60f)));
      }
      PlayHarness.cb("set_default_spawn_position/extreme", new ClientboundSetDefaultSpawnPositionPacket(
         new LevelData.RespawnData(GlobalPos.of(Level.END, POSITIONS[3]), Float.NaN, Float.MAX_VALUE)));

      // ---- time
      var clocks = PlayHarness.access.lookupOrThrow(Registries.WORLD_CLOCK);
      System.out.println("world clocks " + clocks.size());
      PlayHarness.cb("set_time/empty", new ClientboundSetTimePacket(0L, new LinkedHashMap<>()));
      PlayHarness.cb("set_time/neg_time", new ClientboundSetTimePacket(-1L, new LinkedHashMap<>()));
      for (int i = 0; i < clocks.size(); i++) {
         Map<Holder<WorldClock>, ClockNetworkState> m = new LinkedHashMap<>();
         m.put(clocks.wrapAsHolder(clocks.byIdOrThrow(i)), new ClockNetworkState(1000L * (i + 1), 0.25f * i, 1.0f));
         PlayHarness.cb("set_time/one_" + i, new ClientboundSetTimePacket(123456789012L, m));
      }
      Map<Holder<WorldClock>, ClockNetworkState> all = new LinkedHashMap<>();
      for (int i = 0; i < clocks.size(); i++)
         all.put(clocks.wrapAsHolder(clocks.byIdOrThrow(i)), new ClockNetworkState(i == 0 ? Long.MAX_VALUE : -5L - i, i == 0 ? Float.NaN : -0.0f, i * 1.5f));
      PlayHarness.cb("set_time/all", new ClientboundSetTimePacket(Long.MAX_VALUE, all));
      PlayHarness.cb("set_time/stopped", new ClientboundSetTimePacket(24000L, Map.of(clocks.wrapAsHolder(clocks.byIdOrThrow(0)), new ClockNetworkState(0L, 0f, 0f))));

      // ---- ticking
      for (float f : new float[]{20f, 0.1f, 10000f, 0f})
         PlayHarness.cb("ticking_state/" + f, new ClientboundTickingStatePacket(f, f == 20f || f == 0f));
      PlayHarness.cb("ticking_state/frozen", new ClientboundTickingStatePacket(20f, true));
      for (int v : new int[]{0, 1, 100, 2147483647, -1})
         PlayHarness.cb("ticking_step/" + v, new ClientboundTickingStepPacket(v));

      // ---- sounds
      int sounds = BuiltInRegistries.SOUND_EVENT.size();
      var direct = new Holder[]{
         Holder.direct(SoundEvent.createVariableRangeEvent(Identifier.parse("custom:thing/ping"))),
         Holder.direct(SoundEvent.createFixedRangeEvent(Identifier.parse("minecraft:entity.pig.ambient"), 16.5f)),
         Holder.direct(SoundEvent.createFixedRangeEvent(Identifier.parse("x:x"), -0.0f))};
      int[] sIds = {0, 1, 200, 1000, sounds - 1};
      SoundSource[] srcs = SoundSource.values();
      for (int i = 0; i < sIds.length; i++) {
         SoundSource src = srcs[i % srcs.length];
         PlayHarness.cb("sound/ref_" + sIds[i], new ClientboundSoundPacket(sound(sIds[i]), src, 0.0, 64.0, 0.0, 1.0f, 1.0f, 0L));
      }
      for (SoundSource src : srcs)
         PlayHarness.cb("sound/source_" + src.name(), new ClientboundSoundPacket(sound(5), src, -1234.567, 70.125, 1.0E7, 0.5f, 2.0f, -987654321L));
      for (int i = 0; i < direct.length; i++)
         PlayHarness.cb("sound/direct_" + i, new ClientboundSoundPacket(direct[i], srcs[i], 12.9, -64.9, -0.01, 3.4028235E38f, 0.5f, Long.MAX_VALUE));
      PlayHarness.cb("sound/extreme_pos", new ClientboundSoundPacket(sound(2), SoundSource.UI, 3.0E9, -3.0E9, Double.NaN, Float.NaN, -0.0f, Long.MIN_VALUE));
      for (int i = 0; i < sIds.length; i++)
         PlayHarness.cb("sound_entity/ref_" + sIds[i], make(ClientboundSoundEntityPacket.class, "sound", sound(sIds[i]), "source", srcs[(i * 3) % srcs.length],
            "id", i * 100000 - 1, "volume", 1.0f, "pitch", 0.5f + i, "seed", 42L * i));
      for (int i = 0; i < direct.length; i++)
         PlayHarness.cb("sound_entity/direct_" + i, make(ClientboundSoundEntityPacket.class, "sound", direct[i], "source", srcs[10 - i],
            "id", 2147483647, "volume", Float.MAX_VALUE, "pitch", Float.MIN_VALUE, "seed", -1L));

      Identifier named = Identifier.parse("minecraft:entity.pig.ambient");
      PlayHarness.cb("stop_sound/none", new ClientboundStopSoundPacket(null, null));
      PlayHarness.cb("stop_sound/name", new ClientboundStopSoundPacket(named, null));
      PlayHarness.cb("stop_sound/name_custom", new ClientboundStopSoundPacket(Identifier.parse("my_pack:a/b.c-d"), null));
      for (SoundSource src : srcs) {
         PlayHarness.cb("stop_sound/source_" + src.name(), new ClientboundStopSoundPacket(null, src));
         PlayHarness.cb("stop_sound/both_" + src.name(), new ClientboundStopSoundPacket(named, src));
      }

      PlayHarness.close();
   }
}
