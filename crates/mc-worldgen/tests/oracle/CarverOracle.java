import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.util.LinkedHashMap;
import java.util.Map;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.Registries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.CarvingMask;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.WorldGenerationContext;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.carver.WorldCarver;

/**
 * Golden generator for the carvers: replays NoiseBasedChunkGenerator.generateCarvers' carver loop
 * (setLargeFeatureSeed(seed + index, srcX, srcZ); isStartChunk; carve) for all source chunks within +-8 of a target.
 * Usage: scripts/vanilla-java.sh crates/mc-worldgen/tests/oracle/CarverOracle.java crates/mc-worldgen/tests/fixtures/carvers.txt crates/mc-worldgen/tests/fixtures/carver_masks.txt
 *
 * carvers.txt:
 *   E OK|ERR json                                       accept/reject of WorldCarver.DIRECT_CODEC for an edge-case JSON
 *   D name json                                         custom carver definition (vanilla's four are read from the data pack)
 *   R name dim seed index tx tz starts carves hash segs blocks maskhash after
 *        starts = isStartChunk true count, carves = carve returned true count, hash = rolling hash over
 *        (start, result, next long) of every source chunk, segs/blocks/maskhash = CarvingMask.visit digest,
 *        after = next long of the random after the last source chunk
 * carver_masks.txt (full dump, subset): S name dim seed index tx tz x z bottomY topY
 */
public class CarverOracle {
   static final String[][] CUSTOM = {
      {"heavy_cave", "{\"type\":\"minecraft:cave\",\"probability\":1.0,\"y\":{\"type\":\"minecraft:weighted_list\",\"distribution\":[{\"data\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":{\"absolute\":-20},\"max_inclusive\":{\"absolute\":40}},\"weight\":3},{\"data\":{\"above_bottom\":20},\"weight\":1}]},\"count\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":2,\"max_inclusive\":6},\"thickness\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0.5,\"max_exclusive\":4.0},\"weird_thickness_bias\":true,\"room_vertical_radius_multiplier\":0.6,\"horizontal_radius_multiplier\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0.8,\"max_exclusive\":1.6},\"vertical_radius_multiplier\":1.1,\"start_vertical_radius_multiplier\":{\"type\":\"minecraft:trapezoid\",\"min\":0.5,\"max\":3.0,\"plateau\":1.0},\"floor_level\":-0.5}"},
      {"gaussian_cave", "{\"type\":\"minecraft:cave\",\"probability\":0.9,\"y\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":{\"above_bottom\":4},\"max_inclusive\":{\"relative_to_sea_level\":30}},\"count\":{\"type\":\"minecraft:biased_to_bottom\",\"min_inclusive\":1,\"max_inclusive\":5},\"thickness\":{\"type\":\"minecraft:clamped_normal\",\"mean\":2.0,\"deviation\":1.5,\"min\":0.2,\"max\":5.0},\"weird_thickness_bias\":false,\"room_vertical_radius_multiplier\":{\"type\":\"minecraft:clamped_normal\",\"mean\":0.5,\"deviation\":0.3,\"min\":0.1,\"max\":1.0},\"horizontal_radius_multiplier\":1.0,\"vertical_radius_multiplier\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0.5,\"max_exclusive\":2.0},\"floor_level\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":-1.0,\"max_exclusive\":0.2}}"},
      {"top_cave", "{\"type\":\"minecraft:cave\",\"probability\":1.0,\"y\":{\"type\":\"minecraft:constant\",\"value\":{\"below_top\":5}},\"count\":{\"type\":\"minecraft:constant\",\"value\":4},\"thickness\":{\"type\":\"minecraft:trapezoid\",\"min\":1.0,\"max\":7.0,\"plateau\":2.0},\"room_vertical_radius_multiplier\":0.5,\"horizontal_radius_multiplier\":1.0,\"vertical_radius_multiplier\":1.0,\"start_vertical_radius_multiplier\":5.0,\"floor_level\":-0.7}"},
      {"wide_canyon", "{\"type\":\"minecraft:canyon\",\"probability\":1.0,\"y\":{\"type\":\"minecraft:trapezoid\",\"min_inclusive\":{\"above_bottom\":10},\"max_inclusive\":{\"below_top\":40},\"plateau\":20},\"vertical_rotation\":{\"type\":\"minecraft:clamped_normal\",\"mean\":0.0,\"deviation\":0.1,\"min\":-0.3,\"max\":0.3},\"shape\":{\"distance_factor\":1.0,\"thickness\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":2.0,\"max_exclusive\":7.0},\"width_smoothness\":1,\"horizontal_radius_factor\":{\"type\":\"minecraft:clamped_normal\",\"mean\":1.0,\"deviation\":0.2,\"min\":0.5,\"max\":1.5},\"vertical_radius_default_factor\":0.7,\"vertical_radius_center_factor\":0.5,\"y_scale\":2.5}}"},
      {"smooth_canyon", "{\"type\":\"minecraft:canyon\",\"probability\":0.5,\"y\":{\"absolute\":50},\"vertical_rotation\":0.05,\"shape\":{\"distance_factor\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0.5,\"max_exclusive\":1.0},\"thickness\":4.0,\"width_smoothness\":7,\"horizontal_radius_factor\":1.0,\"vertical_radius_default_factor\":1.0,\"vertical_radius_center_factor\":0.0,\"y_scale\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":2.0,\"max_exclusive\":4.0}}}"},
   };
   static final String[] VANILLA = {"cave", "cave_extra_underground", "canyon", "nether_cave"};
   static final Object[][] DIMS = {{"overworld", -64, 384, 63}, {"nether", 0, 128, 32}, {"custom", -32, 112, 10}};
   static final long[] SEEDS = {12345L, -4172144997902289642L, 0L, 987654321012L};
   static final int[] INDEXES = {0, 3};
   static final int[][] TARGETS = {{0, 0}, {-5, 7}, {113, -64}, {2, 2}};


   // Codec edge cases: "E <OK|ERR> json" (accept/reject of WorldCarver.DIRECT_CODEC).
   static final String CAVE_BASE = "\"y\":{\"absolute\":10},\"count\":3,\"thickness\":1.0,\"room_vertical_radius_multiplier\":0.5,\"horizontal_radius_multiplier\":1.0,\"vertical_radius_multiplier\":1.0,\"floor_level\":-0.5";
   static final String SHAPE = "\"shape\":{\"distance_factor\":1.0,\"thickness\":3.0,\"width_smoothness\":3,\"horizontal_radius_factor\":1.0,\"vertical_radius_default_factor\":1.0,\"vertical_radius_center_factor\":0.0,\"y_scale\":3.0}";
   static final String[] EDGE = {
      "{\"type\":\"minecraft:cave\",\"probability\":0.5," + CAVE_BASE + "}",
      "{\"type\":\"cave\",\"probability\":0.0," + CAVE_BASE + "}",
      "{\"type\":\"cave\",\"probability\":1.0," + CAVE_BASE + ",\"extra\":7}",
      "{\"type\":\"cave\",\"probability\":1.01," + CAVE_BASE + "}",
      "{\"type\":\"cave\",\"probability\":-0.01," + CAVE_BASE + "}",
      "{\"type\":\"cave\"," + CAVE_BASE + "}",
      "{\"type\":\"cave\",\"probability\":0.5}",
      "{\"probability\":0.5," + CAVE_BASE + "}",
      "{\"type\":\"nope\",\"probability\":0.5," + CAVE_BASE + "}",
      "{\"type\":\"cave\",\"probability\":0.5," + CAVE_BASE + ",\"weird_thickness_bias\":true}",
      "{\"type\":\"cave\",\"probability\":0.5," + CAVE_BASE + ",\"weird_thickness_bias\":1}",
      "{\"type\":\"cave\",\"probability\":0.5," + CAVE_BASE + ",\"start_vertical_radius_multiplier\":{\"type\":\"uniform\",\"min_inclusive\":1,\"max_exclusive\":2}}",
      "{\"type\":\"cave\",\"probability\":0.5," + CAVE_BASE + ",\"start_vertical_radius_multiplier\":\"x\"}",
      "{\"type\":\"cave\",\"probability\":0.5,\"y\":{\"absolute\":10},\"count\":-1,\"thickness\":1.0,\"room_vertical_radius_multiplier\":0.5,\"horizontal_radius_multiplier\":1.0,\"vertical_radius_multiplier\":1.0,\"floor_level\":-0.5}",
      "{\"type\":\"cave\",\"probability\":0.5,\"y\":{\"absolute\":10},\"count\":{\"type\":\"uniform\",\"min_inclusive\":0,\"max_inclusive\":9},\"thickness\":-0.5,\"room_vertical_radius_multiplier\":0.5,\"horizontal_radius_multiplier\":1.0,\"vertical_radius_multiplier\":1.0,\"floor_level\":-0.5}",
      "{\"type\":\"cave\",\"probability\":0.5,\"y\":{\"absolute\":10},\"count\":0,\"thickness\":0.0,\"room_vertical_radius_multiplier\":-5.0,\"horizontal_radius_multiplier\":100.0,\"vertical_radius_multiplier\":1.0,\"floor_level\":-1.0}",
      "{\"type\":\"cave\",\"probability\":0.5,\"y\":{\"absolute\":10},\"count\":0,\"thickness\":0.0,\"room_vertical_radius_multiplier\":0.5,\"horizontal_radius_multiplier\":1.0,\"vertical_radius_multiplier\":1.0,\"floor_level\":1.0}",
      "{\"type\":\"cave\",\"probability\":0.5,\"y\":{\"absolute\":10},\"count\":0,\"thickness\":0.0,\"room_vertical_radius_multiplier\":0.5,\"horizontal_radius_multiplier\":1.0,\"vertical_radius_multiplier\":1.0,\"floor_level\":1.5}",
      "{\"type\":\"cave\",\"probability\":0.5,\"y\":{\"absolute\":10},\"count\":0,\"thickness\":0.0,\"room_vertical_radius_multiplier\":0.5,\"horizontal_radius_multiplier\":1.0,\"vertical_radius_multiplier\":1.0,\"floor_level\":{\"type\":\"uniform\",\"min_inclusive\":-1.0,\"max_exclusive\":1.0}}",
      "{\"type\":\"cave\",\"probability\":0.5,\"y\":{\"absolute\":10},\"count\":0,\"thickness\":0.0,\"room_vertical_radius_multiplier\":0.5,\"horizontal_radius_multiplier\":1.0,\"vertical_radius_multiplier\":1.0,\"floor_level\":{\"type\":\"uniform\",\"min_inclusive\":-1.0,\"max_exclusive\":1.5}}",
      "{\"type\":\"cave\",\"probability\":0.5,\"y\":7,\"count\":1,\"thickness\":1.0,\"room_vertical_radius_multiplier\":0.5,\"horizontal_radius_multiplier\":1.0,\"vertical_radius_multiplier\":1.0,\"floor_level\":-0.5}",
      "{\"type\":\"canyon\",\"probability\":0.01,\"y\":{\"absolute\":10},\"vertical_rotation\":0.1," + SHAPE + "}",
      "{\"type\":\"minecraft:canyon\",\"probability\":2,\"y\":{\"absolute\":10},\"vertical_rotation\":0.1," + SHAPE + "}",
      "{\"type\":\"canyon\",\"probability\":0.01,\"y\":{\"absolute\":10}," + SHAPE + "}",
      "{\"type\":\"canyon\",\"probability\":0.01,\"y\":{\"absolute\":10},\"vertical_rotation\":0.1}",
      "{\"type\":\"canyon\",\"probability\":0.01,\"y\":{\"absolute\":10},\"vertical_rotation\":0.1,\"shape\":{\"distance_factor\":1.0}}",
      "{\"type\":\"canyon\",\"probability\":0.01,\"y\":{\"absolute\":10},\"vertical_rotation\":0.1,\"shape\":{\"distance_factor\":1.0,\"thickness\":3.0,\"width_smoothness\":0,\"horizontal_radius_factor\":1.0,\"vertical_radius_default_factor\":1.0,\"vertical_radius_center_factor\":0.0,\"y_scale\":3.0}}",
      "{\"type\":\"canyon\",\"probability\":0.01,\"y\":{\"absolute\":10},\"vertical_rotation\":0.1,\"shape\":{\"distance_factor\":1.0,\"thickness\":3.0,\"width_smoothness\":-3,\"horizontal_radius_factor\":1.0,\"vertical_radius_default_factor\":1.0,\"vertical_radius_center_factor\":0.0,\"y_scale\":3.0}}",
      "{\"type\":\"canyon\",\"probability\":0.01,\"y\":{\"absolute\":10},\"vertical_rotation\":0.1,\"shape\":{\"distance_factor\":1.0,\"thickness\":3.0,\"width_smoothness\":2.5,\"horizontal_radius_factor\":1.0,\"vertical_radius_default_factor\":1.0,\"vertical_radius_center_factor\":0.0,\"y_scale\":3.0}}",
      "{\"type\":\"canyon\",\"probability\":0.01,\"y\":{\"absolute\":10},\"vertical_rotation\":0.1,\"shape\":{\"distance_factor\":1.0,\"thickness\":3.0,\"width_smoothness\":2,\"horizontal_radius_factor\":1.0,\"vertical_radius_default_factor\":\"1\",\"vertical_radius_center_factor\":0.0,\"y_scale\":3.0}}",
      "{\"type\":\"canyon\",\"probability\":0.01,\"y\":{\"type\":\"nope\"},\"vertical_rotation\":0.1," + SHAPE + "}",
      "[]", "5", "\"minecraft:cave\"", "{}",
   };

   static WorldGenerationContext ctx(int minY, int height, int sea) throws Exception {
      Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
      uf.setAccessible(true);
      sun.misc.Unsafe unsafe = (sun.misc.Unsafe)uf.get(null);
      WorldGenerationContext context = (WorldGenerationContext)unsafe.allocateInstance(WorldGenerationContext.class);
      int[] values = {minY, height, sea};
      String[] names = {"minY", "height", "seaLevel"};
      for (int i = 0; i < 3; i++) {
         Field f = WorldGenerationContext.class.getDeclaredField(names[i]);
         f.setAccessible(true);
         f.setInt(context, values[i]);
      }
      return context;
   }

   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      HolderLookup.Provider provider = VanillaRegistries.createWorldLookup();
      HolderLookup<WorldCarver> lookup = provider.lookupOrThrow(Registries.CARVER);
      Map<String, WorldCarver> carvers = new LinkedHashMap<>();
      for (String name : VANILLA) {
         carvers.put(name, lookup.getOrThrow(ResourceKey.create(Registries.CARVER, Identifier.withDefaultNamespace(name))).value());
      }
      PrintWriter out = new PrintWriter(args[0]);
      PrintWriter dump = new PrintWriter(args[1]);
      for (String[] custom : CUSTOM) {
         out.println("D " + custom[0] + " " + custom[1]);
         carvers.put(custom[0], WorldCarver.DIRECT_CODEC.parse(JsonOps.INSTANCE, JsonParser.parseString(custom[1])).getOrThrow());
      }

      for (String json : EDGE) {
         boolean ok;
         try {
            ok = WorldCarver.DIRECT_CODEC.parse(JsonOps.INSTANCE, JsonParser.parseString(json)).result().isPresent();
         } catch (Throwable t) {
            ok = false;
         }
         out.println("E " + (ok ? "OK" : "ERR") + " " + json);
      }

      for (Map.Entry<String, WorldCarver> entry : carvers.entrySet()) {
         String name = entry.getKey();
         WorldCarver carver = entry.getValue();
         for (Object[] dim : DIMS) {
            String dimName = (String)dim[0];
            int minY = (Integer)dim[1];
            int height = (Integer)dim[2];
            WorldGenerationContext context = ctx(minY, height, (Integer)dim[3]);
            for (long seed : SEEDS) {
               for (int index : INDEXES) {
                  for (int[] target : TARGETS) {
                     ChunkPos pos = new ChunkPos(target[0], target[1]);
                     CarvingMask mask = new CarvingMask(minY + 1, minY + height - 1 - 7);
                     WorldgenRandom random = new WorldgenRandom(new LegacyRandomSource(0));
                     int starts = 0;
                     int carves = 0;
                     long hash = 0;
                     for (int dx = -8; dx <= 8; dx++) {
                        for (int dz = -8; dz <= 8; dz++) {
                           ChunkPos sourcePos = new ChunkPos(pos.x() + dx, pos.z() + dz);
                           random.setLargeFeatureSeed(seed + index, sourcePos.x(), sourcePos.z());
                           boolean start = carver.isStartChunk(random);
                           boolean result = false;
                           if (start) {
                              starts++;
                              result = carver.carve(context, random, pos, sourcePos, mask);
                              if (result) carves++;
                           }
                           hash = hash * 31 + (start ? 1 : 0) + (result ? 2 : 0);
                           hash = hash * 31 + random.nextLong();
                        }
                     }
                     long after = random.nextLong();
                     long[] digest = new long[3]; // segs, blocks, hash
                     boolean dumpThis = seed == 12345L && index == 0;
                     String prefix = "S " + name + " " + dimName + " " + seed + " " + index + " " + target[0] + " " + target[1] + " ";
                     mask.visit((x, z, b, t) -> {
                        digest[0]++;
                        digest[1] += t - b + 1;
                        digest[2] = digest[2] * 31 + (((long)x << 40) ^ ((long)z << 32) ^ ((long)(b + 100000) << 16) ^ (t + 100000));
                        if (dumpThis) dump.println(prefix + x + " " + z + " " + b + " " + t);
                     });
                     out.println("R " + name + " " + dimName + " " + seed + " " + index + " " + target[0] + " " + target[1] + " " + starts + " "
                        + carves + " " + hash + " " + digest[0] + " " + digest[1] + " " + digest[2] + " " + after);
                  }
               }
            }
         }
      }
      out.close();
      dump.close();
      System.exit(0);
   }
}
