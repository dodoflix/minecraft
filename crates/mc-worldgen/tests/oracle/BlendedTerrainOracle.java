import java.io.*;
import java.lang.reflect.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.*;
import net.minecraft.resources.*;
import net.minecraft.server.level.WorldGenRegion;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.blending.*;
import net.minecraft.world.level.levelgen.synth.NormalNoise;

/**
 * Blended terrain: createBiomes / doFill / buildTerrain of the real NoiseBasedChunkGenerator with a non-empty Blender built by
 * Blender.of(region) over hand-built old chunks (and below-zero-retrogen targets).
 * Usage: MC_EXTRA_CP=tools/extractor/build scripts/vanilla-java.sh -o crates/mc-worldgen/tests/fixtures/blended_terrain_golden.txt crates/mc-worldgen/tests/oracle/BlendedTerrainOracle.java crates/mc-worldgen/tests/fixtures/blended_terrain_golden.txt
 * Output: crates/mc-worldgen/tests/fixtures/blended_terrain_golden.txt
 * Lines: CASE/OLD/RETRO parameter lines (read by tests/golden/blending.rs) and per case
 *   BE key empty | BB key biomeHash | BD key sectionHashes | BF key sectionHashes | BFH key type hash | BFP key hash | BR key sectionHashes
 */
public class BlendedTerrainOracle {
   static long mix64(long z) {
      z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
      z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
      return z ^ (z >>> 31);
   }
   static long h(long seed, long a, long b, long c) {
      return mix64(seed * 0x9E3779B97F4A7C15L ^ mix64(a * 0xD6E8FEB86659FD93L ^ mix64(b * 0xCA5A826395121157L ^ mix64(c + 0x1234567L))));
   }
   static int hmod(long v, int m) { return (int)((v >>> 1) % m); }

   static final String[] BIOME_NAMES = {"plains", "forest", "desert", "lush_caves", "dripstone_caves", "deep_dark", "ocean", "river"};
   static final List<Holder<Biome>> BIOMES = new ArrayList<>();
   static PalettedContainerFactory FACTORY;
   static Block[] SURF;

   static Holder<Biome> biomeFn(long seed, int qx, int qy, int qz) {
      return BIOMES.get(hmod(h(seed, qx, qy, qz), 8));
   }

   static BlockState terrain(long seed, int wx, int y, int wz) {
      long hc = h(seed, wx, wz, 77);
      int top = 50 + hmod(hc >>> 3, 40);
      int kind = hmod(hc >>> 11, 16);
      Block surface = kind == 1 ? Blocks.DEEPSLATE : SURF[hmod(hc >>> 19, SURF.length)];
      long r = h(seed, wx, y, wz);
      if (y < top - 4) {
         if (hmod(r, 13) == 0) return Blocks.AIR.defaultBlockState();
         return (hmod(r >>> 7, 7) == 0 ? Blocks.DEEPSLATE : Blocks.STONE).defaultBlockState();
      }
      if (y < top) return (y == top - 1 ? surface : (kind == 1 ? Blocks.DEEPSLATE : Blocks.DIRT)).defaultBlockState();
      if (kind == 0 && y <= 62) return Blocks.WATER.defaultBlockState();
      if (y < top + 6) {
         return switch (hmod(r, 14)) {
            case 0 -> Blocks.OAK_LEAVES.defaultBlockState();
            case 1 -> Blocks.OAK_LOG.defaultBlockState();
            case 2 -> Blocks.BROWN_MUSHROOM_BLOCK.defaultBlockState();
            case 3 -> Blocks.TORCH.defaultBlockState();
            case 4 -> Blocks.COBWEB.defaultBlockState();
            case 5 -> Blocks.RED_MUSHROOM_BLOCK.defaultBlockState();
            case 6 -> Blocks.SPRUCE_LEAVES.defaultBlockState();
            case 7 -> Blocks.WATER.defaultBlockState();
            case 8 -> Blocks.LAVA.defaultBlockState();
            case 9 -> Blocks.GLASS.defaultBlockState();
            case 10 -> Blocks.OAK_FENCE.defaultBlockState();
            case 11 -> Blocks.SNOW.defaultBlockState();
            default -> Blocks.AIR.defaultBlockState();
         };
      }
      return Blocks.AIR.defaultBlockState();
   }

   static final int[] SPARSE = {0, 4, 8, 12, 15};
   static boolean contains(int[] a, int v) { for (int x : a) if (x == v) return true; return false; }

   record Old(int x, int z, int minSec, int maxSec, ChunkStatus status, boolean primed, float[] heights) {}

   static ProtoChunk makeOld(int cx, int cz, long seed, LevelHeightAccessor lh, Old o, boolean fillBlocks, ChunkStatus statusOverride) {
      BlendingData blending = BlendingData.unpack(new BlendingData.Packed(o.minSec, o.maxSec, Optional.ofNullable(o.heights)));
      ProtoChunk chunk = new ProtoChunk(new ChunkPos(cx, cz), UpgradeData.EMPTY, lh, FACTORY, blending);
      BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
      if (fillBlocks) {
         for (int x = 0; x < 16; x++) for (int z = 0; z < 16; z++) {
            if (!(contains(SPARSE, x) && contains(SPARSE, z))) continue;
            for (int y = lh.getMinY(); y < lh.getMinY() + lh.getHeight(); y++) {
               BlockState state = terrain(seed, cx * 16 + x, y, cz * 16 + z);
               if (!state.isAir()) chunk.setBlockState(pos.set(x, y, z), state, 0);
            }
         }
      }
      chunk.fillBiomesFromNoise((qx, qy, qz) -> biomeFn(seed ^ 0xB10, qx, qy, qz));
      if (o.primed) Heightmap.primeHeightmaps(chunk, EnumSet.of(Heightmap.Types.WORLD_SURFACE_WG));
      chunk.setPersistedStatus(statusOverride != null ? statusOverride : o.status);
      return chunk;
   }

   static class FakeRegion extends WorldGenRegion {
      FakeRegion() { super(null, null, null, null); }
      ChunkPos center;
      long seed;
      LevelHeightAccessor lh;
      Map<Long, ProtoChunk> chunks = new HashMap<>();
      @Override public ChunkPos getCenter() { return center; }
      @Override public boolean isOldChunkAround(ChunkPos pos, int range) {
         for (ProtoChunk c : chunks.values())
            if (c.getBlendingData() != null && Math.max(Math.abs(c.getPos().x() - pos.x()), Math.abs(c.getPos().z() - pos.z())) <= range) return true;
         return false;
      }
      @Override public ChunkAccess getChunk(int x, int z) {
         return chunks.computeIfAbsent(ChunkPos.pack(x, z), k -> new ProtoChunk(new ChunkPos(x, z), UpgradeData.EMPTY, lh, FACTORY, null));
      }
   }

   static FakeRegion allocRegion() throws Exception {
      Field f = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
      f.setAccessible(true);
      FakeRegion region = (FakeRegion)((sun.misc.Unsafe)f.get(null)).allocateInstance(FakeRegion.class);
      region.chunks = new HashMap<>();
      return region;
   }

   record Case(int id, String setting, long seed, int cx, int cz, int kind, int layout, List<Old> olds, long[] bits, String target) {}

   static final long FNV_INIT = 0xcbf29ce484222325L;
   static long fnv(long h, int word) {
      for (int i = 0; i < 4; i++) { h ^= (word >>> (8 * i)) & 0xFF; h *= 0x100000001b3L; }
      return h;
   }
   static long fnv(long h, long word) { return fnv(fnv(h, (int)word), (int)(word >>> 32)); }
   static String hex(long h) { return String.format(java.util.Locale.ROOT, "%016x", h); }

   static String sectionHashes(ChunkAccess chunk) {
      StringBuilder sb = new StringBuilder();
      int minY = chunk.getMinY();
      BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
      for (int s = 0; s < chunk.getSectionsCount(); s++) {
         long hh = FNV_INIT;
         for (int y = 0; y < 16; y++) for (int z = 0; z < 16; z++) for (int x = 0; x < 16; x++) {
            p.set(chunk.getPos().getMinBlockX() + x, minY + s * 16 + y, chunk.getPos().getMinBlockZ() + z);
            hh = fnv(hh, Block.BLOCK_STATE_REGISTRY.getId(chunk.getBlockState(p)));
         }
         sb.append(' ').append(hex(hh));
      }
      return sb.toString();
   }

   static long biomeHash(ChunkAccess chunk) {
      long hh = FNV_INIT;
      int qMinY = chunk.getMinY() >> 2;
      int qMaxY = qMinY + (chunk.getHeight() >> 2);
      for (int qy = qMinY; qy < qMaxY; qy++) for (int qz = 0; qz < 4; qz++) for (int qx = 0; qx < 4; qx++) {
         Holder<Biome> b = chunk.getNoiseBiome(chunk.getPos().getMinBlockX() / 4 + qx, qy, chunk.getPos().getMinBlockZ() / 4 + qz);
         for (byte by : (b.unwrapKey().get().identifier().toString() + "\n").getBytes(java.nio.charset.StandardCharsets.UTF_8)) { hh ^= by & 0xFF; hh *= 0x100000001b3L; }
      }
      return hh;
   }

   static void printHeightmaps(PrintWriter out, String key, ChunkAccess chunk) {
      for (Heightmap.Types t : Heightmap.Types.values()) {
         if (chunk.hasPrimedHeightmap(t)) {
            long hh = FNV_INIT;
            for (long l : chunk.getOrCreateHeightmapUnprimed(t).getRawData()) hh = fnv(hh, l);
            out.println("BFH " + key + " " + t.name() + " " + hex(hh));
         }
      }
   }

   static long postHash(ChunkAccess chunk) {
      long hh = FNV_INIT;
      it.unimi.dsi.fastutil.shorts.ShortList[] lists = chunk.getPostProcessing();
      for (int i = 0; i < lists.length; i++) {
         hh = fnv(hh, -1 - i);
         if (lists[i] != null) for (short s : lists[i]) hh = fnv(hh, (int)s);
      }
      return hh;
   }

   /** Fresh region with the case's old chunks and the target chunk (kind 0 new, 1 old, 2 below-zero retrogen). */
   static FakeRegion build(Case c, LevelHeightAccessor lh, ChunkStatus targetStatus) throws Exception {
      FakeRegion region = allocRegion();
      region.center = new ChunkPos(c.cx, c.cz);
      region.seed = c.seed;
      region.lh = lh;
      for (Old o : c.olds) {
         if (o.x == c.cx && o.z == c.cz) continue;
         region.chunks.put(ChunkPos.pack(o.x, o.z), makeOld(o.x, o.z, c.seed, lh, o, true, null));
      }
      ProtoChunk target;
      if (c.kind == 1) {
         Old o = c.olds.stream().filter(x -> x.x == c.cx && x.z == c.cz).findFirst().get();
         target = makeOld(c.cx, c.cz, c.seed, lh, o, false, targetStatus);
      } else {
         target = new ProtoChunk(new ChunkPos(c.cx, c.cz), UpgradeData.EMPTY, lh, FACTORY, null);
         if (c.kind == 2) {
            BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
            for (int x = 0; x < 16; x++) for (int z = 0; z < 16; z++) for (int y = 0; y < 8; y++)
               if (hmod(h(c.seed, c.cx * 16 + x, y, c.cz * 16 + z), 3) != 0) target.setBlockState(pos.set(x, y, z), Blocks.BEDROCK.defaultBlockState(), 0);
            // the section holding y = 0 keeps its (retained) biomes
            target.getSection(target.getSectionIndexFromSectionY(0)).fillBiomesFromNoise((qx, qy, qz) -> biomeFn(c.seed ^ 12, qx, qy, qz), c.cx * 4, 0, c.cz * 4);
            CompoundTag tag = new CompoundTag();
            tag.putString("target_status", c.target);
            if (c.bits != null) tag.put("missing_bedrock", new LongArrayTag(c.bits));
            target.setBelowZeroRetrogen(BelowZeroRetrogen.CODEC.parse(NbtOps.INSTANCE, tag).getOrThrow());
         }
         target.setPersistedStatus(targetStatus);
      }
      region.chunks.put(ChunkPos.pack(c.cx, c.cz), target);
      return region;
   }

   public static void main(String[] args) throws Exception {
      var access = extractor.Setup.bootstrap();
      SURF = new Block[]{Blocks.GRASS_BLOCK, Blocks.SAND, Blocks.RED_SAND, Blocks.SNOW_BLOCK, Blocks.MYCELIUM, Blocks.PODZOL, Blocks.TERRACOTTA, Blocks.COARSE_DIRT, Blocks.STONE, Blocks.GRAVEL};
      HolderGetter<NormalNoise> noises = access.lookupOrThrow(Registries.NOISE);
      var settingsLookup = access.lookupOrThrow(Registries.NOISE_SETTINGS);
      var biomeGetter = access.lookupOrThrow(Registries.BIOME);
      var presetLookup = access.lookupOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST);
      FACTORY = PalettedContainerFactory.create(access);
      for (String n : BIOME_NAMES) BIOMES.add(biomeGetter.getOrThrow(ResourceKey.create(Registries.BIOME, Identifier.withDefaultNamespace(n))));
      PrintWriter out = new PrintWriter(new FileWriter(args[0]));

      StructureManager sm = new StructureManager(null, null, null) {
         @Override public List<net.minecraft.world.level.levelgen.structure.StructureStart> startsForStructure(int sx, int sz, java.util.function.Predicate<net.minecraft.world.level.levelgen.structure.Structure> m) { return List.of(); }
         @Override public StructureManager forWorldGenRegion(WorldGenRegion region) { return this; }
      };
      Method createNoiseChunk = NoiseBasedChunkGenerator.class.getDeclaredMethod("createNoiseChunk", ChunkAccess.class, StructureManager.class, Blender.class, RandomState.class, NoiseSettings.class);
      createNoiseChunk.setAccessible(true);
      Method doFill = NoiseBasedChunkGenerator.class.getDeclaredMethod("doFill", NoiseChunk.class, ChunkAccess.class);
      doFill.setAccessible(true);

      String[] settingNames = {"overworld", "overworld", "overworld", "overworld", "overworld", "overworld", "overworld", "overworld", "amplified", "nether", "overworld", "overworld"};
      int[][] chunks = {{3, 7}, {-15, 22}, {0, 0}, {40, -33}, {-1, -1}, {5, -5}, {100, 100}, {-200, 150}, {17, -9}, {-7, -90}, {8, 8}, {-64, 64}};
      int[] kinds = {0, 0, 0, 0, 1, 1, 2, 2, 0, 0, 2, 0};
      long[] seeds = {0L, 12345L, -4172144997902289642L};
      Random rnd = new Random(0xB1E4DEDL);
      List<Case> cases = new ArrayList<>();
      for (int id = 0; id < settingNames.length; id++) {
         int cx = chunks[id][0], cz = chunks[id][1];
         int layout = id % 8;
         List<Old> olds = new ArrayList<>();
         double density = 0.2 + rnd.nextDouble() * 0.6;
         for (int dx = -9; dx <= 9; dx++) for (int dz = -9; dz <= 9; dz++) {
            boolean old = switch (layout) {
               case 0 -> dx == -1 && dz == 0;
               case 1 -> Math.max(Math.abs(dx), Math.abs(dz)) == 1;
               case 2 -> rnd.nextDouble() < density;
               case 3 -> dx == 7 && dz == -3 || dx == -5 && dz == 5 || dx == 2 && dz == 0;
               case 4 -> dx < 0 || dx == 0 && dz == 0;
               case 5 -> dx == 1 || dx == 0 && dz == 0;
               case 6 -> Math.abs(dx) + Math.abs(dz) <= 3 && rnd.nextInt(3) != 0 || dx == 0 && dz == 0;
               default -> dz >= 0 && dx > -3 && dx < 3 || dx == 0 && dz == 0;
            };
            if (kinds[id] == 1 && dx == 0 && dz == 0) old = true;
            if (kinds[id] != 1 && dx == 0 && dz == 0) old = false;
            if (old) {
               int[] secs = rnd.nextInt(3) == 0 ? new int[]{-4, 20} : (rnd.nextInt(4) == 0 ? new int[]{-2, 12} : new int[]{0, 16});
               int hmode = rnd.nextInt(4);
               float[] heights = null;
               if (hmode != 0) {
                  heights = new float[16];
                  for (int i = 0; i < 16; i++) heights[i] = hmode == 2 && rnd.nextInt(3) == 0 ? Float.MAX_VALUE : (hmode == 3 ? rnd.nextInt(200) - 50 : (float)(30 + rnd.nextDouble() * 90));
               }
               ChunkStatus st = rnd.nextInt(6) == 0 ? ChunkStatus.STRUCTURE_STARTS : ChunkStatus.BIOMES;
               olds.add(new Old(cx + dx, cz + dz, secs[0], secs[1], st, rnd.nextBoolean(), heights));
            }
         }
         long[] bits = null;
         if (kinds[id] == 2 && id != 6) { bits = new long[4]; for (int i = 0; i < 4; i++) bits[i] = h(id, i, 9, 9) & h(id, i, 9, 10); }
         cases.add(new Case(id, settingNames[id], seeds[id % 3], cx, cz, kinds[id], layout, olds, bits, id % 2 == 0 ? "minecraft:full" : "minecraft:features"));
      }

      for (Case c : cases) {
         out.println("CASE " + c.id + " " + c.setting + " " + c.seed + " " + c.cx + " " + c.cz + " " + c.kind + " " + c.olds.size());
         for (Old o : c.olds) {
            StringBuilder sb = new StringBuilder();
            if (o.heights == null) sb.append('-');
            else for (int i = 0; i < 16; i++) { if (i > 0) sb.append(','); sb.append(Integer.toHexString(Float.floatToRawIntBits(o.heights[i]))); }
            out.println("OLD " + o.x + " " + o.z + " " + o.minSec + " " + o.maxSec + " " + o.status.getName() + " " + (o.primed ? 1 : 0) + " " + sb);
         }
         out.println("RETRO " + c.id + " " + c.target + " " + (c.bits == null ? "-" : Arrays.toString(c.bits).replace(" ", "")));

         Holder<NoiseGeneratorSettings> holder = settingsLookup.getOrThrow(ResourceKey.create(Registries.NOISE_SETTINGS, Identifier.withDefaultNamespace(c.setting)));
         NoiseGeneratorSettings settings = holder.value();
         boolean tall = !c.setting.equals("nether");
         LevelHeightAccessor lh = tall ? LevelHeightAccessor.create(-64, 384) : LevelHeightAccessor.create(0, 256);
         BiomeSource source = MultiNoiseBiomeSource.createFromPreset(presetLookup.getOrThrow(c.setting.equals("nether") ? MultiNoiseBiomeSourceParameterLists.NETHER : MultiNoiseBiomeSourceParameterLists.OVERWORLD));
         NoiseBasedChunkGenerator gen = new NoiseBasedChunkGenerator(source, holder);
         RandomState rs = RandomState.create(noises, c.seed, settings);
         BiomeResolver uncached = source.createUncachedResolver(rs);
         BiomeManager biomeManager = new BiomeManager(uncached::getNoiseBiome, BiomeManager.obfuscateSeed(c.seed));
         String key = c.id + "";

         // createBiomes (target not generated far enough yet: only the carving filter centre uses its blending data)
         FakeRegion r1 = build(c, lh, ChunkStatus.EMPTY);
         Blender b1 = Blender.of(r1);
         ProtoChunk t1 = r1.chunks.get(ChunkPos.pack(c.cx, c.cz));
         gen.createBiomes(rs, b1, sm, t1).join();
         out.println("BE " + key + " " + (b1.isEmpty() ? 1 : 0));
         t1.setPersistedStatus(ChunkStatus.BIOMES);
         out.println("BB " + key + " " + hex(biomeHash(t1)));

         // doFill alone
         FakeRegion r2 = build(c, lh, ChunkStatus.BIOMES);
         Blender b2 = Blender.of(r2);
         ProtoChunk t2 = r2.chunks.get(ChunkPos.pack(c.cx, c.cz));
         NoiseSettings ns = settings.noiseSettings().clampToHeightAccessor(t2.getHeightAccessorForGeneration());
         try (NoiseChunk nc = (NoiseChunk)createNoiseChunk.invoke(gen, t2, sm, b2, rs, ns)) {
            doFill.invoke(gen, nc, t2);
         }
         out.println("BD " + key + sectionHashes(t2));

         // full buildTerrain (+ the retrogen post steps of ChunkStatusTasks.buildTerrain)
         FakeRegion r3 = build(c, lh, ChunkStatus.BIOMES);
         Blender b3 = Blender.of(r3);
         ProtoChunk t3 = r3.chunks.get(ChunkPos.pack(c.cx, c.cz));
         gen.buildTerrain(t3, b3, rs, sm, biomeManager, null, new HashSet<>(source.possibleBiomes())).join();
         out.println("BF " + key + sectionHashes(t3));
         printHeightmaps(out, key, t3);
         out.println("BFP " + key + " " + hex(postHash(t3)));
         BelowZeroRetrogen retrogen = t3.getBelowZeroRetrogen();
         if (retrogen != null) {
            BelowZeroRetrogen.replaceOldBedrock(t3);
            if (retrogen.hasBedrockHoles()) retrogen.applyBedrockMask(t3);
            out.println("BR " + key + sectionHashes(t3));
         }
         out.flush();
      }
      out.println("END");
      out.close();
   }
}
