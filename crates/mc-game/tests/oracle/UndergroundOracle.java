import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.io.*;
import java.lang.reflect.Field;
import java.nio.file.*;
import java.util.*;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.util.StaticCache2D;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.ProtoChunk;
import net.minecraft.world.level.chunk.status.ChunkPyramid;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.chunk.status.ChunkStep;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.feature.EndSpikeFeature;
import net.minecraft.world.level.levelgen.feature.Feature;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.ticks.WorldGenTickAccess;

/**
 * Oracle of crates/mc-game/tests/feat_underground_golden.rs: the vanilla geode, speleothem, speleothem_cluster,
 * large_dripstone, monster_room, spike, end_spike, end_gateway, end_island, end_podium and end_platform features run on the
 * real vanilla WorldGenRegion over the synthetic terrain of FeatureOracleBase (data-pack entries plus hand written
 * variants), after random terrain preparation (caverns, solid blocks, snow), followed by FeatureOracleBase.dump.
 * Usage (repo root): scripts/vanilla-java.sh crates/mc-game/tests/oracle/UndergroundOracle.java crates/mc-game/tests/fixtures/feat_underground.txt
 *
 * Output lines: {@code CASE id kind json}; {@code BAD id json} + {@code = ERR} (the codec must reject); then per case a few
 * {@code SCENARIO id n seed cx cz}, {@code > op} / {@code = result} lines, {@code D <dump line>} lines and {@code END}.
 * Entity UUIDs are random in vanilla and therefore stripped from the dump ({@code N} lines).
 */
public class UndergroundOracle {
    static final Set<String> TYPES = Set.of("geode", "speleothem", "speleothem_cluster", "large_dripstone", "monster_room", "spike", "end_spike",
        "end_gateway", "end_island", "end_podium", "end_platform");

    record Case(String id, String kind, JsonObject json) {}

    // ---- the region with a fake ServerLevel (entity creation) ----
    public static class FakeLevel extends ServerLevel {
        FakeLevel() {
            super(null, null, null, null, null, null, false, 0L, null, false);
        }

        @Override public int getNextEntityId() { return 0; }
        @Override public RegistryAccess registryAccess() { return FeatureOracleBase.ACCESS; }
        @Override public FeatureFlagSet enabledFeatures() { return FeatureFlags.DEFAULT_FLAGS; }
    }

    public static class UgRegion extends FeatureOracleBase.Region {
        FakeLevel fake;

        @Override public ServerLevel getLevel() { return fake; }
    }

    static sun.misc.Unsafe unsafe() throws Exception {
        Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        uf.setAccessible(true);
        return (sun.misc.Unsafe) uf.get(null);
    }

    static UgRegion newRegion(long seed, int cx, int cz) throws Exception {
        FeatureOracleBase.setup();
        sun.misc.Unsafe unsafe = unsafe();
        UgRegion region = (UgRegion) unsafe.allocateInstance(UgRegion.class);
        region.fake = (FakeLevel) unsafe.allocateInstance(FakeLevel.class);
        region.seed = seed;
        region.chunks = new HashMap<>();
        int chunkRadius = 1;
        for (int x = cx - chunkRadius; x <= cx + chunkRadius; x++)
            for (int z = cz - chunkRadius; z <= cz + chunkRadius; z++)
                region.chunks.put(ChunkPos.pack(x, z), FeatureOracleBase.makeChunk(seed, x, z, ChunkStatus.TERRAIN, true));
        ChunkStep step = ChunkPyramid.GENERATION_PYRAMID.getStepTo(ChunkStatus.FEATURES);
        StaticCache2D<ChunkAccess> cache = StaticCache2D.create(cx, cz, chunkRadius, (x, z) -> region.chunks.get(ChunkPos.pack(x, z)));
        RandomState randomState = FeatureOracleBase.randomState(seed);
        FeatureOracleBase.setField(region, "cache", cache);
        FeatureOracleBase.setField(region, "center", region.chunks.get(ChunkPos.pack(cx, cz)));
        FeatureOracleBase.setField(region, "seed", seed);
        FeatureOracleBase.setField(region, "random", randomState.getOrCreateRandomFactory(net.minecraft.resources.Identifier.withDefaultNamespace("worldgen_region_random")).at(new ChunkPos(cx, cz).getWorldPosition()));
        FeatureOracleBase.setField(region, "blockTicks", new WorldGenTickAccess<Block>(pos -> region.getChunk(pos).getBlockTicks()));
        FeatureOracleBase.setField(region, "fluidTicks", new WorldGenTickAccess<Fluid>(pos -> region.getChunk(pos).getFluidTicks()));
        FeatureOracleBase.setField(region, "biomeManager", new net.minecraft.world.level.biome.BiomeManager(region, net.minecraft.world.level.biome.BiomeManager.obfuscateSeed(seed)));
        FeatureOracleBase.setField(region, "generatingStep", step);
        FeatureOracleBase.setField(region, "subTickCount", new java.util.concurrent.atomic.AtomicLong());
        FeatureOracleBase.setField(region, "uncachedBiomeResolver", FeatureOracleBase.generator().getBiomeSource().createUncachedResolver(randomState));
        FeatureOracleBase.setField(region, "centerChunkX", cx);
        FeatureOracleBase.setField(region, "centerChunkZ", cz);
        FeatureOracleBase.setField(region, "writeRadius", step.blockStateWriteRadius());
        return region;
    }

    // ---- cases ----
    static void collect(String id, JsonElement e, List<JsonObject> out) {
        if (e.isJsonObject()) {
            JsonObject o = e.getAsJsonObject();
            if (o.has("type") && o.get("type").isJsonPrimitive() && TYPES.contains(o.get("type").getAsString().replace("minecraft:", ""))) out.add(o);
            for (var en : o.entrySet()) collect(id, en.getValue(), out);
        } else if (e.isJsonArray()) {
            for (JsonElement c : e.getAsJsonArray()) collect(id, c, out);
        }
    }

    static JsonObject obj(String s) {
        return JsonParser.parseString(s).getAsJsonObject();
    }

    /** Hand-written valid variants exercising defaults, extremes and rarely taken branches. */
    static String[] variants() {
        return new String[] {
            "{\"type\":\"minecraft:geode\",\"blocks\":{\"filling_provider\":{\"id\":\"minecraft:water\"},\"inner_layer_provider\":{\"id\":\"minecraft:amethyst_block\"},\"alternate_inner_layer_provider\":{\"id\":\"minecraft:budding_amethyst\"},\"middle_layer_provider\":{\"id\":\"minecraft:calcite\"},\"outer_layer_provider\":{\"id\":\"minecraft:smooth_basalt\"},\"inner_placements\":[\"minecraft:small_amethyst_bud\",\"minecraft:large_amethyst_bud\",{\"id\":\"minecraft:amethyst_cluster\",\"properties\":{\"waterlogged\":\"true\"}}],\"cannot_replace\":\"#minecraft:features_cannot_replace\",\"invalid_blocks\":\"#minecraft:geode_invalid_blocks\"},\"layers\":{\"filling\":1.0,\"inner_layer\":1.5,\"middle_layer\":2.0,\"outer_layer\":3.0},\"crack\":{\"generate_crack_chance\":0.0},\"use_potential_placements_chance\":1.0,\"use_alternate_layer0_chance\":1.0,\"placements_require_layer0_alternate\":false,\"outer_wall_distance\":5,\"distribution_points\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":2,\"max_inclusive\":6},\"point_offset\":0,\"min_gen_offset\":-10,\"max_gen_offset\":10,\"noise_multiplier\":0.2,\"invalid_blocks_threshold\":100}",
            "{\"type\":\"minecraft:geode\",\"blocks\":{\"filling_provider\":{\"id\":\"minecraft:air\"},\"inner_layer_provider\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:gold_block\"}},\"alternate_inner_layer_provider\":{\"id\":\"minecraft:diamond_block\"},\"middle_layer_provider\":{\"id\":\"minecraft:iron_block\"},\"outer_layer_provider\":{\"id\":\"minecraft:copper_block\"},\"inner_placements\":[\"minecraft:small_amethyst_bud\"],\"cannot_replace\":[\"minecraft:bedrock\",\"minecraft:chest\"],\"invalid_blocks\":\"minecraft:lava\"},\"layers\":{\"outer_layer\":50.0,\"filling\":0.01},\"crack\":{\"generate_crack_chance\":1.0,\"base_crack_size\":5.0,\"crack_point_offset\":10},\"min_gen_offset\":6,\"max_gen_offset\":-6,\"invalid_blocks_threshold\":-1}",
            "{\"type\":\"minecraft:speleothem\",\"base_block\":\"minecraft:dripstone_block\",\"pointed_block\":\"minecraft:pointed_dripstone\",\"replaceable_blocks\":\"#minecraft:dripstone_replaceable_blocks\",\"chance_of_taller_generation\":1.0,\"chance_of_directional_spread\":1.0,\"chance_of_spread_radius2\":1.0,\"chance_of_spread_radius3\":1.0}",
            "{\"type\":\"minecraft:speleothem\",\"base_block\":\"minecraft:sulfur\",\"pointed_block\":\"minecraft:sulfur_spike\",\"replaceable_blocks\":[\"minecraft:stone\",\"minecraft:deepslate\"],\"chance_of_taller_generation\":0.0,\"chance_of_directional_spread\":0.0,\"chance_of_spread_radius2\":0.0,\"chance_of_spread_radius3\":0.0}",
            "{\"type\":\"minecraft:speleothem_cluster\",\"base_block\":\"minecraft:dripstone_block\",\"pointed_block\":\"minecraft:pointed_dripstone\",\"replaceable_blocks\":\"#minecraft:dripstone_replaceable_blocks\",\"floor_to_ceiling_search_range\":40,\"height\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":2,\"max_inclusive\":12},\"radius\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":3,\"max_inclusive\":9},\"max_stalagmite_stalactite_height_diff\":3,\"height_deviation\":5,\"speleothem_block_layer_thickness\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0,\"max_inclusive\":5},\"density\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0.6,\"max_exclusive\":1.2},\"wetness\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0.5,\"max_exclusive\":1.5},\"chance_of_speleothem_at_max_distance_from_center\":0.8,\"max_distance_from_edge_affecting_chance_of_speleothem\":4,\"max_distance_from_center_affecting_height_bias\":5}",
            "{\"type\":\"minecraft:large_dripstone\",\"replaceable_blocks\":\"#minecraft:dripstone_replaceable_blocks\",\"floor_to_ceiling_search_range\":60,\"column_radius\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":4,\"max_inclusive\":14},\"height_scale\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":1.0,\"max_exclusive\":6.0},\"max_column_radius_to_cave_height_ratio\":0.9,\"stalactite_bluntness\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0.5,\"max_exclusive\":3.0},\"stalagmite_bluntness\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0.5,\"max_exclusive\":3.0},\"wind_speed\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0.5,\"max_exclusive\":2.0},\"min_radius_for_wind\":0,\"min_bluntness_for_wind\":0.0}",
            "{\"type\":\"minecraft:end_spike\",\"spikes\":[{\"centerX\":3,\"centerZ\":-5,\"radius\":4,\"height\":70,\"guarded\":true},{\"centerX\":-2,\"centerZ\":9,\"radius\":2,\"height\":95},{}],\"crystal_invulnerable\":true,\"crystal_beam_target\":[0,128,0]}",
            "{\"type\":\"minecraft:end_gateway\",\"exact\":false,\"exit\":[10,60,-20]}",
            "{\"type\":\"minecraft:end_podium\"}",
            "{\"type\":\"minecraft:spike\",\"state\":{\"id\":\"minecraft:packed_ice\"},\"can_place_on\":{\"type\":\"minecraft:matching_blocks\",\"blocks\":[\"minecraft:snow_block\",\"minecraft:grass_block\"]},\"can_replace\":{\"type\":\"minecraft:matching_blocks\",\"blocks\":[\"minecraft:water\",\"minecraft:grass_block\",\"minecraft:dirt\"]}}",
        };
    }

    /** JSON the codec must reject. */
    static String[] bad() {
        String geode = "\"blocks\":{\"filling_provider\":{\"id\":\"minecraft:air\"},\"inner_layer_provider\":{\"id\":\"minecraft:stone\"},\"alternate_inner_layer_provider\":{\"id\":\"minecraft:stone\"},\"middle_layer_provider\":{\"id\":\"minecraft:stone\"},\"outer_layer_provider\":{\"id\":\"minecraft:stone\"},\"inner_placements\":[\"minecraft:stone\"],\"cannot_replace\":\"minecraft:bedrock\",\"invalid_blocks\":\"minecraft:lava\"},\"layers\":{},\"crack\":{},\"invalid_blocks_threshold\":1";
        String spel = "\"base_block\":\"minecraft:dripstone_block\",\"pointed_block\":\"minecraft:pointed_dripstone\",\"replaceable_blocks\":\"minecraft:stone\"";
        String cluster = spel + ",\"floor_to_ceiling_search_range\":12,\"height\":3,\"radius\":3,\"max_stalagmite_stalactite_height_diff\":1,\"height_deviation\":3,\"speleothem_block_layer_thickness\":2,\"density\":0.5,\"wetness\":0.5,\"chance_of_speleothem_at_max_distance_from_center\":0.1,\"max_distance_from_edge_affecting_chance_of_speleothem\":3,\"max_distance_from_center_affecting_height_bias\":8";
        String large = "\"replaceable_blocks\":\"minecraft:stone\",\"column_radius\":3,\"height_scale\":1.0,\"max_column_radius_to_cave_height_ratio\":0.5,\"stalactite_bluntness\":1.0,\"stalagmite_bluntness\":1.0,\"wind_speed\":1.0,\"min_radius_for_wind\":2,\"min_bluntness_for_wind\":1.0";
        return new String[] {
            "{\"type\":\"minecraft:geode\"," + geode.replace("\"layers\":{}", "\"layers\":{\"filling\":0.0}") + "}",
            "{\"type\":\"minecraft:geode\"," + geode.replace("\"layers\":{}", "\"layers\":{\"outer_layer\":50.5}") + "}",
            "{\"type\":\"minecraft:geode\"," + geode.replace("\"crack\":{}", "\"crack\":{\"base_crack_size\":5.1}") + "}",
            "{\"type\":\"minecraft:geode\"," + geode.replace("\"crack\":{}", "\"crack\":{\"crack_point_offset\":11}") + "}",
            "{\"type\":\"minecraft:geode\"," + geode.replace("\"crack\":{}", "\"crack\":{\"generate_crack_chance\":1.5}") + "}",
            "{\"type\":\"minecraft:geode\"," + geode.replace(",\"invalid_blocks_threshold\":1", "") + "}",
            "{\"type\":\"minecraft:geode\"," + geode + ",\"outer_wall_distance\":21}",
            "{\"type\":\"minecraft:geode\"," + geode + ",\"outer_wall_distance\":0}",
            "{\"type\":\"minecraft:geode\"," + geode + ",\"distribution_points\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0,\"max_inclusive\":3}}",
            "{\"type\":\"minecraft:geode\"," + geode + ",\"point_offset\":11}",
            "{\"type\":\"minecraft:geode\"," + geode + ",\"noise_multiplier\":1.1}",
            "{\"type\":\"minecraft:geode\"," + geode + ",\"use_potential_placements_chance\":-0.1}",
            "{\"type\":\"minecraft:geode\"," + geode.replace("[\"minecraft:stone\"]", "[]") + "}",
            "{\"type\":\"minecraft:geode\"," + geode.replace("\"minecraft:bedrock\"", "\"minecraft:nonexistent\"") + "}",
            "{\"type\":\"minecraft:geode\"," + geode.replace("\"filling_provider\":{\"id\":\"minecraft:air\"}", "\"filling_provider\":5") + "}",
            "{\"type\":\"minecraft:speleothem\"," + spel + ",\"chance_of_taller_generation\":1.1}",
            "{\"type\":\"minecraft:speleothem\"," + spel + ",\"chance_of_directional_spread\":-0.5}",
            "{\"type\":\"minecraft:speleothem\"," + spel.replace(",\"pointed_block\":\"minecraft:pointed_dripstone\"", "") + "}",
            "{\"type\":\"minecraft:speleothem\"," + spel.replace("minecraft:dripstone_block", "minecraft:nope") + "}",
            "{\"type\":\"minecraft:speleothem_cluster\"," + cluster.replace("\"floor_to_ceiling_search_range\":12", "\"floor_to_ceiling_search_range\":0") + "}",
            "{\"type\":\"minecraft:speleothem_cluster\"," + cluster.replace("\"floor_to_ceiling_search_range\":12", "\"floor_to_ceiling_search_range\":513") + "}",
            "{\"type\":\"minecraft:speleothem_cluster\"," + cluster.replace("\"radius\":3", "\"radius\":129") + "}",
            "{\"type\":\"minecraft:speleothem_cluster\"," + cluster.replace("\"height\":3", "\"height\":0") + "}",
            "{\"type\":\"minecraft:speleothem_cluster\"," + cluster.replace("\"height_deviation\":3", "\"height_deviation\":0") + "}",
            "{\"type\":\"minecraft:speleothem_cluster\"," + cluster.replace("\"speleothem_block_layer_thickness\":2", "\"speleothem_block_layer_thickness\":129") + "}",
            "{\"type\":\"minecraft:speleothem_cluster\"," + cluster.replace("\"density\":0.5", "\"density\":2.5") + "}",
            "{\"type\":\"minecraft:speleothem_cluster\"," + cluster.replace("\"wetness\":0.5", "\"wetness\":-1.0") + "}",
            "{\"type\":\"minecraft:speleothem_cluster\"," + cluster.replace("\"max_distance_from_edge_affecting_chance_of_speleothem\":3", "\"max_distance_from_edge_affecting_chance_of_speleothem\":65") + "}",
            "{\"type\":\"minecraft:speleothem_cluster\"," + cluster.replace(",\"max_distance_from_center_affecting_height_bias\":8", "") + "}",
            "{\"type\":\"minecraft:large_dripstone\"," + large.replace("\"max_column_radius_to_cave_height_ratio\":0.5", "\"max_column_radius_to_cave_height_ratio\":0.05") + "}",
            "{\"type\":\"minecraft:large_dripstone\"," + large.replace("\"stalactite_bluntness\":1.0", "\"stalactite_bluntness\":10.5") + "}",
            "{\"type\":\"minecraft:large_dripstone\"," + large.replace("\"stalagmite_bluntness\":1.0", "\"stalagmite_bluntness\":0.05") + "}",
            "{\"type\":\"minecraft:large_dripstone\"," + large.replace("\"column_radius\":3", "\"column_radius\":17") + "}",
            "{\"type\":\"minecraft:large_dripstone\"," + large.replace("\"column_radius\":3", "\"column_radius\":0") + "}",
            "{\"type\":\"minecraft:large_dripstone\"," + large.replace("\"min_radius_for_wind\":2", "\"min_radius_for_wind\":101") + "}",
            "{\"type\":\"minecraft:large_dripstone\"," + large.replace("\"min_bluntness_for_wind\":1.0", "\"min_bluntness_for_wind\":5.5") + "}",
            "{\"type\":\"minecraft:large_dripstone\"," + large.replace("\"wind_speed\":1.0", "\"wind_speed\":3.0") + "}",
            "{\"type\":\"minecraft:large_dripstone\"," + large + ",\"floor_to_ceiling_search_range\":0}",
            "{\"type\":\"minecraft:large_dripstone\"," + large.replace(",\"height_scale\":1.0", "") + "}",
            "{\"type\":\"minecraft:end_gateway\"}",
            "{\"type\":\"minecraft:end_gateway\",\"exact\":true,\"exit\":[1,2]}",
            "{\"type\":\"minecraft:end_gateway\",\"exact\":\"yes\"}",
            "{\"type\":\"minecraft:end_spike\"}",
            "{\"type\":\"minecraft:end_spike\",\"spikes\":[],\"crystal_beam_target\":[1,2]}",
            "{\"type\":\"minecraft:end_spike\",\"spikes\":[{\"centerX\":\"a\"}]}",
            "{\"type\":\"minecraft:end_spike\",\"spikes\":{}}",
            "{\"type\":\"minecraft:end_podium\",\"active\":3}",
            "{\"type\":\"minecraft:spike\",\"state\":\"minecraft:stone\",\"can_replace\":{\"type\":\"minecraft:solid\"}}",
            "{\"type\":\"minecraft:spike\",\"state\":\"minecraft:stone\",\"can_place_on\":{\"type\":\"minecraft:solid\"}}",
            "{\"type\":\"minecraft:spike\",\"can_place_on\":{\"type\":\"minecraft:solid\"},\"can_replace\":{\"type\":\"minecraft:solid\"}}",
            "{\"type\":\"minecraft:spike\",\"state\":\"minecraft:stone\",\"can_place_on\":{\"type\":\"minecraft:nope\"},\"can_replace\":{\"type\":\"minecraft:solid\"}}",
            "{\"type\":\"minecraft:monster_room\",\"extra\":1}".replace(",\"extra\":1", "").replace("monster_room", "monster_roomx"),
        };
    }

    static Feature decode(JsonObject json) {
        var ops = RegistryOps.create(JsonOps.INSTANCE, FeatureOracleBase.ACCESS);
        var r = Feature.DIRECT_CODEC.parse(ops, json);
        if (r.error().isPresent() && System.getProperty("ugdebug") != null) System.err.println("DECODE ERROR: " + r.error().get().message());
        return r.result().orElse(null);
    }

    // ---- scenario scripts ----
    static String st(BlockState s) {
        return BlockStateParser.serialize(s);
    }

    static int rnd(Random r, int lo, int hi) {
        return lo + r.nextInt(hi - lo + 1);
    }

    static String fill(int x1, int y1, int z1, int x2, int y2, int z2, String state) {
        return "fill " + x1 + " " + y1 + " " + z1 + " " + x2 + " " + y2 + " " + z2 + " " + state;
    }

    /** One scenario script: preparation fills and placements (`place x y z randomSeed`). */
    static List<String> script(Random r, Case c, long seed, int cx, int cz) {
        List<String> ops = new ArrayList<>();
        int bx = cx * 16, bz = cz * 16;
        int[] cav = null;
        int places = c.kind.equals("geode") ? rnd(r, 3, 5) : rnd(r, 5, 9);
        switch (c.kind) {
            case "speleothem", "speleothem_cluster", "large_dripstone", "geode" -> {
                int n = rnd(r, 1, 3);
                int[][] cavs = new int[n][];
                for (int i = 0; i < n; i++) {
                    int w = rnd(r, 5, 22), d = rnd(r, 5, 22), h = rnd(r, 4, 16);
                    int x0 = bx + rnd(r, -8, 24 - w), z0 = bz + rnd(r, -8, 24 - d), y0 = rnd(r, -50, 30);
                    if (c.kind.equals("geode") && r.nextBoolean()) {
                        ops.add(fill(bx - 14, y0 - 20, bz - 14, bx + 29, y0 + 20, bz + 29, "minecraft:stone"));
                        cavs[i] = new int[] {x0, y0, z0, w, h, d};
                        continue;
                    }
                    ops.add(fill(x0, y0, z0, x0 + w, y0 + h, z0 + d, "minecraft:air"));
                    int liquid = rnd(r, 0, 3);
                    if (liquid == 1) ops.add(fill(x0, y0, z0, x0 + w, y0 + rnd(r, 0, h / 2), z0 + d, "minecraft:water"));
                    if (liquid == 2) ops.add(fill(x0 + 2, y0, z0 + 2, x0 + w - 2, y0, z0 + d - 2, "minecraft:lava"));
                    if (r.nextInt(3) == 0) ops.add(fill(x0 + w / 2, y0 + h / 2, z0 + d / 2, x0 + w / 2 + 1, y0 + h, z0 + d / 2 + 1, "minecraft:stone"));
                    if (r.nextInt(4) == 0) ops.add(fill(x0 + 1, y0 + h, z0 + 1, x0 + w - 1, y0 + h, z0 + d - 1, "minecraft:dripstone_block"));
                    cavs[i] = new int[] {x0, y0, z0, w, h, d};
                }
                for (int p = 0; p < places; p++) {
                    int x, y, z;
                    if (r.nextInt(10) < 8) {
                        cav = cavs[r.nextInt(n)];
                        x = cav[0] + rnd(r, 0, cav[3]); z = cav[2] + rnd(r, 0, cav[5]); y = cav[1] + rnd(r, 0, cav[4]);
                        x = Math.max(bx, Math.min(bx + 15, x)); z = Math.max(bz, Math.min(bz + 15, z));
                    } else {
                        x = bx + rnd(r, 0, 15); z = bz + rnd(r, 0, 15); y = rnd(r, -60, 80);
                    }
                    ops.add("place " + x + " " + y + " " + z + " " + r.nextInt(1 << 30));
                }
            }
            case "monster_room" -> {
                for (int p = 0; p < places; p++) {
                    int x = bx + rnd(r, 0, 15), z = bz + rnd(r, 0, 15), y = rnd(r, -40, 40);
                    ops.add(fill(x - 7, y - 3, z - 7, x + 7, y + 5, z + 7, r.nextInt(5) == 0 ? "minecraft:deepslate" : "minecraft:stone"));
                    int tunnels = rnd(r, 0, 4);
                    for (int t = 0; t < tunnels; t++) {
                        int off = rnd(r, -2, 2);
                        switch (r.nextInt(4)) {
                            case 0 -> ops.add(fill(x - 7, y, z + off, x - 1, y + 1, z + off, "minecraft:air"));
                            case 1 -> ops.add(fill(x + 1, y, z + off, x + 7, y + 1, z + off, "minecraft:air"));
                            case 2 -> ops.add(fill(x + off, y, z - 7, x + off, y + 1, z - 1, "minecraft:air"));
                            default -> ops.add(fill(x + off, y, z + 1, x + off, y + 1, z + 7, "minecraft:air"));
                        }
                    }
                    if (r.nextInt(3) == 0) ops.add(fill(x + rnd(r, -2, 2), y, z + rnd(r, -2, 2), x + rnd(r, -2, 2), y, z + rnd(r, -2, 2), r.nextBoolean() ? "minecraft:chest[facing=north,type=single,waterlogged=false]" : "minecraft:spawner"));
                    if (r.nextInt(6) == 0) ops.add(fill(x - 3, y - 1, z - 3, x + 3, y - 1, z + 3, "minecraft:bedrock"));
                    if (r.nextInt(6) == 0) ops.add(fill(x, y, z, x, y, z, "minecraft:spawner"));
                    ops.add("place " + x + " " + y + " " + z + " " + r.nextInt(1 << 30));
                }
            }
            case "spike" -> {
                for (int i = 0; i < 2; i++) {
                    int x0 = bx + rnd(r, -8, 14), z0 = bz + rnd(r, -8, 14);
                    for (int x = x0; x < x0 + 12; x++)
                        for (int z = z0; z < z0 + 12; z++) {
                            int top = FeatureOracleBase.surfaceTop(seed, x, z);
                            ops.add(fill(x, top - 1, z, x, top - 1, z, "minecraft:snow_block"));
                        }
                }
                for (int p = 0; p < places; p++) {
                    int x = bx + rnd(r, 0, 15), z = bz + rnd(r, 0, 15);
                    ops.add("place " + x + " " + (FeatureOracleBase.surfaceTop(seed, x, z) + rnd(r, 0, 20)) + " " + z + " " + r.nextInt(1 << 30));
                }
            }
            case "end_spike" -> {
                for (int p = 0; p < 2; p++) {
                    ops.add("place " + (bx + rnd(r, 0, 15)) + " " + rnd(r, -64, 100) + " " + (bz + rnd(r, 0, 15)) + " " + r.nextInt(1 << 30));
                    if (r.nextInt(3) == 0) ops.add("place " + (bx - 16) + " 60 " + bz + " " + r.nextInt(1 << 30));
                }
            }
            default -> {
                for (int p = 0; p < places; p++) {
                    ops.add("place " + (bx + rnd(r, 0, 15)) + " " + rnd(r, -55, 100) + " " + (bz + rnd(r, 0, 15)) + " " + r.nextInt(1 << 30));
                }
            }
        }
        return ops;
    }

    static String exec(UgRegion region, Feature feature, String op) {
        String[] t = op.split(" ");
        if (t[0].equals("fill")) {
            BlockState state;
            try {
                state = BlockStateParser.parseForBlock(FeatureOracleBase.ACCESS.lookupOrThrow(Registries.BLOCK), t[7], false).blockState();
            } catch (Exception e) {
                throw new RuntimeException(e);
            }
            int x1 = Integer.parseInt(t[1]), y1 = Integer.parseInt(t[2]), z1 = Integer.parseInt(t[3]), x2 = Integer.parseInt(t[4]), y2 = Integer.parseInt(t[5]), z2 = Integer.parseInt(t[6]);
            for (int y = Math.min(y1, y2); y <= Math.max(y1, y2); y++)
                for (int z = Math.min(z1, z2); z <= Math.max(z1, z2); z++)
                    for (int x = Math.min(x1, x2); x <= Math.max(x1, x2); x++) region.setBlock(new BlockPos(x, y, z), state, 2, 512);
            return "ok";
        }
        BlockPos origin = new BlockPos(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3]));
        RandomSource random = new XoroshiroRandomSource(Long.parseLong(t[4]));
        boolean placed = feature.place(region, FeatureOracleBase.generator(), random, origin);
        return placed + " " + random.nextLong();
    }

    static void scenarios(PrintWriter out, Case c, Feature feature, int idx) throws Exception {
        Random r = new Random(c.id.hashCode() * 31L + c.json.toString().hashCode());
        int n = c.kind.equals("end_spike") ? 3 : 5;
        for (int s = 0; s < n; s++) {
            long seed = r.nextLong();
            int cx = rnd(r, -5, 5), cz = rnd(r, -5, 5);
            if (c.kind.equals("end_spike")) {
                List<EndSpikeFeature.EndSpike> spikes = ((EndSpikeFeature) feature).spikes();
                if (spikes.isEmpty()) {
                    UgRegion probe = newRegion(seed, 0, 0);
                    spikes = EndSpikeFeature.getSpikesForLevel(probe);
                    var sp = spikes.get(r.nextInt(spikes.size()));
                    cx = sp.getCenterX() >> 4;
                    cz = sp.getCenterZ() >> 4;
                } else {
                    cx = 0;
                    cz = 0;
                }
            }
            UgRegion region = newRegion(seed, cx, cz);
            FeatureOracleBase.Snapshot before;
            out.println("SCENARIO " + c.id + " " + s + " " + seed + " " + cx + " " + cz);
            List<String> ops = script(r, c, seed, cx, cz);
            List<String> sorted = new ArrayList<>();
            for (String op : ops) if (op.startsWith("fill")) sorted.add(op);
            for (String op : sorted) {
                out.println("> " + op);
                out.println("= " + exec(region, feature, op));
            }
            // the dump lists the blocks changed since the snapshot: take it after the terrain preparation
            before = FeatureOracleBase.snapshot(region);
            out.println("SNAP");
            for (String op : ops) {
                if (op.startsWith("fill")) continue;
                out.println("> " + op);
                out.println("= " + exec(region, feature, op));
            }
            for (String line : FeatureOracleBase.dump(region, before)) out.println("D " + line.replaceAll("UUID:\\[I;[-0-9,]+\\]", "UUID:[I;0]"));
            out.println("END");
        }
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        PrintWriter out = new PrintWriter(new BufferedWriter(new FileWriter(args[0])));
        List<Case> cases = new ArrayList<>();
        Path dir = Path.of("ref/src/data/minecraft/worldgen/feature");
        List<Path> files;
        try (var s = Files.walk(dir)) { files = s.filter(Files::isRegularFile).sorted().toList(); }
        for (Path f : files) {
            JsonElement root = JsonParser.parseString(Files.readString(f));
            List<JsonObject> found = new ArrayList<>();
            collect(f.getFileName().toString(), root, found);
            String base = dir.relativize(f).toString().replace(".json", "");
            for (int i = 0; i < found.size(); i++) cases.add(new Case(base + "#" + i, found.get(i).get("type").getAsString().replace("minecraft:", ""), found.get(i)));
        }
        String[] vs = variants();
        for (int i = 0; i < vs.length; i++) cases.add(new Case("variant#" + i, obj(vs[i]).get("type").getAsString().replace("minecraft:", ""), obj(vs[i])));
        // the same feature JSON reached through different files is only exercised once
        Set<String> seen = new HashSet<>();
        List<Case> unique = new ArrayList<>();
        for (Case c : cases) if (seen.add(c.json.toString())) unique.add(c);
        for (String b : bad()) {
            JsonObject j = obj(b);
            if (decode(j) != null) throw new IllegalStateException("expected a codec error: " + b);
            out.println("BAD " + j);
            out.println("= ERR");
        }
        int idx = 0;
        for (Case c : unique) {
            Feature feature = decode(c.json);
            if (feature == null) throw new IllegalStateException("case " + c.id + " does not decode: " + c.json);
            out.println("CASE " + c.id + " " + c.kind + " " + c.json);
            scenarios(out, c, feature, idx++);
        }
        out.close();
        System.out.println("cases: " + unique.size());
    }
}
