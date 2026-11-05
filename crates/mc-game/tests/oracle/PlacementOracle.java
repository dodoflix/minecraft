import java.io.*;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.placement.BiomeFilter;
import net.minecraft.world.level.levelgen.placement.PlacedFeature;
import net.minecraft.world.level.levelgen.placement.PlacementContext;
import net.minecraft.world.level.levelgen.placement.PlacementModifier;

/**
 * Oracle of crates/mc-game/tests/golden/placement_golden.rs: for every placed feature of the data pack, the PLACEMENT chain only (the
 * loop of vanilla's FeaturePlacer.place with biome check, minus the feature) over the shared synthetic world: the ordered final
 * positions (count + order-sensitive hash) and the next long of the random after the chain.
 * Plus the chains of tests/fixtures/placement_synthetic.json (modifier types and parameters the data pack never uses), id {@code synthetic:<name>}.
 * Usage (repo root): scripts/vanilla-java.sh crates/mc-game/tests/oracle/PlacementOracle.java crates/mc-game/tests/fixtures/placement_golden.txt crates/mc-game/tests/fixtures/placement_synthetic.json [idFilter]
 *
 * Lines: {@code R seed cx cz} (region), six {@code O x y z} origins, then per case {@code id mode oi count hash nextLong}
 * ({@code EXC} instead of the results when vanilla throws). Mode b = vanilla chain with the biome check, n = the chain without
 * its BiomeFilter entries (so the rest of the chain is exercised where the synthetic biomes would reject the feature).
 * Origin oi odd uses a LegacyRandomSource, even a XoroshiroRandomSource, seeded by {@link #randomSeed}.
 */
public class PlacementOracle {
    static final long[] SEEDS = {1L, 0x5DEECE66DL, -7046029254386353131L};
    static final int[][] CHUNKS = {{0, 0}, {-3, 5}};

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        String filter = args.length > 2 ? args[2] : null;
        var ops = net.minecraft.resources.RegistryOps.create(com.mojang.serialization.JsonOps.INSTANCE, (net.minecraft.core.RegistryAccess) FeatureOracleBase.ACCESS);
        List<String> syntheticNames = new ArrayList<>();
        List<List<PlacementModifier>> synthetic = new ArrayList<>();
        for (var e : com.google.gson.JsonParser.parseString(java.nio.file.Files.readString(java.nio.file.Path.of(args[1]))).getAsJsonArray()) {
            syntheticNames.add("synthetic:" + e.getAsJsonObject().get("name").getAsString());
            synthetic.add(PlacementModifier.CODEC.listOf().parse(ops, e.getAsJsonObject().get("placement")).getOrThrow());
        }
        var registry = FeatureOracleBase.ACCESS.lookupOrThrow(Registries.PLACED_FEATURE);
        List<Identifier> ids = new ArrayList<>();
        registry.listElementIds().forEach(k -> ids.add(k.identifier()));
        ids.sort(Comparator.comparing(Identifier::toString));
        try (PrintWriter out = new PrintWriter(new FileWriter(args[0]))) {
            int cases = 0;
            for (long seed : SEEDS) {
                for (int[] c : CHUNKS) {
                    var region = FeatureOracleBase.newRegion(seed, c[0], c[1], ChunkStatus.FEATURES, 2);
                    out.println("R " + seed + " " + c[0] + " " + c[1]);
                    Random pick = new Random(seed ^ (c[0] * 31L + c[1]));
                    BlockPos[] origins = new BlockPos[6];
                    origins[0] = new BlockPos(c[0] * 16, FeatureOracleBase.MIN_Y, c[1] * 16);
                    for (int i = 1; i < origins.length; i++)
                        origins[i] = new BlockPos(c[0] * 16 + pick.nextInt(16), -60 + pick.nextInt(360), c[1] * 16 + pick.nextInt(16));
                    for (BlockPos o : origins) out.println("O " + o.getX() + " " + o.getY() + " " + o.getZ());
                    for (Identifier id : ids) {
                        if (filter != null && !id.toString().equals(filter)) continue;
                        PlacedFeature pf = registry.getOrThrow(net.minecraft.resources.ResourceKey.create(Registries.PLACED_FEATURE, id)).value();
                        for (char mode : new char[] {'b', 'n'}) {
                            for (int oi = 0; oi < origins.length; oi++) {
                                BlockPos origin = origins[oi];
                                RandomSource random = oi % 2 == 1 ? new LegacyRandomSource(randomSeed(seed, c, oi, id)) : new XoroshiroRandomSource(randomSeed(seed, c, oi, id));
                                StringBuilder line = new StringBuilder(id + " " + mode + " " + oi + " ");
                                try {
                                    List<BlockPos> result = chain(pf.placement(), mode == 'b' ? Optional.of(pf) : Optional.empty(), region, random, origin);
                                    long acc = 0;
                                    for (BlockPos p : result) acc = FeatureOracleBase.mix64(acc ^ p.asLong());
                                    line.append(result.size()).append(' ').append(Long.toHexString(acc)).append(' ').append(random.nextLong());
                                } catch (Throwable t) {
                                    line.append("EXC");
                                }
                                out.println(line);
                                cases++;
                            }
                        }
                    }
                    for (int si = 0; si < synthetic.size(); si++) {
                        String name = syntheticNames.get(si);
                        if (filter != null && !name.equals(filter)) continue;
                        for (int oi = 0; oi < origins.length; oi++) {
                            RandomSource random = oi % 2 == 1 ? new LegacyRandomSource(randomSeed(seed, c, oi, name)) : new XoroshiroRandomSource(randomSeed(seed, c, oi, name));
                            StringBuilder line = new StringBuilder(name + " n " + oi + " ");
                            try {
                                List<BlockPos> result = chain(synthetic.get(si), Optional.empty(), region, random, origins[oi]);
                                long acc = 0;
                                for (BlockPos p : result) acc = FeatureOracleBase.mix64(acc ^ p.asLong());
                                line.append(result.size()).append(' ').append(Long.toHexString(acc)).append(' ').append(random.nextLong());
                            } catch (Throwable t) {
                                line.append("EXC");
                            }
                            out.println(line);
                            cases++;
                        }
                    }
                }
            }
            System.out.println(cases + " cases");
        }
    }

    static long randomSeed(long seed, int[] c, int oi, Object id) {
        return FeatureOracleBase.h(seed, c[0], c[1], oi * 1000003L + id.toString().hashCode());
    }

    /** FeaturePlacer.place(.., biomeCheck = true) with the feature replaced by recording the position. */
    static List<BlockPos> chain(List<PlacementModifier> modifiers, Optional<PlacedFeature> top, FeatureOracleBase.Region level, RandomSource random, BlockPos origin) {
        PlacementContext context = new PlacementContext(level, FeatureOracleBase.generator(), top);
        List<PlacementModifier> placement = top.isPresent() ? modifiers : modifiers.stream().filter(m -> !(m instanceof BiomeFilter)).toList();
        List<BlockPos> result = new ArrayList<>();
        if (placement.isEmpty()) {
            result.add(origin);
            return result;
        }
        List<BlockPos> positions = new ArrayList<>();
        List<Integer> modifierIndices = new ArrayList<>();
        List<BlockPos> modified = new ArrayList<>();
        positions.add(origin);
        modifierIndices.add(0);
        for (; !positions.isEmpty(); modified.clear()) {
            BlockPos pos = positions.removeLast();
            int index = modifierIndices.removeLast();
            placement.get(index).modify(context, random, pos, modified::add);
            int next = index + 1;
            if (next < placement.size()) {
                for (int i = modified.size() - 1; i >= 0; i--) {
                    positions.add(modified.get(i));
                    modifierIndices.add(next);
                }
            } else {
                result.addAll(modified);
            }
        }
        return result;
    }
}
