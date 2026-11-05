import java.io.*;
import java.lang.reflect.*;
import java.util.*;
import it.unimi.dsi.fastutil.shorts.ShortList;
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
import net.minecraft.world.level.levelgen.densityfunction.*;

/**
 * Differential oracle for crates/mc-worldgen::blending: the real Blender / BlendingData / BelowZeroRetrogen on hand-built old-chunk
 * scenarios. Usage (repo root): scripts/vanilla-java.sh crates/mc-worldgen/tests/oracle/BlendingOracle.java out.txt
 * The output holds the scenario parameters (SCENARIO/OLD lines, read by tests/golden/blending.rs) followed by digests (FNV-1a 64 over the
 * little-endian ints / float bits) of every dumped value stream: `S <id> <what> <digest> <count>`.
 */
public class BlendingOracle {
    // ---- shared deterministic helpers (ported verbatim in tests/golden/blending.rs) ----
    static long mix64(long z) {
        z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
        z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
        return z ^ (z >>> 31);
    }
    static long h(long seed, long a, long b, long c) {
        return mix64(seed * 0x9E3779B97F4A7C15L ^ mix64(a * 0xD6E8FEB86659FD93L ^ mix64(b * 0xCA5A826395121157L ^ mix64(c + 0x1234567L))));
    }
    static int hmod(long v, int m) { return (int) ((v >>> 1) % m); }

    static final class Digest {
        long v = 0xcbf29ce484222325L;
        long count = 0;
        void b(int x) { v ^= (x & 0xFF); v *= 0x100000001b3L; }
        void i(int x) { b(x); b(x >>> 8); b(x >>> 16); b(x >>> 24); count++; }
        void f(float x) { i(Float.floatToRawIntBits(x)); }
    }

    // ---- registries / chunk factory ----
    static final String[] BIOME_NAMES = {"plains", "forest", "desert", "lush_caves", "dripstone_caves", "deep_dark", "ocean", "river"};
    static final List<Holder<Biome>> BIOMES = new ArrayList<>();
    static PalettedContainerFactory FACTORY;

    static void setup() throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        var info = new net.minecraft.server.packs.PackLocationInfo("gen", net.minecraft.network.chat.Component.literal("gen"), net.minecraft.server.packs.repository.PackSource.BUILT_IN, Optional.empty());
        var rm = new net.minecraft.server.packs.resources.MultiPackResourceManager(net.minecraft.server.packs.PackType.SERVER_DATA,
            List.<net.minecraft.server.packs.PackResources>of(new net.minecraft.server.packs.PathPackResources(info, java.nio.file.Path.of("ref/generated"))));
        net.minecraft.tags.TagLoader.loadTagsForExistingRegistries(rm, net.minecraft.core.RegistryAccess.fromRegistryOfRegistries(net.minecraft.core.registries.BuiltInRegistries.REGISTRY)).forEach(net.minecraft.core.Registry.PendingTags::apply);
        initSurf();
        IdMapper<Holder<Biome>> ids = new IdMapper<>();
        HolderOwner<Biome> owner = new HolderOwner<Biome>() {};
        for (String n : BIOME_NAMES) {
            Holder<Biome> holder = Holder.Reference.createStandAlone(owner, ResourceKey.create(Registries.BIOME, Identifier.withDefaultNamespace(n)));
            BIOMES.add(holder);
            ids.add(holder);
        }
        var blocks = Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY);
        var biomes = Strategy.createForBiomes(ids);
        FACTORY = new PalettedContainerFactory(blocks, Blocks.AIR.defaultBlockState(), null, biomes, BIOMES.get(0), null);
    }

    static Holder<Biome> biomeFn(long seed, int qx, int qy, int qz) {
        return BIOMES.get(hmod(h(seed, qx, qy, qz), 8));
    }
    static int biomeId(Holder<Biome> b) { return BIOMES.indexOf(b); }

    // ---- terrain ----
    static Block[] SURF;
    static void initSurf() { SURF = new Block[]{Blocks.GRASS_BLOCK, Blocks.SAND, Blocks.RED_SAND, Blocks.SNOW_BLOCK, Blocks.MYCELIUM, Blocks.PODZOL, Blocks.TERRACOTTA, Blocks.COARSE_DIRT, Blocks.STONE, Blocks.GRAVEL}; }

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

    static ProtoChunk makeChunk(int cx, int cz, long seed, int minY, int height, boolean old, int minSec, int maxSec, float[] heights,
                                ChunkStatus status, boolean primed, boolean full) {
        BlendingData blending = old ? BlendingData.unpack(new BlendingData.Packed(minSec, maxSec, Optional.ofNullable(heights))) : null;
        ProtoChunk chunk = new ProtoChunk(new ChunkPos(cx, cz), UpgradeData.EMPTY, LevelHeightAccessor.create(minY, height), FACTORY, blending);
        if (old) {
            BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
            for (int x = 0; x < 16; x++) {
                for (int z = 0; z < 16; z++) {
                    boolean sparseCol = contains(SPARSE, x) && contains(SPARSE, z);
                    if (!full && !sparseCol) continue;
                    for (int y = minY; y < minY + height; y++) {
                        BlockState state = terrain(seed, cx * 16 + x, y, cz * 16 + z);
                        if (!state.isAir()) chunk.setBlockState(pos.set(x, y, z), state, 0);
                    }
                }
            }
            chunk.fillBiomesFromNoise((qx, qy, qz) -> biomeFn(seed ^ 0xB10, qx, qy, qz));
            if (primed) Heightmap.primeHeightmaps(chunk, EnumSet.of(Heightmap.Types.WORLD_SURFACE_WG));
            chunk.setPersistedStatus(status);
        }
        return chunk;
    }

    static boolean contains(int[] a, int v) { for (int x : a) if (x == v) return true; return false; }

    // ---- fake region ----
    static class FakeRegion extends WorldGenRegion {
        FakeRegion() { super(null, null, null, null); }
        ChunkPos center;
        long seed;
        int minY, height;
        Map<Long, ProtoChunk> chunks = new HashMap<>();

        @Override public ChunkPos getCenter() { return center; }
        @Override public boolean isOldChunkAround(ChunkPos pos, int range) {
            for (ProtoChunk c : chunks.values()) {
                if (c.getBlendingData() != null && Math.max(Math.abs(c.getPos().x() - pos.x()), Math.abs(c.getPos().z() - pos.z())) <= range) return true;
            }
            return false;
        }
        @Override public ChunkAccess getChunk(int x, int z) {
            return chunks.computeIfAbsent(ChunkPos.pack(x, z), k -> makeChunk(x, z, seed, minY, height, false, 0, 0, null, ChunkStatus.EMPTY, false, false));
        }
    }

    static FakeRegion allocRegion() throws Exception {
        Field f = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        f.setAccessible(true);
        sun.misc.Unsafe unsafe = (sun.misc.Unsafe) f.get(null);
        FakeRegion region = (FakeRegion) unsafe.allocateInstance(FakeRegion.class);
        region.chunks = new HashMap<>();
        return region;
    }

    static void emit(PrintWriter out, int id, String what, Digest d) {
        out.println("S " + id + " " + what + " " + Long.toHexString(d.v) + " " + d.count);
    }

    static Object field(Object o, String name) throws Exception {
        Field f = BlendingData.class.getDeclaredField(name);
        f.setAccessible(true);
        return f.get(o);
    }

    static float[] heightsOf(Random rnd, int mode) {
        if (mode == 0) return null;
        float[] heights = new float[16];
        for (int i = 0; i < 16; i++) {
            if (mode == 2 && rnd.nextInt(3) == 0) heights[i] = Float.MAX_VALUE;
            else heights[i] = mode == 3 ? rnd.nextInt(200) - 50 : (float) (30 + rnd.nextDouble() * 90);
        }
        if (mode == 2) heights[rnd.nextInt(16)] = 64.5f;
        return heights;
    }

    public static void main(String[] args) throws Exception {
        setup();
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        ChunkStatus[] statuses = {ChunkStatus.BIOMES, ChunkStatus.TERRAIN, ChunkStatus.FULL, ChunkStatus.EMPTY, ChunkStatus.STRUCTURE_STARTS, ChunkStatus.FEATURES};
        int scenarios = 16;
        Random rnd = new Random(0xB1E0DL);
        for (int id = 0; id < scenarios; id++) {
            long seed = rnd.nextLong();
            int[][] cfg = {{-64, 384}, {0, 256}, {-64, 384}, {-48, 160}};
            int[] lh = cfg[id % 4];
            int cx = rnd.nextInt(2000) - 1000;
            int cz = rnd.nextInt(2000) - 1000;
            // old chunk layouts: 0 single neighbour, 1 ring r=1, 2 random blob, 3 far chunk only (r=7), 4 half plane, 5 centre only, ...
            int layout = id % 8;
            double density = 0.15 + rnd.nextDouble() * 0.7;
            List<int[]> olds = new ArrayList<>();
            for (int dx = -9; dx <= 9; dx++) {
                for (int dz = -9; dz <= 9; dz++) {
                    boolean old = switch (layout) {
                        case 0 -> dx == -1 && dz == 0;
                        case 1 -> Math.max(Math.abs(dx), Math.abs(dz)) == 1;
                        case 2 -> rnd.nextDouble() < density;
                        case 3 -> dx == 7 && dz == -3 || dx == -5 && dz == 5;
                        case 4 -> dx < 0;
                        case 5 -> dx == 0 && dz == 0;
                        case 7 -> dx == 9 && dz == 2 || dx == -9 && dz == -9;
                        default -> Math.abs(dx) + Math.abs(dz) <= 3 && rnd.nextInt(3) != 0;
                    };
                    if (old) olds.add(new int[]{dx, dz});
                }
            }
            out.println("SCENARIO " + id + " " + seed + " " + lh[0] + " " + lh[1] + " " + cx + " " + cz + " " + olds.size());
            FakeRegion region = allocRegion();
            region.center = new ChunkPos(cx, cz);
            region.seed = seed;
            region.minY = lh[0];
            region.height = lh[1];
            for (int[] o : olds) {
                int ox = cx + o[0], oz = cz + o[1];
                int[] secs = rnd.nextInt(3) == 0 ? new int[]{-4, 20} : (rnd.nextInt(4) == 0 ? new int[]{-2, 12} : new int[]{0, 16});
                int hmode = rnd.nextInt(4);
                float[] heights = heightsOf(rnd, hmode);
                ChunkStatus status = rnd.nextInt(6) == 0 ? statuses[rnd.nextInt(statuses.length)] : ChunkStatus.BIOMES;
                boolean primed = rnd.nextBoolean();
                boolean full = o[0] == 0 && o[1] == 0 || rnd.nextInt(10) == 0;
                StringBuilder sb = new StringBuilder();
                if (heights == null) sb.append('-');
                else for (int i = 0; i < 16; i++) { if (i > 0) sb.append(','); sb.append(Integer.toHexString(Float.floatToRawIntBits(heights[i]))); }
                out.println("OLD " + ox + " " + oz + " " + secs[0] + " " + secs[1] + " " + status.getName() + " " + (primed ? 1 : 0) + " " + (full ? 1 : 0) + " " + sb);
                region.chunks.put(ChunkPos.pack(ox, oz), makeChunk(ox, oz, seed, lh[0], lh[1], true, secs[0], secs[1], heights, status, primed, full));
            }

            Blender blender = Blender.of(region);
            out.println("S " + id + " empty " + (blender.isEmpty() ? 1 : 0) + " 0");

            // calculateData results of every old chunk (heights as persisted + transient densities / biomes)
            Digest dh = new Digest(), dd = new Digest(), db = new Digest();
            for (int[] o : olds) {
                ProtoChunk c = region.chunks.get(ChunkPos.pack(cx + o[0], cz + o[1]));
                BlendingData data = c.getBlendingData();
                BlendingData.Packed p = data.pack();
                dh.i(p.minSection()); dh.i(p.maxSection());
                if (p.heights().isPresent()) { dh.i(1); for (float f : p.heights().get()) dh.f(f); } else dh.i(0);
                float[][] dens = (float[][]) field(data, "densities");
                for (float[] col : dens) {
                    if (col == null) { dd.i(-1); continue; }
                    dd.i(col.length);
                    for (float f : col) dd.f(f);
                }
                @SuppressWarnings("unchecked")
                List<List<Holder<Biome>>> bios = (List<List<Holder<Biome>>>) field(data, "biomes");
                for (List<Holder<Biome>> col : bios) {
                    if (col == null) { db.i(-1); continue; }
                    db.i(col.size());
                    for (Holder<Biome> b : col) db.i(b == null ? -2 : biomeId(b));
                }
            }
            emit(out, id, "heights", dh);
            emit(out, id, "densities", dd);
            emit(out, id, "biomes", db);

            // blendOffsetAndFactor per block
            Method boaf = Blender.class.getDeclaredMethod("blendOffsetAndFactor", int.class, int.class);
            boaf.setAccessible(true);
            Digest dof = new Digest();
            for (int a = 0; a < 48; a++) {
                for (int b = 0; b < 48; b++) {
                    int bx = cx * 16 - 64 + a * 3, bz = cz * 16 - 64 + b * 3;
                    Object o = boaf.invoke(blender, bx, bz);
                    Method alpha = o.getClass().getDeclaredMethod("alpha"), off = o.getClass().getDeclaredMethod("blendingOffset");
                    alpha.setAccessible(true); off.setAccessible(true);
                    dof.f((Float) alpha.invoke(o)); dof.f((Float) off.invoke(o));
                }
            }
            emit(out, id, "offsets", dof);

            // volumes + output buffer + samplers
            DensityVolume[] volumes = {
                new DensityVolume(5, 3, 5, cx * 16, -64, cz * 16, 4, 8, 4),
                new DensityVolume(17, 2, 17, cx * 16 - 8, 0, cz * 16 - 8, 1, 1, 1),
                new DensityVolume(9, 2, 9, cx * 16 - 37, 10, cz * 16 + 11, 2, 3, 2),
                new DensityVolume(3, 1, 4, cx * 16 + 5, 70, cz * 16 - 3, 7, 1, 5)
            };
            Digest dv = new Digest();
            SamplerContext ctx = SamplerContext.EMPTY_UNCACHED;
            for (DensityVolume v : volumes) {
                Blender.OutputBuffer ob = blender.blendOffsetAndFactor(v);
                dv.i(ob.minCellX()); dv.i(ob.minCellZ()); dv.i(ob.cellCountX()); dv.i(ob.cellCountZ());
                for (float f : ob.alphas()) dv.f(f);
                for (float f : ob.offsets()) dv.f(f);
                DensitySampler as = ob.createAlphaSampler(), os = ob.createOffsetSampler();
                for (int bx = v.minBlockX() - 24; bx <= v.maxBlockX() + 24; bx += 5) {
                    for (int bz = v.minBlockZ() - 24; bz <= v.maxBlockZ() + 24; bz += 5) {
                        dv.f(ob.getAlpha(bx, bz)); dv.f(ob.getOffset(bx, bz));
                        dv.f(as.sampleValue(ctx, bx, 5, bz)); dv.f(os.sampleValue(ctx, bx, 5, bz));
                    }
                }
            }
            emit(out, id, "volumes", dv);

            // blendDensity
            Digest dbd = new Digest();
            for (int i = 0; i < 6000; i++) {
                int x = cx * 16 - 48 + hmod(h(seed, i, 2, 0), 128);
                int y = -70 + hmod(h(seed, i, 2, 1), 400);
                int z = cz * 16 - 48 + hmod(h(seed, i, 2, 2), 128);
                if (i % 3 == 0) { x &= ~3; z &= ~3; y &= ~7; }
                float noise = (hmod(h(seed, i, 2, 3), 4000) - 2000) / 1000f;
                dbd.f(blender.blendDensity(x, y, z, noise));
            }
            emit(out, id, "density", dbd);

            // biome resolver
            Digest dbr = new Digest();
            BiomeResolver resolver = blender.getBiomeResolver((qx, qy, qz) -> biomeFn(seed ^ 5, qx, qy, qz));
            for (int qx = cx * 4 - 16; qx < cx * 4 + 20; qx++) {
                for (int qz = cz * 4 - 16; qz < cz * 4 + 20; qz++) {
                    for (int qy = -16; qy < 80; qy++) dbr.i(biomeId(resolver.getNoiseBiome(qx, qy, qz)));
                }
            }
            emit(out, id, "biome_resolver", dbr);

            // carving filter
            Digest dc = new Digest();
            CarvingMask.Filter filter = blender.getCarvingFilter();
            int trues = 0;
            if (filter == null) dc.i(-1);
            else for (int i = 0; i < 20000; i++) {
                int x = hmod(h(seed, i, 3, 0), 64) - 24; // chunk-local coordinates, like the carvers pass them
                int y = lh[0] - 8 + hmod(h(seed, i, 3, 1), lh[1] + 16);
                int z = hmod(h(seed, i, 3, 2), 64) - 24;
                boolean t = filter.test(x, y, z);
                if (t) trues++;
                dc.i(t ? 1 : 0);
            }
            emit(out, id, "carving", dc);
            out.println("S " + id + " carving_true " + trues + " 0");

            // generateBorderTicks on the centre chunk (made old + fully filled in some scenarios)
            Digest dbt = new Digest();
            ProtoChunk centre = (ProtoChunk) region.getChunk(cx, cz);
            Blender.generateBorderTicks(region, centre);
            ShortList[] post = centre.getPostProcessing();
            for (ShortList l : post) {
                if (l == null) { dbt.i(-1); continue; }
                dbt.i(l.size());
                for (short s : l) dbt.i(s);
            }
            emit(out, id, "border", dbt);
        }

        // ---- BelowZeroRetrogen ----
        for (int id = 0; id < 6; id++) {
            long seed = rnd.nextLong();
            int cx = rnd.nextInt(200) - 100, cz = rnd.nextInt(200) - 100;
            ProtoChunk chunk = new ProtoChunk(new ChunkPos(cx, cz), UpgradeData.EMPTY, LevelHeightAccessor.create(-64, 384), FACTORY, null);
            Block[] pool = {Blocks.BEDROCK, Blocks.BEDROCK, Blocks.BEDROCK, Blocks.STONE, Blocks.DEEPSLATE, Blocks.AIR, Blocks.WATER, Blocks.DIRT};
            BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
            for (int x = 0; x < 16; x++) for (int z = 0; z < 16; z++) for (int y = -64; y < 12; y++) {
                Block b = pool[hmod(h(seed, x + cx * 16, y, z + cz * 16), pool.length)];
                if (b != Blocks.AIR) chunk.setBlockState(pos.set(x, y, z), b.defaultBlockState(), 0);
            }
            chunk.fillBiomesFromNoise((qx, qy, qz) -> biomeFn(seed ^ 11, qx, qy, qz));
            // section 0 biomes (the resolver reads y = 0)
            chunk.getSection(chunk.getSectionIndexFromSectionY(0)).fillBiomesFromNoise((qx, qy, qz) -> biomeFn(seed ^ 12, qx, qy, qz), cx * 4, 0, cz * 4);
            CompoundTag tag = new CompoundTag();
            tag.putString("target_status", id % 2 == 0 ? "minecraft:full" : "minecraft:features");
            if (id != 3) {
                long[] bits = new long[4];
                for (int i = 0; i < 4; i++) bits[i] = id == 4 ? 0 : (h(seed, i, 9, 9) & h(seed, i, 9, 10));
                if (id == 5) bits[3] = 0;
                tag.put("missing_bedrock", new LongArrayTag(bits));
            }
            BelowZeroRetrogen retrogen = BelowZeroRetrogen.CODEC.parse(NbtOps.INSTANCE, tag).getOrThrow();
            chunk.setBelowZeroRetrogen(retrogen);
            out.println("RETRO " + id + " " + seed + " " + cx + " " + cz + " " + tag.getString("target_status").get() + " "
                + (tag.contains("missing_bedrock") ? Arrays.toString(((LongArrayTag) tag.get("missing_bedrock")).getAsLongArray()).replace(" ", "") : "-"));

            BelowZeroRetrogen.replaceOldBedrock(chunk);
            emitBlocks(out, id, "retro_replace", chunk);
            if (retrogen.hasBedrockHoles()) retrogen.applyBedrockMask(chunk);
            out.println("S " + id + " retro_holes " + (retrogen.hasBedrockHoles() ? 1 : 0) + " 0");
            emitBlocks(out, id, "retro_mask", chunk);

            Digest dr = new Digest();
            BiomeResolver base = (qx, qy, qz) -> biomeFn(seed ^ 13, qx, qy, qz);
            BiomeResolver wrapped = BelowZeroRetrogen.getBiomeResolver(base, chunk);
            for (int qx = cx * 4; qx < cx * 4 + 4; qx++) for (int qz = cz * 4; qz < cz * 4 + 4; qz++) for (int qy = -16; qy < 0; qy++) {
                dr.i(biomeId(wrapped.getNoiseBiome(qx, qy, qz)));
            }
            chunk.setBelowZeroRetrogen(null);
            BiomeResolver plain = BelowZeroRetrogen.getBiomeResolver(base, chunk);
            for (int qx = cx * 4; qx < cx * 4 + 4; qx++) for (int qz = cz * 4; qz < cz * 4 + 4; qz++) for (int qy = -16; qy < 0; qy++) {
                dr.i(biomeId(plain.getNoiseBiome(qx, qy, qz)));
            }
            emit(out, id, "retro_resolver", dr);
        }
        out.println("END");
        out.close();
    }

    static void emitBlocks(PrintWriter out, int id, String what, ProtoChunk chunk) {
        Digest d = new Digest();
        BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
        for (int y = -64; y < 16; y++) for (int z = 0; z < 16; z++) for (int x = 0; x < 16; x++) {
            d.i(Block.getId(chunk.getBlockState(pos.set(x, y, z))));
        }
        emit(out, id, what, d);
    }
}
