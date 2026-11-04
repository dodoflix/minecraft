import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.*;
import net.minecraft.core.RegistryAccess;
import net.minecraft.resources.RegistryOps;
import net.minecraft.world.level.levelgen.placement.PlacementModifier;

/**
 * Oracle of the codec cases of crates/mc-game/tests/placement_golden.rs: whether vanilla's {@code PlacementModifier.CODEC}
 * accepts each JSON. Usage (repo root):
 * scripts/vanilla-java.sh crates/mc-game/tests/oracle/PlacementCodecOracle.java crates/mc-game/tests/fixtures/placement_codec.txt
 * Output lines: {@code OK|ERR<TAB>json}.
 */
public class PlacementCodecOracle {
    static final String T = "\"type\":\"minecraft:";
    static final String ANY = "{" + T + "air\"}";
    static final String[] CASES = {
        // unit types ignore extra fields; unknown / foreign types are rejected
        "{" + T + "biome\"}", "{" + T + "biome\",\"x\":1}", "{" + T + "in_square\"}", "{" + T + "in_square\",\"x\":[1]}",
        "{" + T + "nope\"}", "{\"type\":\"nope\"}", "{\"type\":\"foo:in_square\"}", "{\"type\":1}", "{}", "[]", "\"minecraft:biome\"",
        "{" + T + "cuboid\",\"xz_size\":3,\"y_size\":4}",
        // count: IntProviders.codec(0, 4096)
        "{" + T + "count\",\"count\":0}", "{" + T + "count\",\"count\":4096}", "{" + T + "count\",\"count\":4097}",
        "{" + T + "count\",\"count\":-1}", "{" + T + "count\",\"count\":1.5}", "{" + T + "count\",\"count\":\"3\"}",
        "{" + T + "count\"}", "{" + T + "count\",\"count\":null}", "{" + T + "count\",\"count\":true}",
        "{" + T + "count\",\"count\":{\"min_inclusive\":0,\"max_inclusive\":4096,\"type\":\"minecraft:uniform\"}}",
        "{" + T + "count\",\"count\":{\"min_inclusive\":0,\"max_inclusive\":4097,\"type\":\"minecraft:uniform\"}}",
        "{" + T + "count\",\"count\":{\"min_inclusive\":-1,\"max_inclusive\":5,\"type\":\"minecraft:uniform\"}}",
        "{" + T + "count\",\"count\":{\"min_inclusive\":5,\"max_inclusive\":1,\"type\":\"minecraft:uniform\"}}",
        "{" + T + "count\",\"count\":{\"type\":\"minecraft:constant\",\"value\":7}}",
        "{" + T + "count\",\"count\":{\"type\":\"minecraft:constant\"}}",
        "{" + T + "count\",\"count\":{\"type\":\"minecraft:biased_to_bottom\",\"min_inclusive\":1,\"max_inclusive\":9}}",
        "{" + T + "count\",\"count\":{\"type\":\"minecraft:clamped\",\"min_inclusive\":1,\"max_inclusive\":9,\"source\":2}}",
        "{" + T + "count\",\"count\":{\"type\":\"minecraft:weighted_list\",\"distribution\":[{\"weight\":1,\"data\":3},{\"weight\":2,\"data\":4097}]}}",
        "{" + T + "count\",\"count\":{\"type\":\"minecraft:weighted_list\",\"distribution\":[]}}",
        "{" + T + "count\",\"count\":{\"type\":\"minecraft:nope\"}}",
        "{" + T + "count\",\"count\":2147483648}", "{" + T + "count\",\"count\":4294967296}",
        // count_on_every_layer: IntProviders.codec(0, 256)
        "{" + T + "count_on_every_layer\",\"count\":256}", "{" + T + "count_on_every_layer\",\"count\":257}", "{" + T + "count_on_every_layer\",\"count\":-1}",
        "{" + T + "count_on_every_layer\"}", "{" + T + "count_on_every_layer\",\"count\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":0,\"max_inclusive\":300}}",
        // rarity_filter: POSITIVE_INT
        "{" + T + "rarity_filter\",\"chance\":1}", "{" + T + "rarity_filter\",\"chance\":0}", "{" + T + "rarity_filter\",\"chance\":-5}",
        "{" + T + "rarity_filter\",\"chance\":2147483647}", "{" + T + "rarity_filter\",\"chance\":2.9}", "{" + T + "rarity_filter\",\"chance\":0.5}",
        "{" + T + "rarity_filter\"}", "{" + T + "rarity_filter\",\"chance\":\"2\"}", "{" + T + "rarity_filter\",\"chance\":[2]}",
        // random_chance: floatRange(0, 1)
        "{" + T + "random_chance\",\"chance\":0}", "{" + T + "random_chance\",\"chance\":1}", "{" + T + "random_chance\",\"chance\":0.25}",
        "{" + T + "random_chance\",\"chance\":1.0000001}", "{" + T + "random_chance\",\"chance\":-0.1}", "{" + T + "random_chance\"}",
        "{" + T + "random_chance\",\"chance\":\"0.5\"}", "{" + T + "random_chance\",\"chance\":1e0}", "{" + T + "random_chance\",\"chance\":1.00000001}",
        // surface_water_depth_filter: Codec.INT
        "{" + T + "surface_water_depth_filter\",\"max_water_depth\":0}", "{" + T + "surface_water_depth_filter\",\"max_water_depth\":-4}",
        "{" + T + "surface_water_depth_filter\"}", "{" + T + "surface_water_depth_filter\",\"max_water_depth\":3.7}",
        "{" + T + "surface_water_depth_filter\",\"max_water_depth\":\"1\"}", "{" + T + "surface_water_depth_filter\",\"max_water_depth\":3000000000}",
        // surface_relative_threshold_filter
        "{" + T + "surface_relative_threshold_filter\",\"heightmap\":\"OCEAN_FLOOR_WG\"}",
        "{" + T + "surface_relative_threshold_filter\",\"heightmap\":\"WORLD_SURFACE\",\"min_inclusive\":-5,\"max_inclusive\":5}",
        "{" + T + "surface_relative_threshold_filter\",\"heightmap\":\"WORLD_SURFACE\",\"min_inclusive\":5,\"max_inclusive\":-5}",
        "{" + T + "surface_relative_threshold_filter\",\"heightmap\":\"world_surface\"}",
        "{" + T + "surface_relative_threshold_filter\",\"heightmap\":\"NOPE\"}", "{" + T + "surface_relative_threshold_filter\"}",
        "{" + T + "surface_relative_threshold_filter\",\"heightmap\":\"MOTION_BLOCKING\",\"min_inclusive\":\"a\"}",
        "{" + T + "surface_relative_threshold_filter\",\"heightmap\":\"MOTION_BLOCKING\",\"max_inclusive\":null}",
        "{" + T + "surface_relative_threshold_filter\",\"heightmap\":\"MOTION_BLOCKING_NO_LEAVES\",\"min_inclusive\":2147483647}",
        // heightmap
        "{" + T + "heightmap\",\"heightmap\":\"MOTION_BLOCKING_NO_LEAVES\"}", "{" + T + "heightmap\",\"heightmap\":\"WORLD_SURFACE_WG\"}",
        "{" + T + "heightmap\",\"heightmap\":\"OCEAN_FLOOR\"}", "{" + T + "heightmap\",\"heightmap\":\"Ocean_Floor\"}",
        "{" + T + "heightmap\",\"heightmap\":0}", "{" + T + "heightmap\"}", "{" + T + "heightmap\",\"heightmap\":\"\"}",
        // height_range
        "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":{\"absolute\":-16},\"max_inclusive\":{\"absolute\":16}}}",
        "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:trapezoid\",\"min_inclusive\":{\"above_bottom\":8},\"max_inclusive\":{\"below_top\":8},\"plateau\":4}}",
        "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:trapezoid\",\"min_inclusive\":{\"above_bottom\":8},\"max_inclusive\":{\"below_top\":8}}}",
        "{" + T + "height_range\",\"height\":{\"absolute\":5}}", "{" + T + "height_range\",\"height\":{\"above_bottom\":5}}",
        "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:biased_to_bottom\",\"min_inclusive\":{\"absolute\":0},\"max_inclusive\":{\"absolute\":9},\"inner\":2}}",
        "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:biased_to_bottom\",\"min_inclusive\":{\"absolute\":0},\"max_inclusive\":{\"absolute\":9},\"inner\":0}}",
        "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:very_biased_to_bottom\",\"min_inclusive\":{\"absolute\":0},\"max_inclusive\":{\"absolute\":9},\"inner\":1}}",
        "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:weighted_list\",\"distribution\":[{\"weight\":1,\"data\":{\"absolute\":3}}]}}",
        "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:constant\",\"value\":{\"absolute\":3}}}",
        "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":{\"absolute\":-16}}}",
        "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":{\"absolute\":-16},\"max_inclusive\":{\"absolute\":16},\"extra\":1}}",
        "{" + T + "height_range\",\"height\":{\"absolute\":1,\"above_bottom\":2}}", "{" + T + "height_range\",\"height\":7}", "{" + T + "height_range\",\"height\":{}}",
        "{" + T + "height_range\"}", "{" + T + "height_range\",\"height\":{\"type\":\"minecraft:nope\"}}",
        // offset: IntProviders.codec(-16, 16)
        "{" + T + "offset\",\"x\":0,\"y\":1,\"z\":0}", "{" + T + "offset\",\"x\":-16,\"y\":16,\"z\":16}", "{" + T + "offset\",\"x\":-17,\"y\":0,\"z\":0}",
        "{" + T + "offset\",\"x\":0,\"y\":17,\"z\":0}", "{" + T + "offset\",\"x\":0,\"y\":0}", "{" + T + "offset\",\"y\":0,\"z\":0}",
        "{" + T + "offset\",\"x\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":-4,\"max_inclusive\":4},\"y\":0,\"z\":{\"type\":\"minecraft:trapezoid\",\"min\":-3,\"max\":3,\"plateau\":1}}",
        "{" + T + "offset\",\"x\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":-4,\"max_inclusive\":40},\"y\":0,\"z\":0}",
        "{" + T + "offset\",\"x\":{\"type\":\"minecraft:trapezoid\",\"min\":-3,\"max\":3,\"plateau\":9},\"y\":0,\"z\":0}",
        "{" + T + "offset\",\"x\":1,\"y\":2,\"z\":3,\"w\":4}",
        // noise_based_count
        "{" + T + "noise_based_count\",\"noise_to_count_ratio\":10,\"noise_factor\":80.0,\"noise_offset\":0.3}",
        "{" + T + "noise_based_count\",\"noise_to_count_ratio\":10,\"noise_factor\":80}", "{" + T + "noise_based_count\",\"noise_to_count_ratio\":10}",
        "{" + T + "noise_based_count\",\"noise_factor\":80.0}", "{" + T + "noise_based_count\",\"noise_to_count_ratio\":10.5,\"noise_factor\":80.0}",
        "{" + T + "noise_based_count\",\"noise_to_count_ratio\":-10,\"noise_factor\":0.0}",
        "{" + T + "noise_based_count\",\"noise_to_count_ratio\":10,\"noise_factor\":80.0,\"noise_offset\":\"x\"}",
        "{" + T + "noise_based_count\",\"noise_to_count_ratio\":10,\"noise_factor\":80.0,\"noise_offset\":null}",
        "{" + T + "noise_based_count\",\"noise_to_count_ratio\":\"10\",\"noise_factor\":80.0}",
        // noise_threshold_count
        "{" + T + "noise_threshold_count\",\"noise_level\":-0.8,\"below_noise\":15,\"above_noise\":4}",
        "{" + T + "noise_threshold_count\",\"noise_level\":-1,\"below_noise\":0,\"above_noise\":-3}", "{" + T + "noise_threshold_count\",\"noise_level\":-0.8,\"below_noise\":15}",
        "{" + T + "noise_threshold_count\",\"below_noise\":15,\"above_noise\":4}", "{" + T + "noise_threshold_count\",\"noise_level\":-0.8,\"above_noise\":4}",
        "{" + T + "noise_threshold_count\",\"noise_level\":\"a\",\"below_noise\":15,\"above_noise\":4}",
        // fixed_placement
        "{" + T + "fixed_placement\",\"positions\":[[1,2,3],[-4,5,6]]}", "{" + T + "fixed_placement\",\"positions\":[]}", "{" + T + "fixed_placement\",\"positions\":[[1,2]]}",
        "{" + T + "fixed_placement\",\"positions\":[[1,2,3,4]]}", "{" + T + "fixed_placement\",\"positions\":[1,2,3]}", "{" + T + "fixed_placement\"}",
        "{" + T + "fixed_placement\",\"positions\":[[1.5,2,3]]}", "{" + T + "fixed_placement\",\"positions\":[[\"a\",2,3]]}",
        "{" + T + "fixed_placement\",\"positions\":[{\"x\":1,\"y\":2,\"z\":3}]}", "{" + T + "fixed_placement\",\"positions\":{}}",
        // randomly_selected
        "{" + T + "randomly_selected\",\"placements\":[{" + T + "in_square\"},{" + T + "count\",\"count\":2}]}", "{" + T + "randomly_selected\",\"placements\":[]}",
        "{" + T + "randomly_selected\"}", "{" + T + "randomly_selected\",\"placements\":[{" + T + "nope\"}]}",
        "{" + T + "randomly_selected\",\"placements\":[{" + T + "count\",\"count\":4097}]}",
        "{" + T + "randomly_selected\",\"placements\":[{" + T + "randomly_selected\",\"placements\":[{" + T + "in_square\"}]}]}",
        "{" + T + "randomly_selected\",\"placements\":{" + T + "in_square\"}}", "{" + T + "randomly_selected\",\"placements\":[1]}",
        // block_predicate_filter
        "{" + T + "block_predicate_filter\",\"predicate\":{" + T + "true\"}}", "{" + T + "block_predicate_filter\",\"predicate\":{" + T + "matching_blocks\",\"blocks\":\"minecraft:air\"}}",
        "{" + T + "block_predicate_filter\",\"predicate\":{" + T + "matching_blocks\",\"blocks\":\"minecraft:nope\"}}",
        "{" + T + "block_predicate_filter\",\"predicate\":{" + T + "not\",\"predicate\":{" + T + "solid\"}}}", "{" + T + "block_predicate_filter\"}",
        "{" + T + "block_predicate_filter\",\"predicate\":{" + T + "nope\"}}", "{" + T + "block_predicate_filter\",\"predicate\":1}",
        "{" + T + "block_predicate_filter\",\"predicate\":{" + T + "all_of\",\"predicates\":[{" + T + "true\"}]}}",
        // environment_scan
        "{" + T + "environment_scan\",\"direction_of_search\":\"up\",\"max_steps\":12,\"target_condition\":{" + T + "solid\"}}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"down\",\"max_steps\":1,\"target_condition\":{" + T + "solid\"},\"allowed_search_condition\":{" + T + "matching_blocks\",\"blocks\":[\"minecraft:air\"]}}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"down\",\"max_steps\":32,\"target_condition\":{" + T + "solid\"}}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"down\",\"max_steps\":33,\"target_condition\":{" + T + "solid\"}}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"down\",\"max_steps\":0,\"target_condition\":{" + T + "solid\"}}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"north\",\"max_steps\":4,\"target_condition\":{" + T + "solid\"}}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"UP\",\"max_steps\":4,\"target_condition\":{" + T + "solid\"}}",
        "{" + T + "environment_scan\",\"max_steps\":4,\"target_condition\":{" + T + "solid\"}}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"up\",\"target_condition\":{" + T + "solid\"}}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"up\",\"max_steps\":4}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"up\",\"max_steps\":4,\"target_condition\":{" + T + "solid\"},\"allowed_search_condition\":{" + T + "nope\"}}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"up\",\"max_steps\":4,\"target_condition\":{" + T + "solid\"},\"allowed_search_condition\":null}",
        "{" + T + "environment_scan\",\"direction_of_search\":\"up\",\"max_steps\":4.5,\"target_condition\":{" + T + "solid\"}}",
    };

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        RegistryOps<JsonElement> ops = RegistryOps.create(JsonOps.INSTANCE, (RegistryAccess) FeatureOracleBase.ACCESS);
        try (PrintWriter out = new PrintWriter(new FileWriter(args[0]))) {
            for (String json : CASES) {
                boolean ok;
                try {
                    ok = PlacementModifier.CODEC.parse(ops, JsonParser.parseString(json)).result().isPresent();
                } catch (RuntimeException e) {
                    ok = false;
                }
                out.println((ok ? "OK" : "ERR") + "\t" + json);
            }
        }
        System.out.println(CASES.length + " cases");
    }
}
