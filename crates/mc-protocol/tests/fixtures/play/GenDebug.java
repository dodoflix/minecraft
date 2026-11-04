// Golden generator for the `debug` play packet group (debug subscriptions, samples, game rules, game tests).
// Run: crates/mc-protocol/tests/fixtures/play/gen.sh GenDebug.java   (writes debug.golden)
import java.lang.reflect.Field;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Vec3i;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.Unit;
import net.minecraft.util.debug.*;
import net.minecraft.util.debugchart.RemoteDebugSampleType;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.entity.TestInstanceBlockEntity;
import net.minecraft.world.level.block.state.properties.TestBlockMode;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.pathfinder.Node;
import net.minecraft.world.level.pathfinder.Path;
import net.minecraft.world.level.pathfinder.PathType;
import net.minecraft.world.level.pathfinder.Target;
import net.minecraft.world.level.redstone.Orientation;
import net.minecraft.world.phys.Vec3;

public class GenDebug {
   static BlockPos bp(int x, int y, int z) { return new BlockPos(x, y, z); }

   static <T> void all(String name, DebugSubscription<T> sub, T value, int salt) {
      PlayHarness.cb("block_" + name, new ClientboundDebugBlockValuePacket(bp(salt, salt * 2 - 3, -salt), sub.packUpdate(value)));
      PlayHarness.cb("chunk_" + name, new ClientboundDebugChunkValuePacket(new ChunkPos(salt, -salt), sub.packUpdate(value)));
      PlayHarness.cb("entity_" + name, new ClientboundDebugEntityValuePacket(salt * 1000, sub.packUpdate(value)));
      PlayHarness.cb("event_" + name, new ClientboundDebugEventPacket(sub.packEvent(value)));
   }

   static Node node(int x, int y, int z, float walked, float malus, boolean closed, PathType t, float f) {
      Node n = new Node(x, y, z);
      fill(n, walked, malus, closed, t, f);
      return n;
   }

   static Target target(int x, int y, int z, float walked, float malus, boolean closed, PathType t, float f) {
      Target n = new Target(x, y, z);
      fill(n, walked, malus, closed, t, f);
      return n;
   }

   static void fill(Node n, float walked, float malus, boolean closed, PathType t, float f) {
      n.walkedDistance = walked;
      n.costMalus = malus;
      n.closed = closed;
      n.type = t;
      n.f = f;
   }

   static Path path(boolean reached, int next, BlockPos target, List<Node> nodes, List<Node> open, List<Node> closed, Set<Target> targets) throws Exception {
      Path p = new Path(nodes, target, reached);
      Field nn = Path.class.getDeclaredField("nextNodeIndex");
      nn.setAccessible(true);
      nn.setInt(p, next);
      Field dd = Path.class.getDeclaredField("debugData");
      dd.setAccessible(true);
      dd.set(p, new Path.DebugData(open, closed, targets));
      return p;
   }

   @SafeVarargs
   static <T> Set<T> set(T... v) { return new LinkedHashSet<>(Arrays.asList(v)); }

   static Component styled() { return Component.translatable("test.status", "a\uD83D\uDE00", 3).withStyle(net.minecraft.ChatFormatting.RED, net.minecraft.ChatFormatting.BOLD); }

   public static void main(String[] args) throws Exception {
      PlayHarness.init(args[0]);

      // ---- subscription values ----
      all("bees_empty", DebugSubscriptions.BEES, new DebugBeeInfo(Optional.empty(), Optional.empty(), 0, List.of()), 1);
      all("bees_full", DebugSubscriptions.BEES, new DebugBeeInfo(Optional.of(bp(10, 64, -10)), Optional.of(bp(-3, 70, 99)), 12345, List.of(bp(1, 2, 3), bp(-1, -2, -3), bp(33554431, 2047, -33554432))), 2);
      all("bees_hive", DebugSubscriptions.BEES, new DebugBeeInfo(Optional.of(bp(0, 0, 0)), Optional.empty(), -1, List.of(bp(5, 5, 5))), 3);
      all("bees_flower", DebugSubscriptions.BEES, new DebugBeeInfo(Optional.empty(), Optional.of(bp(7, -64, 7)), Integer.MAX_VALUE, List.of()), 4);

      all("brains_min", DebugSubscriptions.BRAINS, new DebugBrainDump("", "", 0, 0.0F, 0.0F, "", false, 0, List.of(), List.of(), List.of(), List.of(), set(), set()), 1);
      all("brains_full", DebugSubscriptions.BRAINS, new DebugBrainDump("Bob \uD83D\uDE00", "minecraft:farmer", -5, 20.0F, 0.5F, "1 bread, 2 wheat", true, -1,
         List.of("core", "idle", "work"), List.of("run_one", "\uD83D\uDE00"), List.of("home: [1, 2, 3]", "job_site: -"), List.of("Alice: major_positive: 20", "Carl: trading: 1"),
         set(bp(1, 2, 3), bp(-4, 5, -6), bp(7, 8, 9)), set(bp(100, 64, -100))), 2);
      all("brains_warden", DebugSubscriptions.BRAINS, new DebugBrainDump("Warden", "", 1000000, 500.0F, 500.0F, "", false, 4, List.of("fight"), List.of(), List.of(), List.of(), set(), set(bp(0, 0, 0), bp(1, 1, 1))), 3);
      all("brains_extremes", DebugSubscriptions.BRAINS, new DebugBrainDump("x".repeat(32767), "p", Integer.MIN_VALUE, Float.MIN_VALUE, Float.MAX_VALUE, "i".repeat(300), true, Integer.MAX_VALUE, List.of(""), List.of(""), List.of(""), List.of(""), set(), set()), 4);

      all("breezes_empty", DebugSubscriptions.BREEZES, new DebugBreezeInfo(Optional.empty(), Optional.empty()), 1);
      all("breezes_full", DebugSubscriptions.BREEZES, new DebugBreezeInfo(Optional.of(300), Optional.of(bp(-1, -1, -1))), 2);
      all("breezes_target", DebugSubscriptions.BREEZES, new DebugBreezeInfo(Optional.of(0), Optional.empty()), 3);
      all("breezes_neg", DebugSubscriptions.BREEZES, new DebugBreezeInfo(Optional.of(-1), Optional.of(bp(9, 9, 9))), 4);
      all("breezes_max", DebugSubscriptions.BREEZES, new DebugBreezeInfo(Optional.of(Integer.MAX_VALUE), Optional.empty()), 5);

      all("goals_empty", DebugSubscriptions.GOAL_SELECTORS, new DebugGoalInfo(List.of()), 1);
      all("goals_some", DebugSubscriptions.GOAL_SELECTORS, new DebugGoalInfo(List.of(
         new DebugGoalInfo.DebugGoal(0, true, "FloatGoal"), new DebugGoalInfo.DebugGoal(-1, false, ""), new DebugGoalInfo.DebugGoal(1000, true, "\uD83D\uDE00goal"),
         new DebugGoalInfo.DebugGoal(Integer.MAX_VALUE, false, "x".repeat(255)))), 2);

      List<Node> allTypes = new ArrayList<>();
      int i = 0;
      for (PathType t : PathType.values()) {
         allTypes.add(node(i, -i, i * 100, i * 0.5F, t.getMalus(), i % 2 == 0, t, i * 1.25F));
         i++;
      }
      all("paths_empty", DebugSubscriptions.ENTITY_PATHS, new DebugPathInfo(path(false, 0, bp(0, 0, 0), List.of(), List.of(), List.of(), set(target(0, 0, 0, 0, 0, false, PathType.BLOCKED, 0))), 0.0F), 1);
      all("paths_types", DebugSubscriptions.ENTITY_PATHS, new DebugPathInfo(path(true, 5, bp(1, 2, 3), allTypes, allTypes.subList(0, 3), allTypes.subList(3, 9),
         set(target(1, 2, 3, 1.5F, -1.0F, true, PathType.WALKABLE, 7.0F), target(-1, -2, -3, 0.0F, 8.0F, false, PathType.WATER, Float.MAX_VALUE))), 16.0F), 2);
      all("paths_neg", DebugSubscriptions.ENTITY_PATHS, new DebugPathInfo(path(false, -1, bp(-30000000, -64, 30000000), List.of(node(Integer.MIN_VALUE, Integer.MAX_VALUE, -1, Float.MIN_VALUE, -0.0F, false, PathType.BIG_MOBS_CLOSE_TO_DANGER, Float.MAX_VALUE)),
         List.of(), List.of(node(1, 1, 1, 0, 0, true, PathType.OPEN, 0)), set(target(5, 5, 5, 2.0F, 2.0F, false, PathType.FENCE, 2.0F))), Float.MAX_VALUE), 3);

      for (DebugEntityBlockIntersection x : DebugEntityBlockIntersection.values()) {
         all("intersection_" + x.name().toLowerCase(), DebugSubscriptions.ENTITY_BLOCK_INTERSECTIONS, x, 1 + x.ordinal());
      }

      all("hive_beehive", DebugSubscriptions.BEE_HIVES, new DebugHiveInfo(Blocks.BEEHIVE, 3, 5, true), 1);
      all("hive_nest", DebugSubscriptions.BEE_HIVES, new DebugHiveInfo(Blocks.BEE_NEST, 0, 0, false), 2);
      all("hive_other", DebugSubscriptions.BEE_HIVES, new DebugHiveInfo(Blocks.STONE, Integer.MAX_VALUE, -1, true), 3);
      Block lastBlock = BuiltInRegistries.BLOCK.byId(BuiltInRegistries.BLOCK.size() - 1);
      all("hive_lastblock", DebugSubscriptions.BEE_HIVES, new DebugHiveInfo(lastBlock, 1, 1, false), 4);

      all("poi_first", DebugSubscriptions.POIS, new DebugPoiInfo(bp(1, 2, 3), BuiltInRegistries.POINT_OF_INTEREST_TYPE.get(0).orElseThrow(), 0), 1);
      all("poi_last", DebugSubscriptions.POIS, new DebugPoiInfo(bp(-1, -2, -3), BuiltInRegistries.POINT_OF_INTEREST_TYPE.get(BuiltInRegistries.POINT_OF_INTEREST_TYPE.size() - 1).orElseThrow(), 1), 2);
      all("poi_tickets", DebugSubscriptions.POIS, new DebugPoiInfo(bp(33554431, 2047, -33554432), BuiltInRegistries.POINT_OF_INTEREST_TYPE.get(5).orElseThrow(), Integer.MAX_VALUE), 3);

      int n = 0;
      for (Direction up : Direction.values()) {
         for (Direction front : Direction.values()) {
            if (up.getAxis() == front.getAxis()) continue;
            for (Orientation.SideBias bias : Orientation.SideBias.values()) {
               Orientation o = Orientation.of(up, front, bias);
               all("orientation_" + o.getIndex(), DebugSubscriptions.REDSTONE_WIRE_ORIENTATIONS, o, 1 + n % 7);
               n++;
            }
         }
      }

      all("village", DebugSubscriptions.VILLAGE_SECTIONS, Unit.INSTANCE, 1);
      all("raids_empty", DebugSubscriptions.RAIDS, List.<BlockPos>of(), 1);
      all("raids_one", DebugSubscriptions.RAIDS, List.of(bp(1, 2, 3)), 2);
      all("raids_many", DebugSubscriptions.RAIDS, List.of(bp(1, 2, 3), bp(-100, 64, 100), bp(0, 0, 0), bp(33554431, 2047, -33554432), bp(-33554432, -2048, 33554431)), 3);

      all("structures_empty", DebugSubscriptions.STRUCTURES, List.<DebugStructureInfo>of(), 1);
      all("structures_one", DebugSubscriptions.STRUCTURES, List.of(new DebugStructureInfo(new BoundingBox(0, 0, 0, 0, 0, 0), List.of())), 2);
      all("structures_pieces", DebugSubscriptions.STRUCTURES, List.of(
         new DebugStructureInfo(new BoundingBox(-10, -64, -10, 10, 100, 10), List.of(
            new DebugStructureInfo.Piece(new BoundingBox(-10, -64, -10, 0, 0, 0), true), new DebugStructureInfo.Piece(new BoundingBox(1, 1, 1, 10, 100, 10), false))),
         new DebugStructureInfo(new BoundingBox(100, 0, 100, 200, 50, 300), List.of(new DebugStructureInfo.Piece(new BoundingBox(100, 0, 100, 200, 50, 300), true)))), 3);

      all("listener_0", DebugSubscriptions.GAME_EVENT_LISTENERS, new DebugGameEventListenerInfo(0), 1);
      all("listener_15", DebugSubscriptions.GAME_EVENT_LISTENERS, new DebugGameEventListenerInfo(15), 2);
      all("listener_max", DebugSubscriptions.GAME_EVENT_LISTENERS, new DebugGameEventListenerInfo(Integer.MAX_VALUE), 3);
      all("listener_neg", DebugSubscriptions.GAME_EVENT_LISTENERS, new DebugGameEventListenerInfo(-1), 4);

      all("neighbor_origin", DebugSubscriptions.NEIGHBOR_UPDATES, bp(0, 0, 0), 1);
      all("neighbor_neg", DebugSubscriptions.NEIGHBOR_UPDATES, bp(-1, -1, -1), 2);
      all("neighbor_extreme", DebugSubscriptions.NEIGHBOR_UPDATES, bp(33554431, 2047, -33554432), 3);
      all("neighbor_extreme2", DebugSubscriptions.NEIGHBOR_UPDATES, bp(-33554432, -2048, 33554431), 4);

      int events = BuiltInRegistries.GAME_EVENT.size();
      for (int id : new int[] {0, 1, events / 2, events - 1}) {
         all("gameevent_" + id, DebugSubscriptions.GAME_EVENTS, new DebugGameEventInfo(BuiltInRegistries.GAME_EVENT.get(id).orElseThrow(), new Vec3(1.5 + id, -64.25, 1e10)), 1 + id % 5);
      }
      all("gameevent_extreme", DebugSubscriptions.GAME_EVENTS, new DebugGameEventInfo(BuiltInRegistries.GAME_EVENT.get(2).orElseThrow(), new Vec3(-0.0, Double.MIN_VALUE, Double.MAX_VALUE)), 6);

      // empty updates for every subscription with a value codec
      for (DebugSubscription<?> sub : BuiltInRegistries.DEBUG_SUBSCRIPTION) {
         if (sub == DebugSubscriptions.DEDICATED_SERVER_TICK_TIME) continue;
         String name = sub.toString().replace("minecraft:", "");
         PlayHarness.cb("block_empty_" + name, new ClientboundDebugBlockValuePacket(bp(-5, 5, -5), sub.emptyUpdate()));
         PlayHarness.cb("chunk_empty_" + name, new ClientboundDebugChunkValuePacket(new ChunkPos(-7, 7), sub.emptyUpdate()));
         PlayHarness.cb("entity_empty_" + name, new ClientboundDebugEntityValuePacket(-1, sub.emptyUpdate()));
      }
      // boundary position/entity values
      PlayHarness.cb("entity_max", new ClientboundDebugEntityValuePacket(Integer.MAX_VALUE, DebugSubscriptions.NEIGHBOR_UPDATES.packUpdate(bp(1, 1, 1))));
      PlayHarness.cb("chunk_extreme", new ClientboundDebugChunkValuePacket(new ChunkPos(Integer.MIN_VALUE, Integer.MAX_VALUE), DebugSubscriptions.NEIGHBOR_UPDATES.packUpdate(bp(1, 1, 1))));

      // ---- debug_sample ----
      PlayHarness.cb("sample_empty", new ClientboundDebugSamplePacket(new long[0], RemoteDebugSampleType.TICK_TIME));
      PlayHarness.cb("sample_some", new ClientboundDebugSamplePacket(new long[] {0L, -1L, Long.MAX_VALUE, Long.MIN_VALUE, 50_000_000L}, RemoteDebugSampleType.TICK_TIME));
      long[] big = new long[200];
      for (int k = 0; k < big.length; k++) big[k] = k * 123456789L - 7;
      PlayHarness.cb("sample_big", new ClientboundDebugSamplePacket(big, RemoteDebugSampleType.TICK_TIME));

      // ---- game rules ----
      PlayHarness.cb("rules_empty", new ClientboundGameRuleValuesPacket(new LinkedHashMap<>()));
      Map<ResourceKey<net.minecraft.world.level.gamerules.GameRule<?>>, String> rules = new LinkedHashMap<>();
      PlayHarness.cb("rules_one", new ClientboundGameRuleValuesPacket(new LinkedHashMap<>(Map.of(rk("advance_time"), "true"))));
      rules.put(rk("advance_time"), "false");
      rules.put(rk("random_tick_speed"), "3");
      rules.put(rk("max_command_sequence_length"), "-2147483648");
      rules.put(rk("fire_damage"), "\uD83D\uDE00");
      rules.put(ResourceKey.create(Registries.GAME_RULE, Identifier.fromNamespaceAndPath("mod", "custom/rule")), "");
      rules.put(rk("block_drops"), "x".repeat(32767));
      PlayHarness.cb("rules_many", new ClientboundGameRuleValuesPacket(rules));
      Map<ResourceKey<net.minecraft.world.level.gamerules.GameRule<?>>, String> allRules = new LinkedHashMap<>();
      for (int id = 0; id < BuiltInRegistries.GAME_RULE.size(); id++) allRules.put(ResourceKey.create(Registries.GAME_RULE, BuiltInRegistries.GAME_RULE.getKey(BuiltInRegistries.GAME_RULE.byId(id))), "v" + id);
      PlayHarness.cb("rules_all", new ClientboundGameRuleValuesPacket(allRules));

      // ---- game test ----
      PlayHarness.cb("highlight_origin", new ClientboundGameTestHighlightPosPacket(bp(0, 0, 0), bp(0, 0, 0)));
      PlayHarness.cb("highlight_some", new ClientboundGameTestHighlightPosPacket(bp(100, 64, -100), bp(-1, -2, -3)));
      PlayHarness.cb("highlight_extreme", new ClientboundGameTestHighlightPosPacket(bp(33554431, 2047, -33554432), bp(-33554432, -2048, 33554431)));
      PlayHarness.cb("status_plain", new ClientboundTestInstanceBlockStatus(Component.literal("Running"), Optional.empty()));
      PlayHarness.cb("status_size", new ClientboundTestInstanceBlockStatus(Component.literal("\uD83D\uDE00 done"), Optional.of(new Vec3i(3, 4, 5))));
      PlayHarness.cb("status_styled", new ClientboundTestInstanceBlockStatus(styled(), Optional.of(new Vec3i(0, 0, 0))));
      PlayHarness.cb("status_neg", new ClientboundTestInstanceBlockStatus(Component.empty(), Optional.of(new Vec3i(-1, 300, Integer.MAX_VALUE))));

      // ---- serverbound ----
      Set<DebugSubscription<?>> all = new LinkedHashSet<>();
      PlayHarness.sb("subscribe_none", new ServerboundDebugSubscriptionRequestPacket(new LinkedHashSet<>(all)));
      all.add(DebugSubscriptions.BEES);
      PlayHarness.sb("subscribe_one", new ServerboundDebugSubscriptionRequestPacket(new LinkedHashSet<>(all)));
      all.add(DebugSubscriptions.DEDICATED_SERVER_TICK_TIME);
      all.add(DebugSubscriptions.GAME_EVENTS);
      PlayHarness.sb("subscribe_three", new ServerboundDebugSubscriptionRequestPacket(new LinkedHashSet<>(all)));
      Set<DebugSubscription<?>> every = new LinkedHashSet<>();
      for (DebugSubscription<?> s : BuiltInRegistries.DEBUG_SUBSCRIPTION) every.add(s);
      PlayHarness.sb("subscribe_all", new ServerboundDebugSubscriptionRequestPacket(every));

      PlayHarness.sb("setrule_empty", new ServerboundSetGameRulePacket(List.of()));
      PlayHarness.sb("setrule_one", new ServerboundSetGameRulePacket(List.of(new ServerboundSetGameRulePacket.Entry(rk("advance_time"), "false"))));
      PlayHarness.sb("setrule_many", new ServerboundSetGameRulePacket(List.of(
         new ServerboundSetGameRulePacket.Entry(rk("random_tick_speed"), "-7"), new ServerboundSetGameRulePacket.Entry(rk("fire_damage"), ""),
         new ServerboundSetGameRulePacket.Entry(ResourceKey.create(Registries.GAME_RULE, Identifier.fromNamespaceAndPath("mod", "x/y")), "\uD83D\uDE00"),
         new ServerboundSetGameRulePacket.Entry(rk("block_drops"), "y".repeat(32767)))));

      for (TestBlockMode mode : TestBlockMode.values()) {
         PlayHarness.sb("testblock_" + mode.name().toLowerCase() + "_empty", new ServerboundSetTestBlockPacket(bp(1, 2, 3), mode, ""));
         PlayHarness.sb("testblock_" + mode.name().toLowerCase() + "_msg", new ServerboundSetTestBlockPacket(bp(-1, -2, -3), mode, "hello \uD83D\uDE00 \u00e9"));
      }
      PlayHarness.sb("testblock_long", new ServerboundSetTestBlockPacket(bp(33554431, 2047, -33554432), TestBlockMode.ACCEPT, "z".repeat(32767)));

      for (ServerboundTestInstanceBlockActionPacket.Action a : ServerboundTestInstanceBlockActionPacket.Action.values()) {
         PlayHarness.sb("testaction_" + a.name().toLowerCase(), new ServerboundTestInstanceBlockActionPacket(bp(4, 5, 6), a,
            new TestInstanceBlockEntity.Data(Optional.empty(), new Vec3i(1, 1, 1), Rotation.NONE, false, TestInstanceBlockEntity.Status.CLEARED, Optional.empty())));
      }
      int r = 0;
      for (Rotation rot : Rotation.values()) {
         for (TestInstanceBlockEntity.Status st : TestInstanceBlockEntity.Status.values()) {
            boolean ignore = (r & 1) == 0;
            PlayHarness.sb("testdata_" + rot.name().toLowerCase() + "_" + st.name().toLowerCase(), new ServerboundTestInstanceBlockActionPacket(bp(-r, r, r), ServerboundTestInstanceBlockActionPacket.Action.SET,
               new TestInstanceBlockEntity.Data(Optional.of(ResourceKey.create(Registries.TEST_INSTANCE, Identifier.fromNamespaceAndPath("mod", "tests/t" + r))), new Vec3i(r, -r, 300 * r), rot, ignore, st,
                  (r % 3 == 0) ? Optional.of(Component.literal("failed \uD83D\uDE00")) : (r % 3 == 1) ? Optional.of(styled()) : Optional.empty())));
            r++;
         }
      }
      PlayHarness.sb("testdata_extreme", new ServerboundTestInstanceBlockActionPacket(bp(33554431, 2047, -33554432), ServerboundTestInstanceBlockActionPacket.Action.RUN,
         new TestInstanceBlockEntity.Data(Optional.of(ResourceKey.create(Registries.TEST_INSTANCE, Identifier.withDefaultNamespace("empty"))), new Vec3i(Integer.MIN_VALUE, Integer.MAX_VALUE, 0), Rotation.COUNTERCLOCKWISE_90, true,
            TestInstanceBlockEntity.Status.FINISHED, Optional.of(Component.empty()))));

      PlayHarness.close();
   }

   static ResourceKey<net.minecraft.world.level.gamerules.GameRule<?>> rk(String name) {
      return ResourceKey.create(Registries.GAME_RULE, Identifier.withDefaultNamespace(name));
   }
}
