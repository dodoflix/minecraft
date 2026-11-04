import extractor.Setup;
import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderGetter;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.blending.Blender;
import net.minecraft.world.level.levelgen.densityfunction.DensityVolume;
import net.minecraft.world.level.levelgen.material.MaterialSystem;
import net.minecraft.world.level.levelgen.material.rule.MaterialRule;
import net.minecraft.world.level.levelgen.synth.NormalNoise;

/**
 * Golden generator for the material system (surface build / top_material / bands / surface depth).
 * Runs the real vanilla NoiseBasedChunkGenerator.doFill, then MaterialSystem.buildSurface, and dumps the
 * doFill output (input of the Rust side), the per-quart biomes the BiomeManager reads, and the result.
 *
 *   java -cp tools/extractor/build:$(scripts/vanilla-cp.sh) crates/mc-worldgen/tests/oracle/MaterialSurfaceOracle.java \
 *        /tmp/material_surface.txt && gzip -9 -c /tmp/material_surface.txt > crates/mc-worldgen/tests/fixtures/material_surface.txt.gz
 *
 * Lines (tokens space separated):
 *   CASE name seed cx cz minY height        starts a chunk case
 *   PAL n name...                            biome palette of the case
 *   Q qx0 qy0 qz0 sx sy sz tok...            biome table: palette index RLE "idx:count", order qy, qz, qx (qx fastest)
 *   POSS n idx...                            possibleBiomes (3x3 chunks)
 *   S section tok...                         doFill block states of a non-empty section, RLE "stateId:count", order y,z,x
 *   PB section v...  / PA section v...       post-process packed offsets before / after surface
 *   HB ws|ofl 256 v...  / HA ws|ofl 256 v... WORLD_SURFACE_WG / OCEAN_FLOOR_WG getHeight(x,z) before / after, z*16+x
 *   D n x,y,z,id ...                         block changes made by buildSurface
 *   T x y z fluid id                         topMaterial on the doFill chunk (id -1 = empty)
 *   END
 *   BANDS name seed 192 ids / B name seed x y z id / SD name seed x z depth / SS name seed x z bits
 */
public class MaterialSurfaceOracle {
   static RegistryAccess.Frozen full;
   static PrintWriter out;

   record Spec(String settings, String source, long seed, int minY, int height, String[] targets, int fallbackCount) {}

   static final String[] SETTINGS_ALL = {"overworld", "large_biomes", "amplified", "nether", "end", "caves", "floating_islands"};

   public static void main(String[] args) throws Exception {
      full = Setup.bootstrap();
      out = new PrintWriter(new java.io.File(args[0])); // vanilla logging also goes to stdout
      List<Spec> specs = new ArrayList<>();
      // overworld: one chunk per overworld biome (scan over several seeds)
      specs.add(new Spec("overworld", "overworld", 12345L, -64, 384, null, 0));
      specs.add(new Spec("large_biomes", "overworld", 777L, -64, 384,
         new String[] {"badlands", "eroded_badlands", "frozen_ocean", "ice_spikes", "desert", "mushroom_fields"}, 0));
      specs.add(new Spec("amplified", "overworld", -4172144997902289642L, -64, 384,
         new String[] {"jagged_peaks", "stony_peaks", "frozen_peaks", "eroded_badlands", "snowy_slopes", "grove"}, 0));
      specs.add(new Spec("nether", "nether", 4242L, 0, 256, null, 0));
      specs.add(new Spec("end", "end", 99L, 0, 256, null, 0));
      specs.add(new Spec("caves", "overworld", 2024L, -64, 384, new String[] {"lush_caves", "dripstone_caves", "plains"}, 0));
      specs.add(new Spec("floating_islands", "overworld", 31337L, -64, 384, new String[] {"plains", "desert", "ocean"}, 0));
      for (Spec spec : specs) runSpec(spec);
      for (String s : SETTINGS_ALL) {
         for (long seed : new long[] {12345L, -4172144997902289642L}) bands(s, seed);
      }
      out.flush();
   }

   static NoiseGeneratorSettings settings(String name) {
      return full.lookupOrThrow(Registries.NOISE_SETTINGS)
         .getOrThrow(ResourceKey.create(Registries.NOISE_SETTINGS, Identifier.withDefaultNamespace(name))).value();
   }

   static BiomeSource biomeSource(String kind) {
      if (kind.equals("end")) {
         return TheEndBiomeSource.create(full.lookupOrThrow(Registries.BIOME));
      }
      ResourceKey<MultiNoiseBiomeSourceParameterList> key =
         kind.equals("nether") ? MultiNoiseBiomeSourceParameterLists.NETHER : MultiNoiseBiomeSourceParameterLists.OVERWORLD;
      return MultiNoiseBiomeSource.createFromPreset(full.lookupOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST).getOrThrow(key));
   }

   static String biomeName(Holder<Biome> h) {
      return h.unwrapKey().get().identifier().toString();
   }

   static void bands(String name, long seed) throws Exception {
      NoiseGeneratorSettings settings = settings(name);
      HolderGetter<NormalNoise> noises = full.lookupOrThrow(Registries.NOISE);
      RandomState rs = RandomState.create(noises, seed, settings);
      MaterialSystem ms = rs.surfaceSystem();
      Field f = MaterialSystem.class.getDeclaredField("clayBands");
      f.setAccessible(true);
      BlockState[] bands = (BlockState[]) f.get(ms);
      StringBuilder sb = new StringBuilder("BANDS " + name + " " + seed + " " + bands.length);
      for (BlockState b : bands) sb.append(' ').append(Block.BLOCK_STATE_REGISTRY.getId(b));
      out.println(sb);
      Method getBand = MaterialSystem.class.getDeclaredMethod("getBand", int.class, int.class, int.class);
      Method getDepth = MaterialSystem.class.getDeclaredMethod("getSurfaceDepth", int.class, int.class);
      Method getSec = MaterialSystem.class.getDeclaredMethod("getSurfaceSecondary", int.class, int.class);
      getBand.setAccessible(true);
      getDepth.setAccessible(true);
      getSec.setAccessible(true);
      Random r = new Random(seed ^ name.hashCode());
      for (int i = 0; i < 300; i++) {
         int x = r.nextInt(20000) - 10000, y = r.nextInt(400) - 80, z = r.nextInt(20000) - 10000;
         if (i < 4) { x = i * 7; z = -i * 13; }
         BlockState b = (BlockState) getBand.invoke(ms, x, y, z);
         out.println("B " + name + " " + seed + " " + x + " " + y + " " + z + " " + Block.BLOCK_STATE_REGISTRY.getId(b));
         int d = (int) getDepth.invoke(ms, x, z);
         out.println("SD " + name + " " + seed + " " + x + " " + z + " " + d);
         double s = (double) getSec.invoke(ms, x, z);
         out.println("SS " + name + " " + seed + " " + x + " " + z + " " + Long.toHexString(Double.doubleToRawLongBits(s)));
      }
   }

   /** Candidate scan result for a target biome. */
   static void runSpec(Spec spec) throws Exception {
      NoiseGeneratorSettings settings = settings(spec.settings);
      HolderGetter<NormalNoise> noises = full.lookupOrThrow(Registries.NOISE);
      BiomeSource source = biomeSource(spec.source);
      List<String> targets;
      if (spec.targets != null) {
         targets = Arrays.asList(spec.targets);
      } else {
         TreeSet<String> names = new TreeSet<>();
         for (Holder<Biome> h : source.possibleBiomes()) names.add(biomeName(h).replace("minecraft:", ""));
         targets = new ArrayList<>(names);
      }
      // try several seeds per spec: the first seed is the spec's, further seeds fill in missing biomes
      long[] seeds = {spec.seed, spec.seed + 1, spec.seed * 31 + 7, spec.seed ^ 0x5DEECE66DL, spec.seed + 99999};
      Set<String> remaining = new LinkedHashSet<>(targets);
      for (long seed : seeds) {
         if (remaining.isEmpty()) break;
         RandomState rs = RandomState.create(noises, seed, settings);
         BiomeResolver resolver = source.createUncachedResolver(rs);
         NoiseBasedChunkGenerator gen = new NoiseBasedChunkGenerator(source, Holder.direct(settings));
         LevelHeightAccessor acc = LevelHeightAccessor.create(spec.minY, spec.height);
         int minQ = spec.minY >> 2, maxQ = minQ + (spec.height >> 2) - 1;
         Map<String, int[]> found = new LinkedHashMap<>();
         int R = spec.source.equals("end") ? 150 : 120;
         int step = 3;
         search:
         for (int ring = 0; ring <= R; ring += step) {
            for (int cx = -ring; cx <= ring; cx += step) {
               for (int cz = -ring; cz <= ring; cz += step) {
                  if (Math.max(Math.abs(cx), Math.abs(cz)) != ring) continue;
                  int bx = cx * 16 + 8, bz = cz * 16 + 8;
                  int h = gen.getBaseHeight(bx, bz, Heightmap.Types.WORLD_SURFACE_WG, acc, rs);
                  int surfQ = Math.max(minQ, Math.min(maxQ, (h + 1) >> 2));
                  String surf = biomeName(resolver.getNoiseBiome(bx >> 2, surfQ, bz >> 2)).replace("minecraft:", "");
                  if (remaining.contains(surf) && !found.containsKey(surf)) {
                     found.put(surf, new int[] {cx, cz});
                  } else {
                     // underground-only biomes: accept presence below the surface
                     for (int qy = minQ; qy < surfQ - 2; qy += 3) {
                        String b = biomeName(resolver.getNoiseBiome(bx >> 2, qy, bz >> 2)).replace("minecraft:", "");
                        if (remaining.contains(b) && !found.containsKey(b) && (b.endsWith("caves") || b.equals("deep_dark"))) {
                           found.put(b, new int[] {cx, cz});
                        }
                     }
                  }
                  if (found.keySet().containsAll(remaining)) break search;
               }
            }
         }
         for (Map.Entry<String, int[]> e : found.entrySet()) {
            runChunk(spec, seed, rs, source, settings, gen, e.getKey(), e.getValue()[0], e.getValue()[1]);
            remaining.remove(e.getKey());
         }
      }
      if (!remaining.isEmpty()) System.err.println("NOT FOUND " + spec.settings + ": " + remaining);
      // keep at least one chunk when no explicit target was reachable
   }

   static void runChunk(Spec spec, long seed, RandomState rs, BiomeSource source, NoiseGeneratorSettings settings,
                        NoiseBasedChunkGenerator gen, String label, int cx, int cz) throws Exception {
      ChunkPos pos = new ChunkPos(cx, cz);
      LevelHeightAccessor acc = LevelHeightAccessor.create(spec.minY, spec.height);
      PalettedContainerFactory factory = PalettedContainerFactory.create(full);
      ProtoChunk chunk = new ProtoChunk(pos, UpgradeData.EMPTY, acc, factory, null);
      BiomeResolver resolver = source.createUncachedResolver(rs);
      BiomeManager biomeManager = new BiomeManager(resolver, BiomeManager.obfuscateSeed(seed));
      int minQ = spec.minY >> 2, maxQ = minQ + (spec.height >> 2) - 1;

      out.println("CASE " + spec.settings + " " + seed + " " + cx + " " + cz + " " + spec.minY + " " + spec.height + " " + label);
      // biome table
      Map<String, Integer> palette = new LinkedHashMap<>();
      int qx0 = 4 * cx - 1, qz0 = 4 * cz - 1, sx = 6, sz = 6, sy = maxQ - minQ + 1;
      StringBuilder q = new StringBuilder("Q " + qx0 + " " + minQ + " " + qz0 + " " + sx + " " + sy + " " + sz);
      int prev = -1, count = 0;
      for (int qy = minQ; qy <= maxQ; qy++) {
         for (int qz = qz0; qz < qz0 + sz; qz++) {
            for (int qx = qx0; qx < qx0 + sx; qx++) {
               String n = biomeName(resolver.getNoiseBiome(qx, qy, qz));
               int idx = palette.computeIfAbsent(n, k -> palette.size());
               if (idx == prev) { count++; } else { if (count > 0) q.append(' ').append(prev).append(':').append(count); prev = idx; count = 1; }
            }
         }
      }
      q.append(' ').append(prev).append(':').append(count);
      // possible biomes: palettes of the 3x3 chunks
      TreeSet<Integer> poss = new TreeSet<>();
      for (int qy = minQ; qy <= maxQ; qy++) {
         for (int qz = 4 * (cz - 1); qz < 4 * (cz + 2); qz++) {
            for (int qx = 4 * (cx - 1); qx < 4 * (cx + 2); qx++) {
               String n = biomeName(resolver.getNoiseBiome(qx, qy, qz));
               poss.add(palette.computeIfAbsent(n, k -> palette.size()));
            }
         }
      }
      StringBuilder pal = new StringBuilder("PAL " + palette.size());
      for (String n : palette.keySet()) pal.append(' ').append(n);
      out.println(pal);
      out.println(q);
      StringBuilder ps = new StringBuilder("POSS " + poss.size());
      for (int i : poss) ps.append(' ').append(i);
      out.println(ps);
      Set<Holder<Biome>> possible = new HashSet<>();
      HolderGetter<Biome> biomes = full.lookupOrThrow(Registries.BIOME);
      List<String> palNames = new ArrayList<>(palette.keySet());
      for (int i : poss) possible.add(biomes.getOrThrow(ResourceKey.create(Registries.BIOME, Identifier.parse(palNames.get(i)))));

      NoiseSettings ns = settings.noiseSettings().clampToHeightAccessor(chunk.getHeightAccessorForGeneration());
      DensityVolume volume = new DensityVolume(16, ns.height(), 16, pos.getMinBlockX(), ns.minY(), pos.getMinBlockZ());
      Method createFluidPicker = NoiseBasedChunkGenerator.class.getDeclaredMethod("createFluidPicker", NoiseGeneratorSettings.class);
      createFluidPicker.setAccessible(true);
      Method doFill = NoiseBasedChunkGenerator.class.getDeclaredMethod("doFill", NoiseChunk.class, ChunkAccess.class);
      doFill.setAccessible(true);
      try (NoiseChunk noiseChunk = new NoiseChunk(rs, null, settings, (Aquifer.FluidPicker) createFluidPicker.invoke(null, settings), Blender.empty(), volume)) {
         doFill.invoke(gen, noiseChunk, chunk);
         dumpChunk(chunk, false);
         BlockState[] before = snapshot(chunk);

         // topMaterial on the doFill chunk
         MaterialRule rule = settings.materialRule().value();
         WorldGenerationContext ctx = new WorldGenerationContext(gen, chunk.getHeightAccessorForGeneration());
         Random r = new Random(seed * 31 + cx * 7919L + cz);
         for (int i = 0; i < 48; i++) {
            int x = pos.getMinBlockX() + r.nextInt(16), z = pos.getMinBlockZ() + r.nextInt(16);
            int h = chunk.getHeight(Heightmap.Types.WORLD_SURFACE_WG, x & 15, z & 15);
            int y = i % 2 == 0 ? spec.minY + r.nextInt(spec.height) : Math.max(spec.minY, Math.min(spec.minY + spec.height - 1, h - 6 + r.nextInt(9)));
            boolean fluid = r.nextBoolean();
            Optional<BlockState> top = rs.surfaceSystem().topMaterial(rule, rs, ctx, biomeManager::getBiome, chunk, noiseChunk.cachingSamplers(), new BlockPos(x, y, z), fluid);
            out.println("T " + x + " " + y + " " + z + " " + fluid + " " + (top.isPresent() ? Block.BLOCK_STATE_REGISTRY.getId(top.get()) : -1));
         }

         rs.surfaceSystem().buildSurface(rs, biomeManager, ctx, chunk, noiseChunk, rule, possible);
         dumpChunk(chunk, true);
         BlockState[] after = snapshot(chunk);
         StringBuilder d = new StringBuilder();
         int n = 0;
         for (int i = 0; i < after.length; i++) {
            if (after[i] != before[i]) {
               int y = spec.minY + i / 256, z = (i / 16) & 15, x = i & 15;
               d.append(' ').append(x).append(',').append(y).append(',').append(z).append(',').append(Block.BLOCK_STATE_REGISTRY.getId(after[i]));
               n++;
            }
         }
         out.println("D " + n + d);
      }
      out.println("END");
   }

   static BlockState[] snapshot(ProtoChunk chunk) {
      int h = chunk.getHeight();
      BlockState[] s = new BlockState[h * 256];
      BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
      for (int i = 0; i < s.length; i++) {
         s[i] = chunk.getBlockState(p.set(chunk.getPos().getMinBlockX() + (i & 15), chunk.getMinY() + i / 256, chunk.getPos().getMinBlockZ() + ((i / 16) & 15)));
      }
      return s;
   }

   static void dumpChunk(ProtoChunk chunk, boolean after) {
      if (!after) {
         for (int si = 0; si < chunk.getSectionsCount(); si++) {
            LevelChunkSection section = chunk.getSection(si);
            if (section.hasOnlyAir()) continue;
            StringBuilder sb = new StringBuilder("S " + si);
            int prev = -1, count = 0;
            for (int y = 0; y < 16; y++) for (int z = 0; z < 16; z++) for (int x = 0; x < 16; x++) {
               int id = Block.BLOCK_STATE_REGISTRY.getId(section.getBlockState(x, y, z));
               if (id == prev) count++; else { if (count > 0) sb.append(' ').append(prev).append(':').append(count); prev = id; count = 1; }
            }
            sb.append(' ').append(prev).append(':').append(count);
            out.println(sb);
         }
      }
      String tag = after ? "A" : "B";
      var pp = chunk.getPostProcessing();
      for (int si = 0; si < pp.length; si++) {
         if (pp[si] == null || pp[si].isEmpty()) continue;
         StringBuilder sb = new StringBuilder("P" + tag + " " + si);
         for (short v : pp[si]) sb.append(' ').append(v);
         out.println(sb);
      }
      for (Heightmap.Types t : new Heightmap.Types[] {Heightmap.Types.WORLD_SURFACE_WG, Heightmap.Types.OCEAN_FLOOR_WG}) {
         StringBuilder sb = new StringBuilder("H" + tag + " " + (t == Heightmap.Types.WORLD_SURFACE_WG ? "ws" : "ofl") + " 256");
         for (int z = 0; z < 16; z++) for (int x = 0; x < 16; x++) sb.append(' ').append(chunk.getHeight(t, x, z));
         out.println(sb);
      }
   }
}
