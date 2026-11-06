import java.io.*;
import java.util.*;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.feature.FallenTreeFeature;
import net.minecraft.world.level.levelgen.feature.Feature;
import net.minecraft.world.level.levelgen.feature.TreeFeature;

/**
 * Oracle of crates/mc-game/tests/golden/tree_golden.rs: every `tree` / `fallen_tree` entry of the data pack is placed 6 times in a
 * region of the synthetic terrain (FeatureOracleBase), for 5 terrain variants and different seeds/chunks; the oracle prints the
 * per-placement result + next RNG long and digests of the region dump per line kind.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/tree_golden.txt crates/mc-game/tests/oracle/TreeOracle.java crates/mc-game/tests/fixtures/tree_golden.txt [scenarioId [full]]
 * (FeatureOracleBase.java next to this file)
 * Output: crates/mc-game/tests/fixtures/tree_golden.txt
 * (`full`: prints every dump line as `L <line>`, for diffing against the Rust test's /tmp dump.)
 */
public class TreeOracle {
    static final int VARIANTS = 5;
    static final int PLACEMENTS = 6;

    static int hmod(long v, int m) {
        return FeatureOracleBase.hmod(v, m);
    }

    static long h(long seed, long a, long b, long c) {
        return FeatureOracleBase.h(seed, a, b, c);
    }

    static BlockState state(String s) throws Exception {
        return BlockStateParser.parseForBlock(BuiltInRegistries.BLOCK, s, false).blockState();
    }

    static void put(FeatureOracleBase.Region region, int x, int y, int z, String state) throws Exception {
        region.setBlock(new BlockPos(x, y, z), state(state), 18, 512);
    }

    static boolean isAir(FeatureOracleBase.Region region, int x, int y, int z) {
        return region.getBlockState(new BlockPos(x, y, z)).isAir();
    }

    /** Terrain variants applied identically by the Rust test (prepare()). */
    static void prepare(FeatureOracleBase.Region region, long seed, int variant, int cx, int cz) throws Exception {
        for (int x = cx * 16 - 8; x < cx * 16 + 24; x++) {
            for (int z = cz * 16 - 8; z < cz * 16 + 24; z++) {
                int top = FeatureOracleBase.surfaceTop(seed, x, z);
                long hv = h(seed, x, z, 100 + variant);
                String ground = BlockStateParser.serialize(region.getBlockState(new BlockPos(x, top - 1, z)));
                boolean grass = ground.startsWith("minecraft:grass_block");
                boolean dirty = grass || ground.startsWith("minecraft:dirt");
                switch (variant) {
                    case 1 -> {
                        if (grass) put(region, x, top - 1, z, "minecraft:grass_block[snowy=true]");
                        if (top >= 63 && isAir(region, x, top, z) && hmod(hv, 4) != 0) put(region, x, top, z, "minecraft:snow[layers=" + (1 + hmod(hv >>> 8, 3)) + "]");
                    }
                    case 2 -> {
                        if (dirty) {
                            switch (hmod(hv, 5)) {
                                case 0 -> put(region, x, top - 1, z, "minecraft:podzol[snowy=false]");
                                case 1 -> put(region, x, top - 1, z, "minecraft:coarse_dirt");
                                case 2 -> put(region, x, top - 1, z, "minecraft:mud");
                                case 3 -> put(region, x, top - 1, z, "minecraft:rooted_dirt");
                                default -> { }
                            }
                        }
                    }
                    case 3 -> {
                        int r = hmod(hv, 40);
                        int d = hmod(h(seed, x, z, 200), 3);
                        if (r < 3) {
                            if (isAir(region, x, top + 1 + d, z)) put(region, x, top + 1 + d, z, "minecraft:oak_leaves[distance=7,persistent=true,waterlogged=false]");
                        } else if (r < 5) {
                            if (isAir(region, x, top, z)) put(region, x, top, z, "minecraft:vine[south=true]");
                        } else if (r < 7) {
                            for (int y = top; y < top + 3; y++) if (isAir(region, x, y, z)) put(region, x, y, z, "minecraft:oak_log[axis=y]");
                        } else if (r < 9) {
                            for (int y = top; y < top + 2 + d; y++) if (isAir(region, x, y, z)) put(region, x, y, z, "minecraft:stone");
                        } else if (r < 20) {
                            if (isAir(region, x, top, z)) put(region, x, top, z, "minecraft:short_grass");
                        } else if (r < 24) {
                            if (isAir(region, x, top + 1, z)) put(region, x, top + 1, z, "minecraft:oak_leaves[distance=4,persistent=false,waterlogged=false]");
                        }
                    }
                    case 4 -> {
                        if (top <= 65) {
                            if (dirty && hmod(hv, 2) == 0) put(region, x, top - 1, z, "minecraft:mud");
                            for (int y = top; y <= 65; y++) if (isAir(region, x, y, z)) put(region, x, y, z, "minecraft:water[level=0]");
                        }
                    }
                    default -> { }
                }
            }
        }
    }

    static Object alloc(sun.misc.Unsafe unsafe, Class<?> c) throws Exception {
        return unsafe.allocateInstance(c);
    }

    static void set(Class<?> owner, Object o, String name, Object value) throws Exception {
        java.lang.reflect.Field f = owner.getDeclaredField(name);
        f.setAccessible(true);
        f.set(o, value);
    }

    /** PaleMossDecorator reaches the generator through `level.getLevel().getChunkSource().getGenerator()`: a stub chain. */
    static void attachGeneratorStub(FeatureOracleBase.Region region) throws Exception {
        java.lang.reflect.Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        uf.setAccessible(true);
        sun.misc.Unsafe unsafe = (sun.misc.Unsafe) uf.get(null);
        var level = alloc(unsafe, net.minecraft.server.level.ServerLevel.class);
        var cache = alloc(unsafe, net.minecraft.server.level.ServerChunkCache.class);
        var map = alloc(unsafe, net.minecraft.server.level.ChunkMap.class);
        var context = new net.minecraft.world.level.chunk.status.WorldGenContext(null, FeatureOracleBase.generator(), null, null, null, null);
        set(net.minecraft.server.level.ChunkMap.class, map, "worldGenContext", context);
        set(net.minecraft.server.level.ServerChunkCache.class, cache, "chunkMap", map);
        set(net.minecraft.server.level.ServerLevel.class, level, "chunkSource", cache);
        set(net.minecraft.server.level.WorldGenRegion.class, region, "level", level);
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        String only = args.length > 1 ? args[1] : null;
        boolean full = args.length > 2;
        var registry = FeatureOracleBase.ACCESS.lookupOrThrow(Registries.FEATURE);
        List<String> ids = new ArrayList<>();
        registry.listElements().forEach(e -> {
            Feature f = e.value();
            if (f instanceof TreeFeature || f instanceof FallenTreeFeature) ids.add(e.key().identifier().toString());
        });
        Collections.sort(ids);
        StringBuilder out = new StringBuilder();
        int scenario = 0;
        for (int fi = 0; fi < ids.size(); fi++) {
            Feature feature = registry.get(net.minecraft.resources.ResourceKey.create(Registries.FEATURE, net.minecraft.resources.Identifier.parse(ids.get(fi)))).orElseThrow().value();
            for (int variant = 0; variant < VARIANTS; variant++, scenario++) {
                if (only != null && !only.equals("all") && !only.equals(Integer.toString(scenario))) continue;
                long seed = h(0x7ee5, fi, variant, 1);
                int cx = (scenario * 3) % 7 - 3, cz = (scenario * 5) % 7 - 3;
                FeatureOracleBase.Region region = FeatureOracleBase.newRegion(seed, cx, cz, ChunkStatus.FEATURES, 1);
                attachGeneratorStub(region);
                prepare(region, seed, variant, cx, cz);
                FeatureOracleBase.Snapshot before = FeatureOracleBase.snapshot(region);
                out.append("SCENARIO ").append(scenario).append(' ').append(ids.get(fi)).append(' ').append(seed).append(' ').append(variant).append(' ').append(cx).append(' ').append(cz).append('\n');
                for (int i = 0; i < PLACEMENTS; i++) {
                    int x = cx * 16 + hmod(h(seed, fi, i, 11), 16), z = cz * 16 + hmod(h(seed, fi, i, 12), 16);
                    int y = region.getHeight(Heightmap.Types.OCEAN_FLOOR, x, z);
                    int r = hmod(h(seed, fi, i, 13), 6);
                    if (r == 0) y -= 1; else if (r == 1) y += 1; else if (r == 2) y += 2;
                    RandomSource random = new XoroshiroRandomSource(h(seed, fi, i, 14));
                    boolean placed = feature.place(region, FeatureOracleBase.generator(), random, new BlockPos(x, y, z));
                    out.append("P ").append(i).append(' ').append(x).append(' ').append(y).append(' ').append(z).append(' ').append(placed).append(' ').append(random.nextLong()).append('\n');
                }
                List<String> dump = FeatureOracleBase.dump(region, before);
                Map<String, FeatureOracleBase.Fnv> digests = new TreeMap<>();
                for (String line : dump) {
                    String kind = line.substring(0, 1);
                    digests.computeIfAbsent(kind, k -> new FeatureOracleBase.Fnv()).s(line);
                    if (full) out.append("L ").append(line).append('\n');
                }
                for (var e : digests.entrySet()) out.append("D ").append(e.getKey()).append(' ').append(e.getValue().count).append(' ').append(e.getValue().hex()).append('\n');
                out.append("END\n");
            }
        }
        try (var w = new FileWriter(args[0])) {
            w.write(out.toString());
        }
    }
}
