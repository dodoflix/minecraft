import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.NoiseColumn;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeGenerationSettings;
import net.minecraft.world.level.biome.BiomeSource;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.ChunkGenerator;
import net.minecraft.world.level.chunk.PalettedContainerFactory;
import net.minecraft.world.level.chunk.ProtoChunk;
import net.minecraft.world.level.chunk.UpgradeData;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.level.levelgen.DebugLevelSource;
import net.minecraft.world.level.levelgen.FlatLevelSource;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.flat.FlatLayerInfo;
import net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorPreset;
import net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings;
import net.minecraft.world.level.levelgen.feature.FillLayerFeature;
import net.minecraft.world.level.levelgen.placement.PlacedFeature;
import net.minecraft.world.level.levelgen.presets.WorldPreset;

/**
 * Goldens for flat/debug generators and world presets.
 *   java -cp tools/extractor/build:$(scripts/vanilla-cp.sh) crates/mc-worldgen/tests/oracle/FlatOracle.java <outdir>
 * Writes <outdir>/flat.txt, <outdir>/debug.txt, <outdir>/debug_grid.txt, <outdir>/world_presets.txt.
 * The Rust test (tests/flat_golden.rs) re-derives the same lines and compares them verbatim.
 */
public class FlatOracle {
   static RegistryAccess.Frozen access;
   static RegistryOps<JsonElement> ops;

   static final String[] CUSTOM = {
      "{\"layers\":[{\"block\":\"minecraft:stone\",\"height\":1}],\"biome\":\"minecraft:plains\"}",
      "{\"layers\":[]}",
      "{\"layers\":[{\"block\":\"minecraft:bedrock\",\"height\":1},{\"block\":\"minecraft:stone\",\"height\":10},{\"block\":\"minecraft:water\",\"height\":3},{\"block\":\"minecraft:glass\",\"height\":2},{\"block\":\"minecraft:snow\",\"height\":1},{\"block\":\"minecraft:short_grass\",\"height\":1}],\"biome\":\"minecraft:ocean\",\"features\":true,\"lakes\":true}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1},{\"block\":\"minecraft:air\",\"height\":2},{\"block\":\"minecraft:lava\",\"height\":2},{\"block\":\"minecraft:stone\",\"height\":0},{\"block\":\"minecraft:oak_leaves\",\"height\":1},{\"block\":\"minecraft:oak_slab\",\"height\":1}],\"biome\":\"minecraft:desert\",\"features\":true}",
      "{\"layers\":[{\"block\":\"minecraft:air\",\"height\":3}],\"biome\":\"minecraft:the_void\",\"features\":true,\"lakes\":true}",
      "{\"layers\":[{\"block\":\"minecraft:air\",\"height\":3}],\"biome\":\"minecraft:plains\",\"features\":true,\"lakes\":false}",
      "{\"layers\":[{\"block\":\"minecraft:air\",\"height\":3}],\"biome\":\"minecraft:the_void\",\"features\":false,\"lakes\":true}",
      "{\"layers\":[{\"block\":\"minecraft:bedrock\",\"height\":1},{\"block\":\"minecraft:netherrack\",\"height\":100},{\"block\":\"minecraft:lava\",\"height\":5}],\"biome\":\"minecraft:nether_wastes\",\"features\":true,\"lakes\":false}",
      "{\"layers\":[{\"block\":\"minecraft:stone\",\"height\":1000}],\"biome\":\"minecraft:plains\"}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":4064}],\"biome\":\"minecraft:taiga\",\"features\":true}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":4064},{\"block\":\"minecraft:stone\",\"height\":1}]}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":4065}]}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":-1}]}",
      "{\"layers\":[{\"block\":\"minecraft:nope\",\"height\":1}]}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\"}]}",
      "{\"layers\":[{\"height\":1}]}",
      "{\"biome\":\"minecraft:plains\"}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"biome\":\"minecraft:nope\"}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"biome\":7}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"lakes\":\"yes\"}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"features\":1}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"structure_overrides\":\"minecraft:villages\"}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"structure_overrides\":[\"minecraft:villages\",\"minecraft:strongholds\"]}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"structure_overrides\":[]}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"structure_overrides\":\"minecraft:nope\"}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"structure_overrides\":[\"minecraft:villages\",\"minecraft:nope\"]}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"structure_overrides\":\"#minecraft:nope\"}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1}],\"structure_overrides\":5}",
      "{\"layers\":[{\"block\":\"dirt\",\"height\":1.0},{\"block\":\"stone\",\"height\":2.9}],\"biome\":\"plains\"}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":1,\"extra\":true}],\"unknown_key\":1}",
      "{\"layers\":{\"block\":\"minecraft:dirt\",\"height\":1}}",
      "[]",
      "{\"layers\":[{\"block\":\"minecraft:water\",\"height\":64},{\"block\":\"minecraft:sand\",\"height\":2},{\"block\":\"minecraft:barrier\",\"height\":1},{\"block\":\"minecraft:water\",\"height\":300}],\"biome\":\"minecraft:warm_ocean\",\"features\":true,\"lakes\":true}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":true}]}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":\"3\"}]}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":2}],\"biome\":\"minecraft:Plains\"}",
      "{\"layers\":[{\"block\":\"minecraft:Dirt\",\"height\":2}]}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":2}],\"biome\":\"minecraft:desert\",\"features\":false,\"lakes\":true}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":2},{\"block\":\"minecraft:stone\",\"height\":4062}],\"biome\":\"minecraft:mushroom_fields\",\"features\":true,\"lakes\":true}",
      "{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":2}],\"structure_overrides\":null}",
   };

   // {minY, height}
   static final int[][] HEIGHTS = {{-64, 384}, {0, 256}, {0, 16}, {-64, 32}, {-2032, 4064}, {0, 3}};
   static final int[][] CHUNKS = {{0, 0}, {-3, 7}};

   static String id(BlockState s) {
      return Integer.toString(Block.getId(s));
   }

   static String stateOrNull(BlockState s) {
      return s == null ? "null" : id(s);
   }

   static String biomeKey(Holder<Biome> b) {
      return b.unwrapKey().map(k -> k.identifier().toString()).orElse("<inline>");
   }

   static <T> String holderSet(Optional<HolderSet<T>> set) {
      if (set.isEmpty()) {
         return "none";
      }
      return set.get().unwrap().map(tag -> "tag:" + tag.location(), list -> {
         StringBuilder sb = new StringBuilder("list:");
         for (Holder<T> h : list) {
            sb.append(h.unwrapKey().map(k -> k.identifier().toString()).orElse("<inline>")).append(',');
         }
         return sb.toString();
      });
   }

   static boolean field(FlatLevelGeneratorSettings s, String name) throws Exception {
      Field f = FlatLevelGeneratorSettings.class.getDeclaredField(name);
      f.setAccessible(true);
      return f.getBoolean(s);
   }

   static String layersInfo(FlatLevelGeneratorSettings s) {
      StringBuilder sb = new StringBuilder();
      for (FlatLayerInfo l : s.getLayersInfo()) {
         sb.append(l.getHeight()).append('*').append(Block.getId(l.getBlockState())).append(',');
      }
      return sb.toString();
   }

   static String rle(List<String> values) {
      StringBuilder sb = new StringBuilder();
      int i = 0;
      while (i < values.size()) {
         int j = i;
         while (j < values.size() && values.get(j).equals(values.get(i))) {
            j++;
         }
         sb.append(j - i).append(':').append(values.get(i)).append(' ');
         i = j;
      }
      return sb.toString().trim();
   }

   static String summary(FlatLevelGeneratorSettings s) throws Exception {
      return "layers=" + layersInfo(s)
         + " state_layers=" + rle(statesOf(s.getLayers()))
         + " biome=" + biomeKey(s.getBiome())
         + " lakes=" + field(s, "addLakes")
         + " features=" + field(s, "decoration")
         + " void=" + field(s, "voidGen")
         + " so=" + holderSet(s.structureOverrides());
   }

   static List<String> statesOf(List<BlockState> states) {
      List<String> out = new ArrayList<>();
      for (BlockState s : states) {
         out.add(stateOrNull(s));
      }
      return out;
   }

   static String placed(Holder<PlacedFeature> h) {
      Optional<String> key = h.unwrapKey().map(k -> k.identifier().toString());
      if (key.isPresent()) {
         return key.get();
      }
      PlacedFeature p = h.value();
      Object f = p.feature().value();
      if (f instanceof FillLayerFeature fill) {
         return "fill_layer(" + fill.height() + "," + Block.getId(fill.state()) + ")+" + p.placement().size();
      }
      return "<inline:" + f.getClass().getName() + ">";
   }

   static String settingsOf(BiomeGenerationSettings g, boolean full) {
      StringBuilder sb = new StringBuilder("carvers=");
      for (Holder<?> c : g.getCarvers()) {
         sb.append(c.unwrapKey().map(k -> k.identifier().toString()).orElse("<inline>")).append(',');
      }
      sb.append(" steps=").append(g.features().size());
      int total = 0;
      for (HolderSet<PlacedFeature> step : g.features()) {
         total += step.size();
      }
      sb.append(" total=").append(total);
      if (full) {
         int step = 0;
         for (HolderSet<PlacedFeature> set : g.features()) {
            sb.append(" |").append(step++).append(':');
            for (Holder<PlacedFeature> h : set) {
               sb.append(placed(h)).append(',');
            }
         }
      }
      return sb.toString();
   }

   static FlatLevelGeneratorSettings parse(String json) {
      JsonElement e = JsonParser.parseString(json);
      return FlatLevelGeneratorSettings.CODEC.parse(ops, e).result().orElse(null);
   }

   static ProtoChunk newChunk(int cx, int cz, int minY, int height) {
      return new ProtoChunk(new ChunkPos(cx, cz), UpgradeData.EMPTY, LevelHeightAccessor.create(minY, height), PalettedContainerFactory.create(access), null);
   }

   static String chunkDump(ProtoChunk chunk, int minY, int height) {
      List<String> blocks = new ArrayList<>();
      BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
      for (int y = minY; y < minY + height; y++) {
         for (int z = 0; z < 16; z++) {
            for (int x = 0; x < 16; x++) {
               blocks.add(id(chunk.getBlockState(p.set(x, y, z))));
            }
         }
      }
      StringBuilder sb = new StringBuilder("blocks " + rle(blocks));
      Map<Integer, String> maps = new TreeMap<>();
      for (var entry : chunk.getHeightmaps()) {
         List<String> h = new ArrayList<>();
         for (int z = 0; z < 16; z++) {
            for (int x = 0; x < 16; x++) {
               h.add(Integer.toString(entry.getValue().getFirstAvailable(x, z)));
            }
         }
         maps.put(entry.getKey().ordinal(), entry.getKey().getSerializationKey() + " " + rle(h));
      }
      for (String m : maps.values()) {
         sb.append(" ; ").append(m);
      }
      return sb.toString();
   }

   static void generatorFunctions(PrintWriter out, String tag, ChunkGenerator gen) {
      out.println(tag + " consts min_y=" + gen.getMinY() + " gen_depth=" + gen.getGenDepth() + " sea_level=" + gen.getSeaLevel());
      for (int[] h : HEIGHTS) {
         LevelHeightAccessor acc = LevelHeightAccessor.create(h[0], h[1]);
         StringBuilder sb = new StringBuilder(tag + " fn " + h[0] + " " + h[1] + " spawn=" + gen.getSpawnHeight(acc));
         for (Heightmap.Types t : Heightmap.Types.values()) {
            sb.append(' ').append(t.getSerializationKey()).append('=').append(gen.getBaseHeight(5, -9, t, acc, null))
               .append('/').append(gen.getFirstFreeHeight(5, -9, t, acc, null))
               .append('/').append(gen.getFirstOccupiedHeight(5, -9, t, acc, null));
         }
         out.println(sb);
         NoiseColumn col = gen.getBaseColumn(-17, 40, acc, null);
         List<String> states = new ArrayList<>();
         for (int y = h[0] - 3; y < h[0] + Math.min(h[1], 600) + 3; y++) {
            states.add(id(col.getBlock(y)));
         }
         out.println(tag + " col " + h[0] + " " + h[1] + " " + rle(states));
      }
   }

   static void flatCase(PrintWriter out, String name, String json, FlatLevelGeneratorSettings reference) throws Exception {
      out.println("INPUT " + name + " " + json);
      if (reference == null) {
         out.println("CASE " + name + " ERR");
         return;
      }
      out.println("CASE " + name + " OK " + summary(reference));
      // Two phases: before and after vanilla lazily calls adjustGenerationSettings (which nulls non-opaque layers).
      for (int phase = 0; phase < 2; phase++) {
         FlatLevelGeneratorSettings settings = parse(json);
         FlatLevelSource source = new FlatLevelSource(settings);
         String tag = name + (phase == 0 ? " pre" : " post");
         if (phase == 1) {
            Holder<Biome> flatBiome = settings.getBiome();
            out.println(tag + " adjust_flat " + settingsOf(source.getBiomeGenerationSettings(flatBiome), true));
            Holder<Biome> other = access.lookupOrThrow(Registries.BIOME).getOrThrow(net.minecraft.world.level.biome.Biomes.THE_VOID);
            Holder<Biome> other2 = access.lookupOrThrow(Registries.BIOME).getOrThrow(net.minecraft.world.level.biome.Biomes.PLAINS);
            out.println(tag + " adjust_void " + biomeKey(other) + " " + settingsOf(source.getBiomeGenerationSettings(other), false));
            out.println(tag + " adjust_plains " + biomeKey(other2) + " " + settingsOf(source.getBiomeGenerationSettings(other2), false));
            out.println(tag + " layers_after " + rle(statesOf(settings.getLayers())));
            out.println(tag + " source biomes=" + source.getBiomeSource().possibleBiomes().stream().map(FlatOracle::biomeKey).toList());
         }
         generatorFunctions(out, tag, source);
         for (int[] h : HEIGHTS) {
            for (int[] c : CHUNKS) {
               ProtoChunk chunk = newChunk(c[0], c[1], h[0], h[1]);
               source.buildTerrain(chunk, null, null, null, null, null, null).join();
               out.println(tag + " chunk " + h[0] + " " + h[1] + " " + c[0] + " " + c[1] + " " + chunkDump(chunk, h[0], h[1]));
            }
         }
      }
   }

   public static void main(String[] args) throws Exception {
      access = extractor.Setup.bootstrap();
      ops = RegistryOps.create(JsonOps.INSTANCE, access);
      String dir = args[0];
      try (PrintWriter out = new PrintWriter(dir + "/flat.txt")) {
         int i = 0;
         for (String json : CUSTOM) {
            flatCase(out, "custom" + i++, json, parse(json));
         }
         Registry<FlatLevelGeneratorPreset> presets = access.lookupOrThrow(Registries.FLAT_LEVEL_GENERATOR_PRESET);
         List<Identifier> keys = new ArrayList<>(presets.keySet());
         keys.sort((a, b) -> a.toString().compareTo(b.toString()));
         for (Identifier key : keys) {
            FlatLevelGeneratorPreset preset = presets.getValue(key);
            out.println("PRESET " + key + " display=" + BuiltInRegistries.ITEM.getKey(preset.displayItem().value()) + " " + summary(preset.settings()));
            // Round trip through the codec: preset settings -> JSON -> settings must keep everything.
            JsonElement encoded = FlatLevelGeneratorSettings.CODEC.encodeStart(ops, preset.settings()).result().orElseThrow();
            out.println("PRESET_JSON " + key + " " + encoded);
            flatCase(out, "preset:" + key, encoded.toString(), parse(encoded.toString()));
         }
         FlatLevelGeneratorSettings dflt = FlatLevelGeneratorSettings.getDefault(
            access.lookupOrThrow(Registries.BIOME), access.lookupOrThrow(Registries.STRUCTURE_SET), access.lookupOrThrow(Registries.PLACED_FEATURE));
         out.println("DEFAULT " + summary(dflt));
         JsonElement dj = FlatLevelGeneratorSettings.CODEC.encodeStart(ops, dflt).result().orElseThrow();
         out.println("DEFAULT_JSON " + dj);
         flatCase(out, "default", dj.toString(), parse(dj.toString()));
         // withBiomeAndLayers as used by flat_all_dimensions
         FlatLevelGeneratorSettings wb = dflt.withBiomeAndLayers(
            List.of(new FlatLayerInfo(1, net.minecraft.world.level.block.Blocks.BEDROCK), new FlatLayerInfo(3, net.minecraft.world.level.block.Blocks.BASALT)),
            Optional.empty(),
            access.lookupOrThrow(Registries.BIOME).getOrThrow(net.minecraft.world.level.biome.Biomes.BASALT_DELTAS));
         out.println("WITH_BIOME_AND_LAYERS " + summary(wb));
         // Vanilla's own bootstrap output (what the data pack JSON was generated from) must match the data pack entries.
         var lookup = net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
         lookup.lookupOrThrow(Registries.FLAT_LEVEL_GENERATOR_PRESET).listElements().sorted((a, b) -> a.key().identifier().toString().compareTo(b.key().identifier().toString())).forEach(h -> {
            try {
               out.println("BOOTSTRAP_PRESET " + h.key().identifier() + " display=" + BuiltInRegistries.ITEM.getKey(h.value().displayItem().value()) + " " + summary(h.value().settings()));
            } catch (Exception e) {
               throw new RuntimeException(e);
            }
         });
      }

      // ---- debug level source
      try (PrintWriter out = new PrintWriter(dir + "/debug.txt"); PrintWriter grid = new PrintWriter(dir + "/debug_grid.txt")) {
         Field allBlocks = DebugLevelSource.class.getDeclaredField("ALL_BLOCKS");
         allBlocks.setAccessible(true);
         @SuppressWarnings("unchecked")
         List<BlockState> all = (List<BlockState>) allBlocks.get(null);
         Field gw = DebugLevelSource.class.getDeclaredField("GRID_WIDTH");
         gw.setAccessible(true);
         Field gh = DebugLevelSource.class.getDeclaredField("GRID_HEIGHT");
         gh.setAccessible(true);
         int gridWidth = gw.getInt(null);
         int gridHeight = gh.getInt(null);
         List<String> ids = new ArrayList<>();
         for (BlockState s : all) {
            ids.add(id(s));
         }
         // consecutive runs of ids (a..b) instead of single ones
         StringBuilder runs = new StringBuilder();
         int i = 0;
         while (i < all.size()) {
            int start = Block.getId(all.get(i));
            int j = i;
            while (j + 1 < all.size() && Block.getId(all.get(j + 1)) == Block.getId(all.get(j)) + 1) {
               j++;
            }
            runs.append(start).append("..").append(Block.getId(all.get(j))).append(' ');
            i = j + 1;
         }
         out.println("ALL_BLOCKS size=" + all.size() + " runs=" + runs.toString().trim());
         out.println("GRID width=" + gridWidth + " height=" + gridHeight);
         out.println("CONSTS height=" + DebugLevelSource.HEIGHT + " barrier=" + DebugLevelSource.BARRIER_HEIGHT);
         Holder.Reference<Biome> plains = access.lookupOrThrow(Registries.BIOME).getOrThrow(net.minecraft.world.level.biome.Biomes.PLAINS);
         DebugLevelSource source = new DebugLevelSource(plains);
         out.println("SOURCE biomes=" + source.getBiomeSource().possibleBiomes().stream().map(FlatOracle::biomeKey).toList());
         out.println("SOURCE_SETTINGS " + settingsOf(source.getBiomeGenerationSettings(plains), false));
         generatorFunctions(out, "debug", source);
         // full grid including a 4-wide ring outside
         for (int x = -4; x <= 2 * gridWidth + 5; x++) {
            for (int z = -4; z <= 2 * gridHeight + 5; z++) {
               BlockState s = DebugLevelSource.getBlockStateFor(x, z);
               if (!s.isAir()) {
                  grid.println(x + " " + z + " " + id(s));
               }
            }
         }
         int[] samples = {Integer.MIN_VALUE, Integer.MIN_VALUE + 1, -1000000, -3, -2, -1, 0, 1, 2, 3, 5, 7, 99, 351, 353, 355, 1000000, Integer.MAX_VALUE - 1, Integer.MAX_VALUE};
         for (int x : samples) {
            for (int z : samples) {
               out.println("STATE_FOR " + x + " " + z + " " + id(DebugLevelSource.getBlockStateFor(x, z)));
            }
         }
         // applyBiomeDecoration writes: capture the setBlock calls through a WorldGenLevel proxy
         for (int[] c : new int[][]{{0, 0}, {-1, -1}, {5, 3}, {-7, 11}, {10, 10}, {12, 20}}) {
            ProtoChunk chunk = newChunk(c[0], c[1], -64, 384);
            List<String> writes = new ArrayList<>();
            net.minecraft.world.level.WorldGenLevel level = (net.minecraft.world.level.WorldGenLevel) java.lang.reflect.Proxy.newProxyInstance(
               FlatOracle.class.getClassLoader(), new Class<?>[]{net.minecraft.world.level.WorldGenLevel.class}, (proxy, method, a) -> {
                  if (method.getName().equals("setBlock")) {
                     BlockPos pos = (BlockPos) a[0];
                     writes.add(pos.getX() + "," + pos.getY() + "," + pos.getZ() + "=" + id((BlockState) a[1]) + "/" + a[2]);
                     return true;
                  }
                  throw new UnsupportedOperationException(method.getName());
               });
            source.applyBiomeDecoration(level, chunk, null);
            out.println("DECORATE " + c[0] + " " + c[1] + " " + writes.size() + " " + String.join(" ", writes));
         }
         ProtoChunk chunk = newChunk(0, 0, -64, 384);
         source.buildTerrain(chunk, null, null, null, null, null, null).join();
         out.println("TERRAIN " + chunkDump(chunk, -64, 384));
      }

      // ---- world presets
      try (PrintWriter out = new PrintWriter(dir + "/world_presets.txt")) {
         Registry<WorldPreset> presets = access.lookupOrThrow(Registries.WORLD_PRESET);
         List<Identifier> keys = new ArrayList<>(presets.keySet());
         keys.sort((a, b) -> a.toString().compareTo(b.toString()));
         for (Identifier key : keys) {
            WorldPreset preset = presets.getValue(key);
            dumpDims(out, "DIM ", key, preset, ops);
         }
         var lookup = net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
         for (Identifier key : keys) {
            dumpDims(out, "BOOTSTRAP_DIM ", key, lookup.lookupOrThrow(Registries.WORLD_PRESET).getOrThrow(net.minecraft.resources.ResourceKey.create(Registries.WORLD_PRESET, key)).value(), lookup.createSerializationContext(JsonOps.INSTANCE));
         }
      }
   }

   static void dumpDims(PrintWriter out, String prefix, Identifier key, WorldPreset preset, com.mojang.serialization.DynamicOps<JsonElement> ops) throws Exception {
      {
         {
            for (var entry : preset.createWorldDimensions().dimensions().entrySet()) {
               LevelStem stem = entry.getValue();
               ChunkGenerator gen = stem.generator();
               StringBuilder sb = new StringBuilder(prefix + key + " " + entry.getKey().identifier());
               sb.append(" type=").append(stem.type().unwrapKey().map(k -> k.identifier().toString()).orElse("<inline>"));
               sb.append(" generator=").append(gen.getTypeNameForDataFixer().orElseThrow());
               JsonElement biomeSource = BiomeSource.CODEC.encodeStart(ops, gen.getBiomeSource()).result().orElseThrow();
               sb.append(" biome_source=").append(biomeSource);
               if (gen instanceof NoiseBasedChunkGenerator noise) {
                  sb.append(" settings=").append(noise.generatorSettings().unwrapKey().map(k -> k.identifier().toString()).orElse("<inline>"));
               } else if (gen instanceof FlatLevelSource flat) {
                  sb.append(" flat=").append(summary(flat.settings()));
               }
               out.println(sb);
            }
         }
      }
   }
}
