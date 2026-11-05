import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.*;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.RegistryOps;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.feature.Feature;

/**
 * Oracle of crates/mc-game/tests/feat_ores_golden.rs: every vanilla `worldgen/feature` entry of the types owned by FeatOres
 * (ore, scattered_ore, disk, block_blob, netherrack_replace_blobs, lake, spring_feature, iceberg, the four selectors,
 * single_block_pillar, stepped_column_cluster) plus a few inline features (JSON in {@link #INLINE}) placed at many origins/seeds
 * over the synthetic terrain of FeatureOracleBase, and the accept/reject outcome of codec edge cases.
 * Usage (repo root): scripts/vanilla-java.sh crates/mc-game/tests/oracle/FeatOresOracle.java crates/mc-game/tests/fixtures/feat_ores.txt
 *
 * Output: {@code CODEC <ok|err> <json>} lines, then per scenario {@code S <name> <seed> <rng> <cx> <cz> <x> <y> <z>},
 * {@code R <result>}, {@code F <next long of the feature random>}, {@code D <dump line>}..., {@code END}. Dump lines are the
 * lines of FeatureOracleBase.dump that differ from the dump before the feature ran (plus the region random line).
 */
public class FeatOresOracle {
    static final Set<String> TYPES = Set.of("ore", "scattered_ore", "disk", "block_blob", "netherrack_replace_blobs", "lake", "spring_feature", "iceberg",
        "random_selector", "simple_random_selector", "random_boolean_selector", "weighted_random_selector", "single_block_pillar", "stepped_column_cluster", "fill_layer");

    /** name -> feature JSON that has no data-pack entry of its own (or variants that exercise other branches). */
    static final String[][] INLINE = {
        {"inline_fill_layer", "{\"type\":\"minecraft:fill_layer\",\"height\":70,\"state\":{\"id\":\"minecraft:stone\"}}"},
        {"inline_fill_layer_water", "{\"type\":\"minecraft:fill_layer\",\"height\":60,\"state\":{\"id\":\"minecraft:water\"}}"},
        {"inline_pillar_up", "{\"type\":\"minecraft:single_block_pillar\",\"block\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:basalt\",\"properties\":{\"axis\":\"y\"}}},\"direction\":\"up\",\"chance_to_continue\":0.8}"},
        {"inline_pillar_down_cap", "{\"type\":\"minecraft:single_block_pillar\",\"block\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:stone\"}},\"can_replace\":{\"type\":\"minecraft:matching_blocks\",\"blocks\":[\"minecraft:air\",\"minecraft:water\"]},\"direction\":\"down\",\"chance_to_continue\":0.9,\"cap_feature\":{\"feature\":{\"type\":\"minecraft:fill_layer\",\"height\":10,\"state\":{\"id\":\"minecraft:gold_block\"}},\"placement\":[]}}"},
        {"inline_lake_water", "{\"type\":\"minecraft:lake\",\"fluid\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:water\"}},\"barrier\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:dirt\"}},\"can_place_feature\":{\"type\":\"minecraft:true\"},\"can_replace_with_air_or_fluid\":{\"type\":\"minecraft:true\"},\"can_replace_with_barrier\":{\"type\":\"minecraft:true\"}}"},
        {"inline_ore_big", "{\"type\":\"minecraft:ore\",\"size\":64,\"discard_chance_on_air_exposure\":0.5,\"targets\":[{\"target\":{\"predicate_type\":\"minecraft:tag_match\",\"tag\":\"minecraft:stone_ore_replaceables\"},\"state\":{\"id\":\"minecraft:diamond_ore\"}},{\"target\":{\"predicate_type\":\"minecraft:tag_match\",\"tag\":\"minecraft:deepslate_ore_replaceables\"},\"state\":{\"id\":\"minecraft:deepslate_diamond_ore\"}}]}"},
        {"inline_ore_air_always", "{\"type\":\"minecraft:ore\",\"size\":20,\"discard_chance_on_air_exposure\":1.0,\"targets\":[{\"target\":{\"predicate_type\":\"minecraft:always_true\"},\"state\":{\"id\":\"minecraft:glowstone\"}}]}"},
        {"inline_scattered_air", "{\"type\":\"minecraft:scattered_ore\",\"size\":12,\"discard_chance_on_air_exposure\":0.3,\"targets\":[{\"target\":{\"predicate_type\":\"minecraft:always_true\"},\"state\":{\"id\":\"minecraft:glowstone\"}}]}"},
        {"inline_blob_air", "{\"type\":\"minecraft:netherrack_replace_blobs\",\"target\":{\"id\":\"minecraft:stone\"},\"state\":{\"id\":\"minecraft:blackstone\"},\"radius\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":3,\"max_inclusive\":7}}"},
        {"inline_spring_water", "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:water\",\"properties\":{\"falling\":\"true\"}},\"requires_block_below\":false,\"rock_count\":2,\"hole_count\":2,\"valid_blocks\":\"#minecraft:base_stone_overworld\"}"},
        {"inline_stepped", "{\"type\":\"minecraft:stepped_column_cluster\",\"block\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:basalt\",\"properties\":{\"axis\":\"y\"}}},\"continue_through\":{\"type\":\"minecraft:matching_blocks\",\"blocks\":[\"minecraft:basalt\"]},\"can_replace\":{\"type\":\"minecraft:matching_blocks\",\"blocks\":[\"minecraft:air\",\"minecraft:water\"]},\"cannot_place_on\":[\"minecraft:bedrock\"],\"cluster_reach\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":1,\"max_inclusive\":5},\"column_count\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":10,\"max_inclusive\":40},\"column_reach\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0,\"max_inclusive\":2},\"height\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":2,\"max_inclusive\":8}}"},
    };

    static final String[] CODEC_CASES = {
        "{\"type\":\"minecraft:fill_layer\",\"height\":0,\"state\":\"minecraft:stone\"}",
        "{\"type\":\"minecraft:fill_layer\",\"height\":4064,\"state\":{\"id\":\"minecraft:stone\"}}",
        "{\"type\":\"minecraft:fill_layer\",\"height\":4065,\"state\":{\"id\":\"minecraft:stone\"}}",
        "{\"type\":\"minecraft:fill_layer\",\"height\":-1,\"state\":{\"id\":\"minecraft:stone\"}}",
        "{\"type\":\"minecraft:fill_layer\",\"height\":5}",
        "{\"type\":\"minecraft:ore\",\"size\":64,\"discard_chance_on_air_exposure\":1.0,\"targets\":[]}",
        "{\"type\":\"minecraft:ore\",\"size\":65,\"discard_chance_on_air_exposure\":0.0,\"targets\":[]}",
        "{\"type\":\"minecraft:ore\",\"size\":-1,\"discard_chance_on_air_exposure\":0.0,\"targets\":[]}",
        "{\"type\":\"minecraft:ore\",\"size\":3,\"discard_chance_on_air_exposure\":1.5,\"targets\":[]}",
        "{\"type\":\"minecraft:ore\",\"size\":3,\"discard_chance_on_air_exposure\":-0.1,\"targets\":[]}",
        "{\"type\":\"minecraft:ore\",\"size\":3,\"targets\":[]}",
        "{\"type\":\"minecraft:ore\",\"size\":3,\"discard_chance_on_air_exposure\":0.5}",
        "{\"type\":\"minecraft:ore\",\"size\":3,\"discard_chance_on_air_exposure\":0.5,\"targets\":[{\"state\":{\"id\":\"minecraft:stone\"}}]}",
        "{\"type\":\"minecraft:ore\",\"size\":3,\"discard_chance_on_air_exposure\":0.5,\"targets\":[{\"target\":{\"predicate_type\":\"minecraft:always_true\"}}]}",
        "{\"type\":\"minecraft:ore\",\"size\":3,\"discard_chance_on_air_exposure\":0.5,\"targets\":[{\"target\":{\"predicate_type\":\"minecraft:nope\"},\"state\":{\"id\":\"minecraft:stone\"}}]}",
        "{\"type\":\"minecraft:ore\",\"size\":3,\"discard_chance_on_air_exposure\":0.5,\"targets\":[{\"target\":{\"predicate_type\":\"minecraft:block_match\",\"block\":\"minecraft:stone\"},\"state\":{\"id\":\"minecraft:nope\"}}]}",
        "{\"type\":\"minecraft:scattered_ore\",\"size\":3,\"discard_chance_on_air_exposure\":0.5,\"targets\":[]}",
        "{\"type\":\"minecraft:scattered_ore\",\"size\":65,\"discard_chance_on_air_exposure\":0.5,\"targets\":[]}",
        "{\"type\":\"minecraft:disk\",\"state_provider\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:sand\"}},\"target\":{\"type\":\"minecraft:true\"},\"radius\":4,\"half_height\":2}",
        "{\"type\":\"minecraft:disk\",\"state_provider\":{\"id\":\"minecraft:sand\"},\"target\":{\"type\":\"minecraft:true\"},\"radius\":4,\"half_height\":2}",
        "{\"type\":\"minecraft:disk\",\"state_provider\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:sand\"}},\"target\":{\"type\":\"minecraft:true\"},\"radius\":9,\"half_height\":2}",
        "{\"type\":\"minecraft:disk\",\"state_provider\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:sand\"}},\"target\":{\"type\":\"minecraft:true\"},\"radius\":-1,\"half_height\":2}",
        "{\"type\":\"minecraft:disk\",\"state_provider\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:sand\"}},\"target\":{\"type\":\"minecraft:true\"},\"radius\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":2,\"max_inclusive\":8},\"half_height\":4}",
        "{\"type\":\"minecraft:disk\",\"state_provider\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:sand\"}},\"target\":{\"type\":\"minecraft:true\"},\"radius\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":2,\"max_inclusive\":9},\"half_height\":4}",
        "{\"type\":\"minecraft:disk\",\"state_provider\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:sand\"}},\"target\":{\"type\":\"minecraft:true\"},\"radius\":4,\"half_height\":5}",
        "{\"type\":\"minecraft:disk\",\"state_provider\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:sand\"}},\"radius\":4,\"half_height\":2}",
        "{\"type\":\"minecraft:block_blob\",\"state\":{\"id\":\"minecraft:mossy_cobblestone\"},\"can_place_on\":{\"type\":\"minecraft:true\"}}",
        "{\"type\":\"minecraft:block_blob\",\"state\":{\"id\":\"minecraft:mossy_cobblestone\"}}",
        "{\"type\":\"minecraft:block_blob\",\"state\":\"minecraft:mossy_cobblestone\",\"can_place_on\":{\"type\":\"minecraft:true\"}}",
        "{\"type\":\"minecraft:netherrack_replace_blobs\",\"target\":{\"id\":\"minecraft:netherrack\"},\"state\":{\"id\":\"minecraft:basalt\"},\"radius\":12}",
        "{\"type\":\"minecraft:netherrack_replace_blobs\",\"target\":{\"id\":\"minecraft:netherrack\"},\"state\":{\"id\":\"minecraft:basalt\"},\"radius\":13}",
        "{\"type\":\"minecraft:netherrack_replace_blobs\",\"target\":{\"id\":\"minecraft:netherrack\"},\"state\":{\"id\":\"minecraft:basalt\"},\"radius\":-1}",
        "{\"type\":\"minecraft:netherrack_replace_blobs\",\"target\":{\"id\":\"minecraft:netherrack\"},\"radius\":3}",
        "{\"type\":\"minecraft:lake\",\"fluid\":{\"id\":\"minecraft:lava\"},\"barrier\":{\"id\":\"minecraft:stone\"},\"can_place_feature\":{\"type\":\"minecraft:true\"},\"can_replace_with_air_or_fluid\":{\"type\":\"minecraft:true\"},\"can_replace_with_barrier\":{\"type\":\"minecraft:true\"}}",
        "{\"type\":\"minecraft:lake\",\"fluid\":{\"id\":\"minecraft:lava\"},\"barrier\":{\"id\":\"minecraft:stone\"},\"can_place_feature\":{\"type\":\"minecraft:true\"},\"can_replace_with_air_or_fluid\":{\"type\":\"minecraft:true\"}}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:lava\"},\"valid_blocks\":\"minecraft:stone\"}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:lava\",\"properties\":{\"falling\":\"true\"}},\"valid_blocks\":[\"minecraft:stone\",\"minecraft:dirt\"]}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:lava\",\"properties\":{\"falling\":\"maybe\",\"bogus\":\"x\"}},\"valid_blocks\":\"#minecraft:dirt\"}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:lava\",\"properties\":5},\"valid_blocks\":\"#minecraft:dirt\"}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:flowing_water\",\"properties\":{\"level\":\"3\",\"falling\":\"false\"}},\"valid_blocks\":\"#minecraft:dirt\",\"rock_count\":-5,\"hole_count\":99}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:empty\"},\"valid_blocks\":\"#minecraft:dirt\"}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":\"minecraft:lava\",\"valid_blocks\":\"#minecraft:dirt\"}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:stone\"},\"valid_blocks\":\"#minecraft:dirt\"}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:lava\"}}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:lava\"},\"valid_blocks\":\"minecraft:nope\"}",
        "{\"type\":\"minecraft:spring_feature\",\"state\":{\"id\":\"minecraft:lava\"},\"requires_block_below\":\"yes\",\"valid_blocks\":\"minecraft:stone\"}",
        "{\"type\":\"minecraft:iceberg\",\"state\":{\"id\":\"minecraft:packed_ice\"}}",
        "{\"type\":\"minecraft:iceberg\"}",
        "{\"type\":\"minecraft:iceberg\",\"state\":{\"id\":\"minecraft:nope\"}}",
        "{\"type\":\"minecraft:single_block_pillar\",\"block\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:stone\"}},\"direction\":\"up\"}",
        "{\"type\":\"minecraft:single_block_pillar\",\"block\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:stone\"}},\"direction\":\"north\"}",
        "{\"type\":\"minecraft:single_block_pillar\",\"block\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:stone\"}},\"direction\":\"down\",\"chance_to_continue\":1.5}",
        "{\"type\":\"minecraft:single_block_pillar\",\"block\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:stone\"}},\"direction\":\"down\",\"chance_to_continue\":0.0,\"can_replace\":{\"type\":\"minecraft:true\"}}",
        "{\"type\":\"minecraft:single_block_pillar\",\"block\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:stone\"}}}",
        "{\"type\":\"minecraft:single_block_pillar\",\"block\":{\"type\":\"minecraft:simple\",\"state\":{\"id\":\"minecraft:stone\"}},\"direction\":\"up\",\"cap_feature\":\"minecraft:nope\"}",
        "{\"type\":\"minecraft:simple_random_selector\",\"features\":[]}",
        "{\"type\":\"minecraft:simple_random_selector\",\"features\":\"#minecraft:nope\"}",
        "{\"type\":\"minecraft:simple_random_selector\",\"features\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]}}",
        "{\"type\":\"minecraft:simple_random_selector\",\"features\":[{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]},{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]}]}",
        "{\"type\":\"minecraft:simple_random_selector\",\"features\":[\"minecraft:nope\"]}",
        "{\"type\":\"minecraft:simple_random_selector\"}",
        "{\"type\":\"minecraft:random_boolean_selector\",\"feature_true\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]},\"feature_false\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]}}",
        "{\"type\":\"minecraft:random_boolean_selector\",\"feature_true\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]}}",
        "{\"type\":\"minecraft:random_selector\",\"features\":[],\"default\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]}}",
        "{\"type\":\"minecraft:random_selector\",\"features\":[{\"feature\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]},\"chance\":1.0}],\"default\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]}}",
        "{\"type\":\"minecraft:random_selector\",\"features\":[{\"feature\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]},\"chance\":1.1}],\"default\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]}}",
        "{\"type\":\"minecraft:random_selector\",\"features\":[{\"feature\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]}}],\"default\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]}}",
        "{\"type\":\"minecraft:random_selector\",\"features\":[]}",
        "{\"type\":\"minecraft:weighted_random_selector\",\"features\":[]}",
        "{\"type\":\"minecraft:weighted_random_selector\",\"features\":[{\"data\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]},\"weight\":0}]}",
        "{\"type\":\"minecraft:weighted_random_selector\",\"features\":[{\"data\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]},\"weight\":3},{\"data\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]},\"weight\":1}]}",
        "{\"type\":\"minecraft:weighted_random_selector\",\"features\":[{\"data\":{\"feature\":{\"type\":\"minecraft:no_op\"},\"placement\":[]},\"weight\":-1}]}",
        "{\"type\":\"minecraft:weighted_random_selector\",\"features\":[{\"weight\":1}]}",
        "{\"type\":\"minecraft:weighted_random_selector\"}",
        "{\"type\":\"minecraft:stepped_column_cluster\",\"block\":{\"id\":\"minecraft:basalt\"},\"continue_through\":{\"type\":\"minecraft:true\"},\"can_replace\":{\"type\":\"minecraft:true\"},\"cannot_place_on\":\"minecraft:bedrock\",\"cluster_reach\":13,\"column_count\":150,\"column_reach\":3,\"height\":10}",
        "{\"type\":\"minecraft:stepped_column_cluster\",\"block\":{\"id\":\"minecraft:basalt\"},\"continue_through\":{\"type\":\"minecraft:true\"},\"can_replace\":{\"type\":\"minecraft:true\"},\"cannot_place_on\":\"minecraft:bedrock\",\"cluster_reach\":14,\"column_count\":150,\"column_reach\":3,\"height\":10}",
        "{\"type\":\"minecraft:stepped_column_cluster\",\"block\":{\"id\":\"minecraft:basalt\"},\"continue_through\":{\"type\":\"minecraft:true\"},\"can_replace\":{\"type\":\"minecraft:true\"},\"cannot_place_on\":\"minecraft:bedrock\",\"cluster_reach\":13,\"column_count\":0,\"column_reach\":3,\"height\":10}",
        "{\"type\":\"minecraft:stepped_column_cluster\",\"block\":{\"id\":\"minecraft:basalt\"},\"continue_through\":{\"type\":\"minecraft:true\"},\"can_replace\":{\"type\":\"minecraft:true\"},\"cannot_place_on\":\"minecraft:bedrock\",\"cluster_reach\":13,\"column_count\":150,\"column_reach\":4,\"height\":10}",
        "{\"type\":\"minecraft:stepped_column_cluster\",\"block\":{\"id\":\"minecraft:basalt\"},\"continue_through\":{\"type\":\"minecraft:true\"},\"can_replace\":{\"type\":\"minecraft:true\"},\"cannot_place_on\":\"minecraft:bedrock\",\"cluster_reach\":13,\"column_count\":150,\"column_reach\":3,\"height\":11}",
        "{\"type\":\"minecraft:stepped_column_cluster\",\"block\":{\"id\":\"minecraft:basalt\"},\"continue_through\":{\"type\":\"minecraft:true\"},\"cannot_place_on\":\"minecraft:bedrock\",\"cluster_reach\":13,\"column_count\":150,\"column_reach\":3,\"height\":10}",
    };

    static final int SCENARIOS = 12;
    static final String[] PREPS = {"none", "none", "none", "none", "none", "nether", "nether", "plateau", "plateau", "cell", "cell", "plateau"};

    static net.minecraft.world.level.block.state.BlockState st(net.minecraft.world.level.block.Block b) {
        return b.defaultBlockState();
    }

    /**
     * Deterministic terrain edit before the feature runs (mirrored in feat_ores_golden.rs): "nether" turns the rock around the
     * origin into nether blocks, "plateau" cuts a flat 25x25 shelf (solid below the origin, air from it up), "cell" builds a
     * stone cube around the origin with a random subset of its neighbours opened (springs).
     */
    static void prep(FeatureOracleBase.Region region, String kind, long seed, BlockPos o) {
        switch (kind) {
            case "nether" -> {
                for (int x = o.getX() - 20; x <= o.getX() + 20; x++)
                    for (int z = o.getZ() - 20; z <= o.getZ() + 20; z++)
                        for (int y = Math.max(o.getY() - 20, FeatureOracleBase.MIN_Y + 1); y <= Math.min(o.getY() + 20, FeatureOracleBase.MIN_Y + FeatureOracleBase.HEIGHT - 1); y++) {
                            BlockPos p = new BlockPos(x, y, z);
                            var s = region.getBlockState(p);
                            if (s.is(net.minecraft.world.level.block.Blocks.STONE) || s.is(net.minecraft.world.level.block.Blocks.DEEPSLATE) || s.is(net.minecraft.world.level.block.Blocks.DIRT)
                                || s.is(net.minecraft.world.level.block.Blocks.GRASS_BLOCK) || s.is(net.minecraft.world.level.block.Blocks.SAND) || s.is(net.minecraft.world.level.block.Blocks.GRAVEL)) {
                                int c = FeatureOracleBase.hmod(FeatureOracleBase.h(seed, x, y, z), 10);
                                region.setBlock(p, st(c <= 5 ? net.minecraft.world.level.block.Blocks.NETHERRACK : c == 6 ? net.minecraft.world.level.block.Blocks.SOUL_SAND : c == 7 ? net.minecraft.world.level.block.Blocks.GRAVEL
                                    : c == 8 ? net.minecraft.world.level.block.Blocks.MAGMA_BLOCK : net.minecraft.world.level.block.Blocks.BLACKSTONE), 2, 512);
                            }
                        }
            }
            case "plateau" -> {
                for (int x = o.getX() - 12; x <= o.getX() + 12; x++)
                    for (int z = o.getZ() - 12; z <= o.getZ() + 12; z++)
                        for (int y = Math.max(o.getY() - 6, FeatureOracleBase.MIN_Y + 1); y <= Math.min(o.getY() + 5, FeatureOracleBase.MIN_Y + FeatureOracleBase.HEIGHT - 1); y++)
                            region.setBlock(new BlockPos(x, y, z), st(y < o.getY() ? net.minecraft.world.level.block.Blocks.STONE : net.minecraft.world.level.block.Blocks.AIR), 2, 512);
            }
            case "cell" -> {
                for (int dx = -1; dx <= 1; dx++)
                    for (int dy = -1; dy <= 1; dy++)
                        for (int dz = -1; dz <= 1; dz++)
                            region.setBlock(o.offset(dx, dy, dz), st(net.minecraft.world.level.block.Blocks.STONE), 2, 512);
                BlockPos[] around = {o.west(), o.east(), o.north(), o.south(), o.below()};
                for (int i = 0; i < around.length; i++)
                    if (FeatureOracleBase.hmod(FeatureOracleBase.h(seed, o.getX() + i, o.getY(), o.getZ()), 4) == 0) region.setBlock(around[i], st(net.minecraft.world.level.block.Blocks.AIR), 2, 512);
                if (FeatureOracleBase.hmod(FeatureOracleBase.h(seed, o.getX(), o.getY() + 9, o.getZ()), 3) == 0) region.setBlock(o, st(net.minecraft.world.level.block.Blocks.AIR), 2, 512);
            }
            default -> {}
        }
    }

    static String featureType(Feature f) {
        return BuiltInRegistries.FEATURE_TYPE.getKey(f.codec()).getPath();
    }

    static RandomSource rng(int kind, long seed) {
        return kind == 0 ? new XoroshiroRandomSource(seed) : new LegacyRandomSource(seed);
    }

    static void run(PrintWriter out, String name, Feature feature, long entryHash) throws Exception {
        Random r = new Random(entryHash);
        for (int i = 0; i < SCENARIOS; i++) {
            long seed = r.nextLong();
            int cx = r.nextInt(2000) - 1000, cz = r.nextInt(2000) - 1000;
            if (i < 2) { cx = r.nextInt(5) - 2; cz = r.nextInt(5) - 2; }
            int x = cx * 16 + r.nextInt(16), z = cz * 16 + r.nextInt(16);
            int top = FeatureOracleBase.surfaceTop(seed, x, z);
            int mode = r.nextInt(7);
            int y = switch (mode) {
                case 0 -> top;
                case 1 -> top - 1;
                case 2 -> top + 1 + r.nextInt(3);
                case 3 -> FeatureOracleBase.SEA_LEVEL - r.nextInt(8);
                case 4 -> top - 2 - r.nextInt(30);
                case 5 -> -60 + r.nextInt(130);
                default -> FeatureOracleBase.SEA_LEVEL - 1 - r.nextInt(3);
            };
            int rngKind = r.nextInt(2);
            long rngSeed = r.nextLong();
            String prep = PREPS[i];
            out.println("S " + name + " " + seed + " " + rngKind + " " + rngSeed + " " + cx + " " + cz + " " + x + " " + y + " " + z + " " + prep);
            FeatureOracleBase.Region region = FeatureOracleBase.newRegion(seed, cx, cz, ChunkStatus.FEATURES, 2);
            FeatureOracleBase.Snapshot before = FeatureOracleBase.snapshot(region);
            prep(region, prep, seed, new BlockPos(x, y, z));
            List<String> base = FeatureOracleBase.dump(region, before);
            RandomSource random = rng(rngKind, rngSeed);
            String result;
            try {
                result = String.valueOf(feature.place(region, FeatureOracleBase.generator(), random, new BlockPos(x, y, z)));
            } catch (Throwable t) {
                result = "EXC " + t.getClass().getSimpleName();
            }
            out.println("R " + result);
            out.println("F " + random.nextLong());
            Map<String, Integer> baseCount = new HashMap<>();
            for (String l : base) baseCount.merge(l, 1, Integer::sum);
            for (String l : FeatureOracleBase.dump(region, before)) {
                Integer c = baseCount.get(l);
                if (c != null && c > 0 && !l.startsWith("G ")) { baseCount.put(l, c - 1); continue; }
                out.println("D " + l);
            }
            out.println("END");
            out.flush();
        }
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        RegistryOps<JsonElement> ops = RegistryOps.create(JsonOps.INSTANCE, FeatureOracleBase.ACCESS);
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        for (String json : CODEC_CASES) {
            boolean ok = Feature.CODEC.parse(ops, JsonParser.parseString(json)).result().isPresent();
            out.println("CODEC " + (ok ? "ok" : "err") + " " + json);
        }
        var features = FeatureOracleBase.ACCESS.lookupOrThrow(Registries.FEATURE);
        List<Holder.Reference<Feature>> entries = new ArrayList<>(features.listElements().toList());
        entries.sort(Comparator.comparing(h -> h.key().identifier().toString()));
        for (Holder.Reference<Feature> e : entries) {
            String name = e.key().identifier().getPath();
            if (!TYPES.contains(featureType(e.value())) || (args.length > 1 && !name.contains(args[1]))) continue;
            run(out, name, e.value(), name.hashCode() * 31L + 7);
        }
        for (String[] inline : INLINE) {
            if (args.length > 1 && !inline[0].contains(args[1])) continue;
            Feature f = Feature.DIRECT_CODEC.parse(ops, JsonParser.parseString(inline[1])).getOrThrow();
            out.println("INLINE " + inline[0] + " " + inline[1]);
            run(out, inline[0], f, inline[0].hashCode() * 31L + 7);
        }
        out.close();
    }
}
