import java.io.*;
import java.util.*;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.feature.Feature;

/**
 * Oracle of crates/mc-game/tests/golden/vegetation_features_golden.rs: every data-pack {@code worldgen/feature} entry of the
 * vegetation/misc feature classes below placed with the real vanilla class on the real WorldGenRegion over the synthetic terrain
 * of FeatureOracleBase, at origins of several kinds (surface, ocean floor, inside the ground, caves, random y, world bounds,
 * chunk borders) on terrain sprinkled with feature-agnostic base blocks, pillars and surface covers.
 *
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/vegetation_features.txt crates/mc-game/tests/oracle/VegetationFeatureOracle.java crates/mc-game/tests/fixtures/vegetation_features.txt
 * (FeatureOracleBase.java next to this file)
 * Output: crates/mc-game/tests/fixtures/vegetation_features.txt
 *
 * Lines: {@code SCENARIO id feature seed cx cz}; {@code S x y z state} (setBlock flags 2, the prepared terrain); then per
 * placement {@code > P x y z L|X randomSeed} / {@code = <result> <next long of the random>}; then the dump lines as
 * {@code D <line>} (C/T/M lines dropped, heightmap digests folded into one line); {@code END}.
 */
public class VegetationFeatureOracle {
    static final Set<String> CLASSES = new HashSet<>(Arrays.asList(
        "SimpleBlockFeature", "BlockColumnFeature", "BambooFeature", "VinesFeature", "MultifaceGrowthFeature", "SculkPatchFeature",
        "VegetationPatchFeature", "WaterloggedVegetationPatchFeature", "RootSystemFeature", "CoralClawFeature", "CoralTreeFeature",
        "ChorusPlantFeature", "HugeBrownMushroomFeature", "HugeRedMushroomFeature", "HugeFungusFeature", "BonusChestFeature",
        "BlockPileFeature", "BlueIceFeature", "DeltaFeature", "OverlayFeature", "ProjectedRandomPatchySquare",
        "RandomNeighborSpreadFeature", "ReplaceBlockFeature", "SequenceFeature", "SnowAndFreezeFeature", "SpikeFeature",
        "UnderwaterMagmaFeature", "VoidStartPlatformFeature"));

    static final String[] BASES = {
        "minecraft:grass_block[snowy=false]", "minecraft:dirt", "minecraft:podzol[snowy=false]", "minecraft:mycelium[snowy=false]",
        "minecraft:moss_block", "minecraft:clay", "minecraft:stone", "minecraft:netherrack", "minecraft:crimson_nylium",
        "minecraft:warped_nylium", "minecraft:end_stone", "minecraft:packed_ice", "minecraft:snow_block", "minecraft:sand",
        "minecraft:red_sand", "minecraft:gravel", "minecraft:soul_sand", "minecraft:water[level=0]", "minecraft:mud",
        "minecraft:rooted_dirt", "minecraft:coarse_dirt", "minecraft:deepslate[axis=y]", "minecraft:tuff", "minecraft:calcite",
        "minecraft:dripstone_block", "minecraft:glow_lichen[down=true,east=false,north=false,south=false,up=false,waterlogged=false,west=false]",
        "minecraft:warped_wart_block", "minecraft:nether_wart_block", "minecraft:basalt[axis=y]", "minecraft:blackstone",
    };
    static final String[] COVERS = {
        "minecraft:short_grass", "minecraft:snow[layers=1]", "minecraft:water[level=0]", "minecraft:lava[level=0]", "minecraft:oak_leaves[distance=1,persistent=true,waterlogged=false]",
        "minecraft:moss_carpet", "minecraft:fern", "minecraft:brown_mushroom", "minecraft:vine[east=false,north=true,south=false,up=false,west=false]", "minecraft:packed_ice",
    };
    static final String[] PILLARS = {
        "minecraft:stone", "minecraft:oak_log[axis=y]", "minecraft:oak_leaves[distance=1,persistent=true,waterlogged=false]", "minecraft:moss_block",
        "minecraft:dirt", "minecraft:deepslate[axis=y]", "minecraft:netherrack", "minecraft:sculk", "minecraft:crimson_stem[axis=y]", "minecraft:glass",
    };

    /** Configurations that exercise the block-logic-heavy features directly (the data-pack uses them only inside sequences). */
    static final String[] SYNTHETIC = {
        "{\"type\":\"minecraft:sculk_patch\",\"charge_count\":10,\"amount_per_charge\":32,\"spread_attempts\":64,\"growth_rounds\":0,\"spread_rounds\":1}",
        "{\"type\":\"minecraft:sculk_patch\",\"charge_count\":1,\"amount_per_charge\":500,\"spread_attempts\":32,\"growth_rounds\":2,\"spread_rounds\":2}",
        "{\"type\":\"minecraft:sculk_patch\",\"charge_count\":32,\"amount_per_charge\":100,\"spread_attempts\":16,\"growth_rounds\":3,\"spread_rounds\":0}",
        "{\"type\":\"minecraft:sculk_patch\",\"charge_count\":4,\"amount_per_charge\":7,\"spread_attempts\":64,\"growth_rounds\":0,\"spread_rounds\":8}",
        "{\"type\":\"minecraft:multiface_growth\",\"block\":\"minecraft:glow_lichen\",\"can_be_placed_on\":\"#minecraft:base_stone_overworld\",\"can_place_on_floor\":true,\"can_place_on_ceiling\":true,\"can_place_on_wall\":true,\"chance_of_spreading\":1.0,\"search_range\":12}",
        "{\"type\":\"minecraft:multiface_growth\",\"block\":\"minecraft:glow_lichen\",\"can_be_placed_on\":[\"minecraft:grass_block\",\"minecraft:dirt\",\"minecraft:sand\",\"minecraft:oak_log\",\"minecraft:stone\"],\"can_place_on_wall\":true}",
        "{\"type\":\"minecraft:multiface_growth\",\"block\":\"minecraft:sculk_vein\",\"can_be_placed_on\":[\"minecraft:sculk\",\"minecraft:stone\",\"minecraft:deepslate\"],\"can_place_on_floor\":true,\"can_place_on_ceiling\":true,\"search_range\":64,\"chance_of_spreading\":0.25}",
        "{\"type\":\"minecraft:multiface_growth\",\"block\":\"minecraft:glow_lichen\",\"can_be_placed_on\":[\"minecraft:netherrack\",\"minecraft:moss_block\",\"minecraft:water\"],\"can_place_on_ceiling\":true,\"chance_of_spreading\":0.9,\"search_range\":1}",
    };

    static BlockState state(String s) throws Exception {
        return BlockStateParser.parseForBlock(BuiltInRegistries.BLOCK, s, false).blockState();
    }

    static int hi(Random r, int lo, int hi) {
        return lo + r.nextInt(hi - lo + 1);
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        PrintWriter out = new PrintWriter(new BufferedWriter(new FileWriter(args[0])));
        var registry = FeatureOracleBase.ACCESS.lookupOrThrow(Registries.FEATURE);
        List<Holder.Reference<Feature>> features = new ArrayList<>();
        registry.listElements().forEach(h -> {
            if (CLASSES.contains(h.value().getClass().getSimpleName())) features.add(h);
        });
        features.sort(Comparator.comparing(h -> h.key().identifier().toString()));
        // (id, feature, json): the data-pack entries, then the synthetic configurations (`SYNTHETIC`), which the Rust test decodes
        // from the `J <json>` line that follows their SCENARIO line
        List<Object[]> entries = new ArrayList<>();
        for (var h : features) entries.add(new Object[] {h.key().identifier().toString(), h.value(), null});
        var ops = net.minecraft.resources.RegistryOps.create(com.mojang.serialization.JsonOps.INSTANCE, FeatureOracleBase.ACCESS);
        for (int i = 0; i < SYNTHETIC.length; i++) {
            var json = com.google.gson.JsonParser.parseString(SYNTHETIC[i]);
            Feature f = Feature.DIRECT_CODEC.parse(ops, json).getOrThrow();
            entries.add(new Object[] {"synthetic:" + i, f, SYNTHETIC[i]});
        }
        Random rnd = new Random(0x5E6E7A71L);
        int id = 0;
        for (Object[] entry : entries) {
            for (int round = 0; round < 3; round++) {
                long seed = rnd.nextLong();
                int cx = round == 0 ? rnd.nextInt(5) - 2 : rnd.nextInt(2000) - 1000, cz = round == 0 ? rnd.nextInt(5) - 2 : rnd.nextInt(2000) - 1000;
                String name = (String) entry[0];
                Feature feature = (Feature) entry[1];
                out.println("SCENARIO " + id++ + " " + name + " " + seed + " " + cx + " " + cz);
                if (entry[2] != null) out.println("J " + entry[2]);
                FeatureOracleBase.Region region = FeatureOracleBase.newRegion(seed, cx, cz, ChunkStatus.FEATURES, 2);
                Random r = new Random(seed ^ name.hashCode());
                prepare(region, out, r, cx, cz);
                FeatureOracleBase.Snapshot before = FeatureOracleBase.snapshot(region); // B lines show only what the feature changed
                boolean crashed = false;
                for (int k = 0; k < 8; k++) {
                    int x = cx * 16 + hi(r, k == 7 ? -4 : 0, k == 7 ? 19 : 15), z = cz * 16 + hi(r, k == 7 ? -4 : 0, k == 7 ? 19 : 15);
                    int y = originY(region, r, k, round, x, z);
                    boolean legacy = r.nextBoolean();
                    long rs = r.nextLong();
                    out.println("> P " + x + " " + y + " " + z + " " + (legacy ? "L" : "X") + " " + rs);
                    out.flush();
                    RandomSource random = legacy ? new LegacyRandomSource(rs) : new XoroshiroRandomSource(rs);
                    boolean placed;
                    try {
                        placed = feature.place(region, FeatureOracleBase.generator(), random, new BlockPos(x, y, z));
                    } catch (Throwable t) {
                        // e.g. features that need a ServerLevel (template); the Rust test skips the rest of the scenario
                        out.println("= EXC " + t);
                        crashed = true;
                        break;
                    }
                    out.println("= " + placed + " " + random.nextLong());
                }
                if (crashed) {
                    out.println("END");
                    continue;
                }
                for (String line : FeatureOracleBase.dump(region, before)) {
                    if (line.startsWith("C ") || line.startsWith("T ") || line.startsWith("M ")) continue;
                    if (line.startsWith("H ")) continue;
                    out.println("D " + line);
                }
                out.println("D HFOLD " + heightmapFold(region));
                out.println("END");
            }
        }
        out.close();
    }

    static String heightmapFold(FeatureOracleBase.Region region) {
        FeatureOracleBase.Fnv f = new FeatureOracleBase.Fnv();
        List<net.minecraft.world.level.chunk.ProtoChunk> chunks = new ArrayList<>(region.chunks.values());
        chunks.sort(Comparator.<net.minecraft.world.level.chunk.ProtoChunk>comparingInt(c -> c.getPos().x()).thenComparingInt(c -> c.getPos().z()));
        for (var c : chunks) {
            List<Map.Entry<Heightmap.Types, Heightmap>> maps = new ArrayList<>(c.getHeightmaps());
            maps.sort(Comparator.comparing(e -> e.getKey().name()));
            for (var e : maps) for (long l : e.getValue().getRawData()) f.l(l);
        }
        return f.hex();
    }

    /** Feature-agnostic terrain variation around the centre chunk, written as {@code S} lines and applied to the region. */
    static void prepare(FeatureOracleBase.Region region, PrintWriter out, Random r, int cx, int cz) throws Exception {
        int x0 = cx * 16 + 1, z0 = cz * 16 + 1;
        for (int x = x0; x < x0 + 14; x++) {
            for (int z = z0; z < z0 + 14; z++) {
                int top = region.getHeight(Heightmap.Types.OCEAN_FLOOR, x, z);
                int roll = r.nextInt(64);
                if (roll < 26) set(region, out, x, top - 1, z, BASES[r.nextInt(BASES.length)]);
                if (r.nextInt(10) == 0) {
                    int h = hi(r, 1, 5);
                    String p = PILLARS[r.nextInt(PILLARS.length)];
                    for (int dy = 0; dy < h; dy++) set(region, out, x, top + dy, z, p);
                } else if (r.nextInt(8) == 0) {
                    set(region, out, x, top, z, COVERS[r.nextInt(COVERS.length)]);
                }
                if (r.nextInt(40) == 0) set(region, out, x, top + hi(r, 2, 6), z, PILLARS[r.nextInt(PILLARS.length)]); // floating block / ceiling
            }
        }
    }

    static void set(FeatureOracleBase.Region region, PrintWriter out, int x, int y, int z, String s) throws Exception {
        out.println("S " + x + " " + y + " " + z + " " + s);
        region.setBlock(new BlockPos(x, y, z), state(s), 2, 512);
    }

    static int originY(FeatureOracleBase.Region region, Random r, int k, int round, int x, int z) {
        int surface = region.getHeight(Heightmap.Types.WORLD_SURFACE, x, z);
        int floor = region.getHeight(Heightmap.Types.OCEAN_FLOOR, x, z);
        switch (k) {
            case 0: return surface;
            case 1: return floor;
            case 2: return floor - 1;
            case 3: {
                // a cave floor/ceiling position: air with solid below or above, searched downwards from the surface
                for (int y = floor - 10; y > FeatureOracleBase.MIN_Y + 6; y--) {
                    BlockPos p = new BlockPos(x, y, z);
                    if (region.getBlockState(p).isAir() && (!region.getBlockState(p.below()).isAir() || !region.getBlockState(p.above()).isAir())) return y;
                }
                return hi(r, -50, 40);
            }
            case 4: return hi(r, FeatureOracleBase.MIN_Y, 130);
            case 5: return round % 2 == 0 ? FeatureOracleBase.MIN_Y + hi(r, 0, 8) : 300 + hi(r, 0, 80);
            case 6: return surface + 1;
            default: return surface + hi(r, -3, 3);
        }
    }
}
