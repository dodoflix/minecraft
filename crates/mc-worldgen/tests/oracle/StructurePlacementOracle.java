import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import com.mojang.serialization.Lifecycle;
import java.io.FileOutputStream;
import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.Set;
import java.util.stream.Collectors;
import java.util.stream.Stream;
import java.util.zip.GZIPOutputStream;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.MappedRegistry;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.Vec3i;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerChunkCache;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.StructureManager;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterLists;
import net.minecraft.world.level.biome.TheEndBiomeSource;
import net.minecraft.world.level.chunk.ChunkGenerator;
import net.minecraft.world.level.chunk.ChunkGeneratorStructureState;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.WorldGenSettings;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureCheckResult;
import net.minecraft.world.level.levelgen.structure.StructureSet;
import net.minecraft.world.level.levelgen.structure.placement.AbstractSpreadingStructurePlacement;
import net.minecraft.world.level.levelgen.structure.placement.ConcentricRingsStructurePlacement;
import net.minecraft.world.level.levelgen.structure.placement.DimensionOriginStructurePlacement;
import net.minecraft.world.level.levelgen.structure.placement.RandomSpreadStructurePlacement;
import net.minecraft.world.level.levelgen.structure.placement.StructurePlacement;
import net.minecraft.world.level.levelgen.synth.NormalNoise;
import sun.misc.Unsafe;

/**
 * Golden generator for the mc-worldgen structure placement tests (crates/mc-worldgen/src/structure_placement).
 * Usage (from the repo root):
 *   java -cp tools/extractor/build:$(scripts/vanilla-cp.sh) crates/mc-worldgen/tests/oracle/StructurePlacementOracle.java \
 *        crates/mc-worldgen/tests/fixtures/structure_placement_cases.json crates/mc-worldgen/tests/fixtures/structure_placement.txt.gz
 *
 * Lines (hashes: h = h * 1000003 + v over the found values, java long arithmetic):
 *   K name ok|err [summary]        codec case (summary format in the Rust test)
 *   L id summary                   vanilla structure sets in registry order
 *   Q id seed hash x,z x,z x,z     getPotentialStructureChunk over x,z in [-100, 100)^2 (random_spread sets)
 *   P dim seed id...               possibleStructureSets
 *   I dim seed id count hash first isStructureChunk over x,z in [-60, 60)^2 (count/hash of the true chunks, first 5)
 *   R dim seed id x,z ...          getRingPositionsFor (concentric rings sets of the possible sets)
 *   Z dim seed id a b              isStructureChunk count over the ring chunks / their +x neighbours
 *   H dim seed id x z range b      hasStructureChunkInRange
 *   Y dim seed structure id...     getPlacementsForStructure (set ids)
 *   T dim seed id ox oz r x y z|null   getNearestGeneratedStructure (random spread ring walk) for a fixed predicate
 *   C dim seed id x z result       getNearestGeneratedStructure (rings) for a fixed predicate
 *   M dim seed variant x z r result  findNearestMapStructure for a fixed predicate
 *  dims: overworld, nether, end, flat (createForFlat on the overworld source)
 */
public class StructurePlacementOracle {
   static final long[] SEEDS = {0L, 12345L, -4172144997902289642L, Long.MAX_VALUE, Long.MIN_VALUE};
   static PrintWriter out;
   static RegistryAccess.Frozen reg;

   static String id(Holder<?> h) {
      return h.unwrapKey().get().identifier().toString();
   }

   static Object get(Object o, Class<?> c, String name) throws Exception {
      Method m = c.getDeclaredMethod(name);
      m.setAccessible(true);
      return m.invoke(o);
   }

   static String ids(HolderSet<?> set) {
      return set.stream().map(StructurePlacementOracle::id).collect(Collectors.joining(","));
   }

   static String summary(StructurePlacement p) throws Exception {
      if (p instanceof DimensionOriginStructurePlacement) {
         return "origin";
      }
      Class<?> c = AbstractSpreadingStructurePlacement.class;
      Vec3i off = ((AbstractSpreadingStructurePlacement) p).locateOffset();
      Object zone = get(p, c, "exclusionZone");
      String z = "-";
      if (((Optional<?>) zone).isPresent()) {
         var ez = (AbstractSpreadingStructurePlacement.ExclusionZone) ((Optional<?>) zone).get();
         z = id(ez.otherSet()) + "/" + ez.chunkCount();
      }
      String common = " off=" + off.getX() + "," + off.getY() + "," + off.getZ()
         + " m=" + ((AbstractSpreadingStructurePlacement.FrequencyReductionMethod) get(p, c, "frequencyReductionMethod")).getSerializedName()
         + " f=" + Float.toString((Float) get(p, c, "frequency"))
         + " salt=" + get(p, c, "salt") + " ex=" + z;
      if (p instanceof RandomSpreadStructurePlacement r) {
         return "rs" + common + " sp=" + r.spacing() + " se=" + r.separation() + " t=" + r.spreadType().getSerializedName();
      }
      ConcentricRingsStructurePlacement r = (ConcentricRingsStructurePlacement) p;
      return "cr" + common + " d=" + r.distance() + " s=" + r.spread() + " c=" + r.count() + " b=" + ids(r.preferredBiomes());
   }

   static String summary(StructureSet s) throws Exception {
      StringBuilder sb = new StringBuilder("[");
      for (var e : s.structures()) {
         sb.append(id(e.structure())).append('*').append(e.weight()).append(' ');
      }
      return sb.append("] ").append(summary(s.placement())).toString();
   }

   static long mix(long h, long v) {
      return h * 1000003L + v;
   }

   // ---- fake level for the private/instance locate methods ----
   static final Unsafe UNSAFE;
   static {
      try {
         Field f = Unsafe.class.getDeclaredField("theUnsafe");
         f.setAccessible(true);
         UNSAFE = (Unsafe) f.get(null);
      } catch (Exception e) {
         throw new RuntimeException(e);
      }
   }

   static boolean predicate(long seed, int x, int z) {
      return Math.floorMod(x * 31L + z * 17L + seed, 5L) == 0L;
   }

   static class FakeManager extends StructureManager {
      long seed;

      FakeManager() {
         super(null, null, null);
      }

      @Override
      public StructureCheckResult checkStructurePresence(ChunkPos pos, Structure structure, StructurePlacement placement, boolean createReference) {
         return predicate(seed, pos.x(), pos.z()) ? StructureCheckResult.START_PRESENT : StructureCheckResult.START_NOT_PRESENT;
      }
   }

   static String pair(com.mojang.datafixers.util.Pair<BlockPos, Holder<Structure>> p) {
      return p == null ? "null" : p.getFirst().getX() + " " + p.getFirst().getY() + " " + p.getFirst().getZ() + " " + id(p.getSecond());
   }

   public static void main(String[] args) throws Exception {
      reg = extractor.Setup.bootstrap();
      RegistryOps<JsonElement> ops = RegistryOps.create(JsonOps.INSTANCE, reg);
      var cases = JsonParser.parseString(Files.readString(Path.of(args[0]))).getAsJsonObject();
      out = new PrintWriter(new GZIPOutputStream(new FileOutputStream(args[1])));

      // codec cases
      for (JsonElement ce : cases.getAsJsonArray("codec")) {
         var c = ce.getAsJsonObject();
         String name = c.get("name").getAsString();
         String kind = c.get("kind").getAsString();
         try {
            if (kind.equals("placement")) {
               var r = StructurePlacement.CODEC.parse(ops, c.get("json"));
               out.println("K " + name + " " + (r.result().isPresent() ? "ok " + summary(r.result().get()) : "err"));
            } else {
               var r = StructureSet.DIRECT_CODEC.parse(ops, c.get("json"));
               out.println("K " + name + " " + (r.result().isPresent() ? "ok " + summary(r.result().get()) : "err"));
            }
         } catch (Exception e) {
            out.println("K " + name + " exc " + e);
         }
      }

      // registry: vanilla sets + custom sets
      Registry<StructureSet> vanilla = reg.lookupOrThrow(Registries.STRUCTURE_SET);
      MappedRegistry<StructureSet> sets = new MappedRegistry<>(Registries.STRUCTURE_SET, Lifecycle.stable());
      List<Holder.Reference<StructureSet>> vanillaHolders = vanilla.listElements().toList();
      for (var h : vanillaHolders) {
         out.println("L " + id(h) + " " + summary(h.value()));
         Registry.register(sets, h.key(), h.value());
      }
      for (JsonElement ce : cases.getAsJsonArray("registry")) {
         var c = ce.getAsJsonObject();
         var set = StructureSet.DIRECT_CODEC.parse(ops, c.get("json")).getOrThrow();
         Registry.register(sets, ResourceKey.create(Registries.STRUCTURE_SET, Identifier.parse(c.get("id").getAsString())), set);
      }
      sets.freeze();
      List<Holder.Reference<StructureSet>> all = sets.listElements().toList();

      // potential structure chunks
      for (var h : all) {
         if (!(h.value().placement() instanceof RandomSpreadStructurePlacement rs)) {
            continue;
         }
         for (long seed : SEEDS) {
            long hash = 0;
            for (int x = -100; x < 100; x++) {
               for (int z = -100; z < 100; z++) {
                  ChunkPos p = rs.getPotentialStructureChunk(seed, x, z);
                  hash = mix(mix(hash, p.x()), p.z());
               }
            }
            StringBuilder sb = new StringBuilder();
            for (int[] s : new int[][]{{0, 0}, {-1, -1}, {199, -250}, {-100000, 100000}, {2000000, -2000000}}) {
               ChunkPos p = rs.getPotentialStructureChunk(seed, s[0], s[1]);
               sb.append(' ').append(p.x()).append(',').append(p.z());
            }
            out.println("Q " + id(h) + " " + seed + " " + hash + sb);
         }
      }

      // biome sources
      var biomes = reg.lookupOrThrow(Registries.BIOME);
      var lists = reg.lookupOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST);
      var noises = reg.lookupOrThrow(Registries.NOISE);
      var settingsLookup = reg.lookupOrThrow(Registries.NOISE_SETTINGS);
      BiomeSource overworld = MultiNoiseBiomeSource.createFromPreset(lists.getOrThrow(MultiNoiseBiomeSourceParameterLists.OVERWORLD));
      BiomeSource nether = MultiNoiseBiomeSource.createFromPreset(lists.getOrThrow(MultiNoiseBiomeSourceParameterLists.NETHER));
      BiomeSource end = TheEndBiomeSource.create(biomes);
      Object[][] dims = {{"overworld", overworld, "overworld"}, {"nether", nether, "nether"}, {"end", end, "end"}};
      var structures = reg.lookupOrThrow(Registries.STRUCTURE);
      List<String> structureIds = new ArrayList<>();
      for (var h : all) {
         for (var e : h.value().structures()) {
            if (!structureIds.contains(id(e.structure()))) {
               structureIds.add(id(e.structure()));
            }
         }
      }

      for (Object[] dim : dims) {
         String dimName = (String) dim[0];
         BiomeSource source = (BiomeSource) dim[1];
         NoiseGeneratorSettings settings = settingsLookup.getOrThrow(ResourceKey.create(Registries.NOISE_SETTINGS, Identifier.withDefaultNamespace((String) dim[2]))).value();
         for (long seed : SEEDS) {
            RandomState rs = RandomState.create(noises, seed, settings);
            ChunkPos origin = new ChunkPos((int) (seed & 7) - 3, (int) ((seed >> 3) & 7) - 3);
            ChunkGeneratorStructureState state = ChunkGeneratorStructureState.createForNormal(rs, seed, origin, source, sets);
            emitState(dimName, seed, state, all, structureIds, structures);
            emitLocate(dimName, seed, state, all, structures);
            if (dimName.equals("overworld")) {
               List<Holder<StructureSet>> overrides = new ArrayList<>();
               for (var h : all) {
                  String hid = id(h);
                  if (hid.equals("minecraft:strongholds") || hid.equals("minecraft:villages") || hid.equals("test:rings_a")
                     || hid.equals("test:rings_b") || hid.equals("test:freq_default_0_5")) {
                     overrides.add(h);
                  }
               }
               ChunkGeneratorStructureState flat = ChunkGeneratorStructureState.createForFlat(rs, seed, origin, source, overrides.stream());
               emitState("flat", seed, flat, all, structureIds, structures);
            }
         }
      }

      out.close();
   }


   static class FakeChunkCache extends ServerChunkCache {
      ChunkGeneratorStructureState state;

      FakeChunkCache() {
         super(null, null, null, null, null, null, 0, 0, false, null);
      }

      @Override
      public ChunkGeneratorStructureState getGeneratorState() {
         return state;
      }
   }

   static class FakeServer extends MinecraftServer {
      WorldGenSettings settings;

      FakeServer() {
         super(null, null, null, null, Optional.empty(), null, null, null, null, false, null);
      }

      @Override
      public WorldGenSettings getWorldGenSettings() {
         return settings;
      }

      @Override
      public int getMaxPlayers() {
         return 0;
      }

      @Override
      protected boolean initServer() {
         return false;
      }

      @Override
      public net.minecraft.server.permissions.LevelBasedPermissionSet operatorUserPermissions() {
         return null;
      }

      @Override
      public net.minecraft.server.permissions.PermissionSet getFunctionCompilationPermissions() {
         return null;
      }

      @Override
      public boolean shouldRconBroadcast() {
         return false;
      }

      @Override
      protected net.minecraft.util.debugchart.SampleLogger getTickTimeLogger() {
         return null;
      }

      @Override
      public boolean isTickTimeLoggingEnabled() {
         return false;
      }

      @Override
      public net.minecraft.SystemReport fillServerSystemReport(net.minecraft.SystemReport report) {
         return report;
      }

      @Override
      public boolean isDedicatedServer() {
         return false;
      }

      @Override
      public int getRateLimitPacketsPerSecond() {
         return 0;
      }

      @Override
      public int getCommandSpamThresholdSeconds() {
         return 0;
      }

      @Override
      public int getChatSpamThresholdSeconds() {
         return 0;
      }

      @Override
      public boolean useNativeTransport() {
         return false;
      }

      @Override
      public boolean isPublished() {
         return false;
      }

      @Override
      public boolean shouldInformAdmins() {
         return false;
      }

      @Override
      public boolean isSingleplayerOwner(net.minecraft.server.players.NameAndId id) {
         return false;
      }

   }

   static class FakeLevel extends ServerLevel {
      FakeChunkCache cache;
      FakeServer server;
      FakeManager manager;

      FakeLevel() {
         super(null, null, null, null, null, null, false, 0L, List.of(), false);
      }

      @Override
      public ServerChunkCache getChunkSource() {
         return cache;
      }

      @Override
      public MinecraftServer getServer() {
         return server;
      }

      @Override
      public StructureManager structureManager() {
         return manager;
      }
   }

   @SuppressWarnings("unchecked")
   static void emitLocate(String dim, long seed, ChunkGeneratorStructureState state, List<Holder.Reference<StructureSet>> all,
                          Registry<Structure> structures) throws Exception {
      FakeManager manager = (FakeManager) UNSAFE.allocateInstance(FakeManager.class);
      manager.seed = seed;
      FakeChunkCache cache = (FakeChunkCache) UNSAFE.allocateInstance(FakeChunkCache.class);
      cache.state = state;
      FakeServer server = (FakeServer) UNSAFE.allocateInstance(FakeServer.class);
      server.settings = new WorldGenSettings(new WorldOptions(seed, true, false), null);
      FakeLevel level = (FakeLevel) UNSAFE.allocateInstance(FakeLevel.class);
      level.cache = cache;
      level.server = server;
      level.manager = manager;
      ChunkGenerator generator = (ChunkGenerator) UNSAFE.allocateInstance(net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator.class);

      Method spreadWalk = ChunkGenerator.class.getDeclaredMethod("getNearestGeneratedStructure", Set.class, net.minecraft.world.level.LevelReader.class,
         StructureManager.class, int.class, int.class, int.class, boolean.class, long.class, RandomSpreadStructurePlacement.class);
      spreadWalk.setAccessible(true);
      Method ringsWalk = ChunkGenerator.class.getDeclaredMethod("getNearestGeneratedStructure", Set.class, ServerLevel.class,
         StructureManager.class, BlockPos.class, boolean.class, ConcentricRingsStructurePlacement.class);
      ringsWalk.setAccessible(true);

      int[][] origins = {{0, 0}, {-37, 120}};
      if (dim.equals("overworld")) {
         for (var h : all) {
            if (!(h.value().placement() instanceof RandomSpreadStructurePlacement rs) || !id(h).startsWith("minecraft:")) {
               continue;
            }
            Holder<Structure> structure = structures.getOrThrow(ResourceKey.create(Registries.STRUCTURE, ((Holder<Structure>) h.value().structures().get(0).structure()).unwrapKey().get().identifier()));
            for (int[] o : origins) {
               for (int r = 0; r <= 5; r++) {
                  var res = (com.mojang.datafixers.util.Pair<BlockPos, Holder<Structure>>) spreadWalk.invoke(null, Set.of(structure), null, manager, o[0], o[1], r, false, seed, rs);
                  out.println("T " + dim + " " + seed + " " + id(h) + " " + o[0] + " " + o[1] + " " + r + " " + pair(res));
               }
            }
         }
      }
      for (var h : all) {
         if (!(h.value().placement() instanceof ConcentricRingsStructurePlacement rings) || state.getRingPositionsFor(rings) == null) {
            continue;
         }
         Holder<Structure> structure = h.value().structures().get(0).structure();
         for (int[] p : new int[][]{{0, 0}, {-5000, 3000}, {123, -77}}) {
            var res = (com.mojang.datafixers.util.Pair<BlockPos, Holder<Structure>>) ringsWalk.invoke(generator, Set.of(structure), level, manager, new BlockPos(p[0], 64, p[1]), false, rings);
            out.println("C " + dim + " " + seed + " " + id(h) + " " + p[0] + " " + p[1] + " " + pair(res));
         }
      }

      String[][] variants = dim.equals("nether")
         ? new String[][]{{"nether", "minecraft:fortress", "minecraft:bastion_remnant"}, {"fort", "minecraft:fortress"}, {"fossil", "minecraft:nether_fossil", "minecraft:ruined_portal_nether"}}
         : dim.equals("end")
         ? new String[][]{{"end", "minecraft:end_city"}}
         : new String[][]{
            {"villages", "minecraft:village_plains", "minecraft:village_desert", "minecraft:pillager_outpost"},
            {"stronghold", "minecraft:stronghold"},
            {"mixed", "minecraft:stronghold", "minecraft:village_plains", "minecraft:mansion", "minecraft:monument"},
            {"rare", "minecraft:mansion"},
            {"twice", "minecraft:village_plains", "minecraft:village_plains", "minecraft:stronghold"},
            {"custom", "minecraft:village_plains", "minecraft:mineshaft", "minecraft:fortress"},
            {"none", "minecraft:end_city"},
         };
      for (String[] v : variants) {
         List<Holder<Structure>> hs = new ArrayList<>();
         for (int i = 1; i < v.length; i++) {
            hs.add(structures.getOrThrow(ResourceKey.create(Registries.STRUCTURE, Identifier.parse(v[i]))));
         }
         for (int[] p : new int[][]{{0, 0}, {-5000, 3000}, {123, -77}}) {
            for (int radius : new int[]{0, 3, 40}) {
               var res = generator.findNearestMapStructure(level, HolderSet.direct(hs), new BlockPos(p[0], 64, p[1]), radius, false);
               out.println("M " + dim + " " + seed + " " + v[0] + " " + p[0] + " " + p[1] + " " + radius + " " + pair(res));
            }
         }
      }
   }

   static void emitState(String dim, long seed, ChunkGeneratorStructureState state, List<Holder.Reference<StructureSet>> all,
                         List<String> structureIds, Registry<Structure> structures) throws Exception {
      StringBuilder possible = new StringBuilder();
      for (var h : state.possibleStructureSets()) {
         possible.append(' ').append(id(h));
      }
      out.println("P " + dim + " " + seed + possible);
      boolean flat = dim.equals("flat");
      for (var h : flat ? state.possibleStructureSets() : all) {
         StructureSet set = h.value();
         String hid = id(h);
         if (!flat) {
            long hash = 0;
            int count = 0;
            StringBuilder first = new StringBuilder();
            for (int x = -60; x < 60; x++) {
               for (int z = -60; z < 60; z++) {
                  if (set.placement().isStructureChunk(state, x, z)) {
                     count++;
                     hash = mix(hash, x * 65537L + z);
                     if (count <= 5) {
                        first.append(' ').append(x).append(',').append(z);
                     }
                  }
               }
            }
            out.println("I " + dim + " " + seed + " " + hid + " " + count + " " + hash + first);
            int[][] pts = {{0, 0}, {-17, 33}, {250, -1000}, {-3, -3}};
            for (int[] pt : pts) {
               for (int range : new int[]{0, 1, 4, 11}) {
                  out.println("H " + dim + " " + seed + " " + hid + " " + pt[0] + " " + pt[1] + " " + range + " "
                     + state.hasStructureChunkInRange(h, pt[0], pt[1], range));
               }
            }
         }
         if (set.placement() instanceof ConcentricRingsStructurePlacement rings) {
            List<ChunkPos> positions = state.getRingPositionsFor(rings);
            if (positions != null) {
               StringBuilder sb = new StringBuilder();
               int a = 0;
               int b = 0;
               for (ChunkPos p : positions) {
                  sb.append(' ').append(p.x()).append(',').append(p.z());
                  if (rings.isStructureChunk(state, p.x(), p.z())) {
                     a++;
                  }
                  if (rings.isStructureChunk(state, p.x() + 1, p.z())) {
                     b++;
                  }
               }
               out.println("R " + dim + " " + seed + " " + hid + sb);
               out.println("Z " + dim + " " + seed + " " + hid + " " + a + " " + b);
            } else {
               out.println("R " + dim + " " + seed + " " + hid + " null");
            }
         }
      }
      if (!flat) {
         for (String sid : structureIds) {
            Holder<Structure> structure = structures.getOrThrow(ResourceKey.create(Registries.STRUCTURE, Identifier.parse(sid)));
            StringBuilder sb = new StringBuilder();
            for (StructurePlacement p : state.getPlacementsForStructure(structure)) {
               for (var h : all) {
                  // DimensionOriginStructurePlacement.INSTANCE is shared: only sets naming the structure count
                  if (h.value().placement() == p && h.value().structures().stream().anyMatch(e -> id(e.structure()).equals(sid))) {
                     sb.append(' ').append(id(h));
                  }
               }
            }
            out.println("Y " + dim + " " + seed + " " + sid + sb);
         }
      }
   }
}
