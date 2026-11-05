import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.JsonOps;
import java.io.*;
import java.lang.reflect.Field;
import java.nio.file.*;
import java.util.*;
import java.util.stream.Stream;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerChunkCache;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.WorldGenRegion;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.chunk.status.WorldGenContext;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.blockpredicates.BlockPredicate;
import net.minecraft.world.level.levelgen.feature.stateproviders.BlockStateProvider;

/**
 * Oracle of crates/mc-game/tests/golden/levelgen_predicates_golden.rs: every {@code BlockPredicate} / {@code BlockStateProvider} JSON of
 * the vanilla data pack (feature, placed_feature, block_state_provider) plus the hand-written accept/reject/evaluation cases of
 * tests/fixtures/levelgen/predicate_provider_edge_cases.txt, decoded with the vanilla codecs and evaluated at positions of the
 * synthetic world of FeatureOracleBase (a real WorldGenRegion).
 * Usage (repo root): scripts/vanilla-java.sh crates/mc-game/tests/oracle/PredicateProviderOracle.java crates/mc-game/tests/fixtures/levelgen/predicate_provider_golden.txt
 *
 * Output lines: {@code CASE i kind json}, {@code DEC i ok|err message}, {@code SCENARIO s seed cx cz}, {@code POS s x,y,z;...},
 * {@code RES i s result}; a predicate result is a 0/1 string per position, a provider result is
 * {@code <state ids>|<optional state ids, -1 = null>|<next long after each pass>}; {@code X <exception>} replaces either.
 */
public class PredicateProviderOracle {
    static final Set<String> PRED_TYPES = Set.of("matching_blocks", "matching_block_tag", "matching_fluids", "matching_biomes", "has_sturdy_face", "solid",
        "replaceable", "would_survive", "inside_world_bounds", "any_of", "all_of", "not", "true", "unobstructed", "height_range", "volume_match");
    static final Set<String> PROV_TYPES = Set.of("simple", "weighted", "noise", "noise_threshold", "dual_noise", "rotated", "random_block", "randomized_int",
        "rule_based", "copy_properties");
    static final Set<String> PRED_KEYS = Set.of("predicate", "if_true", "allowed_placement", "allowed_search_condition", "target_condition", "can_replace",
        "continue_through", "can_place_on", "can_place_feature", "can_replace_with_air_or_fluid", "can_replace_with_barrier", "allowed_tree_position",
        "project_through", "replaceable_blocks", "target");
    static final Set<String> PROV_KEYS = Set.of("state_provider", "block_state_provider", "to_place", "provider", "block_provider", "foliage_provider",
        "trunk_provider", "dirt_provider", "below_trunk_provider", "state", "source", "fallback", "then", "block");
    static final int POSITIONS = 160;

    record Case(String kind, JsonElement json) {}

    static void collect(JsonElement e, String key, boolean root, Map<String, Case> out) {
        if (e.isJsonObject()) {
            JsonObject o = e.getAsJsonObject();
            String type = o.has("type") && o.get("type").isJsonPrimitive() ? o.get("type").getAsString() : null;
            if (type != null && type.startsWith("minecraft:")) {
                String t = type.substring(10);
                if (root && PROV_TYPES.contains(t)) out.putIfAbsent("prov " + o, new Case("prov", o));
                else if (key != null && PRED_KEYS.contains(key) && PRED_TYPES.contains(t)) out.putIfAbsent("pred " + o, new Case("pred", o));
                else if (key != null && PROV_KEYS.contains(key) && PROV_TYPES.contains(t)) out.putIfAbsent("prov " + o, new Case("prov", o));
            }
            for (var en : o.entrySet()) collect(en.getValue(), en.getKey(), false, out);
        } else if (e.isJsonArray()) {
            for (JsonElement c : e.getAsJsonArray()) collect(c, key, false, out);
        }
    }

    static List<Case> cases() throws Exception {
        Map<String, Case> out = new LinkedHashMap<>();
        Path root = Paths.get("ref/src/data/minecraft/worldgen");
        for (String dir : new String[] {"block_state_provider", "feature", "placed_feature"}) {
            List<Path> files;
            try (Stream<Path> s = Files.walk(root.resolve(dir))) { files = s.filter(p -> p.toString().endsWith(".json")).sorted().toList(); }
            for (Path f : files) collect(JsonParser.parseString(Files.readString(f)), null, dir.equals("block_state_provider"), out);
        }
        for (String line : Files.readAllLines(Paths.get("crates/mc-game/tests/fixtures/levelgen/predicate_provider_edge_cases.txt"))) {
            if (line.isBlank() || line.startsWith("#")) continue;
            int sp = line.indexOf(' ');
            String kind = line.substring(0, sp);
            JsonElement json = JsonParser.parseString(line.substring(sp + 1));
            out.putIfAbsent(kind + " " + json, new Case(kind, json));
        }
        return new ArrayList<>(out.values());
    }

    static sun.misc.Unsafe unsafe;

    static void set(Object o, Class<?> c, String name, Object v) throws Exception {
        Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        f.set(o, v);
    }

    /** {@code region.getLevel().getChunkSource().getGenerator()} (WorldGenerationContext.of) over the shared overworld generator. */
    static void giveLevel(WorldGenRegion region) throws Exception {
        ServerLevel level = (ServerLevel) unsafe.allocateInstance(ServerLevel.class);
        ServerChunkCache cache = (ServerChunkCache) unsafe.allocateInstance(ServerChunkCache.class);
        ChunkMap map = (ChunkMap) unsafe.allocateInstance(ChunkMap.class);
        set(map, ChunkMap.class, "worldGenContext", new WorldGenContext(null, FeatureOracleBase.generator(), null, null, null, null));
        set(cache, ServerChunkCache.class, "chunkMap", map);
        set(level, ServerLevel.class, "chunkSource", cache);
        set(region, WorldGenRegion.class, "level", level);
    }

    static String oneLine(String s) {
        return s.replace('\n', ' ').replace('\r', ' ');
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        uf.setAccessible(true);
        unsafe = (sun.misc.Unsafe) uf.get(null);
        List<Case> cases = cases();
        RegistryOps<JsonElement> ops = RegistryOps.create(JsonOps.INSTANCE, FeatureOracleBase.ACCESS);
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        Object[] decoded = new Object[cases.size()];
        for (int i = 0; i < cases.size(); i++) {
            Case c = cases.get(i);
            out.println("CASE " + i + " " + c.kind + " " + c.json);
            try {
                DataResult<?> r = c.kind.equals("pred") ? BlockPredicate.CODEC.parse(ops, c.json) : BlockStateProvider.CODEC.parse(ops, c.json);
                Optional<?> v = r.result();
                if (v.isPresent()) {
                    decoded[i] = c.kind.equals("pred") ? v.get() : ((Holder<?>) v.get()).value();
                    out.println("DEC " + i + " ok");
                } else {
                    out.println("DEC " + i + " err " + oneLine(r.error().map(e -> e.message()).orElse("?")));
                }
            } catch (RuntimeException e) {
                // codecs whose constructors throw (e.g. WeightedList weight overflow) abort the decode with an exception
                out.println("DEC " + i + " err exception " + e.getClass().getSimpleName() + ": " + oneLine(String.valueOf(e.getMessage())));
            }
        }
        long[][] scenarios = {{7L, 0, 0}, {-123456789012345L, 37, -91}, {424242L, -5, 3}};
        for (int s = 0; s < scenarios.length; s++) {
            long seed = scenarios[s][0];
            int cx = (int) scenarios[s][1], cz = (int) scenarios[s][2];
            FeatureOracleBase.Region region = FeatureOracleBase.newRegion(seed, cx, cz, ChunkStatus.FEATURES, 2);
            giveLevel(region);
            Random r = new Random(seed ^ 0x9051L);
            List<BlockPos> positions = new ArrayList<>();
            StringBuilder ps = new StringBuilder();
            for (int p = 0; p < POSITIONS; p++) {
                int x = cx * 16 + r.nextInt(16), z = cz * 16 + r.nextInt(16);
                int y = r.nextInt(10) < 7 ? 45 + r.nextInt(40) : -64 + r.nextInt(384);
                positions.add(new BlockPos(x, y, z));
                ps.append(p == 0 ? "" : ";").append(x).append(',').append(y).append(',').append(z);
            }
            out.println("SCENARIO " + s + " " + seed + " " + cx + " " + cz);
            out.println("POS " + s + " " + ps);
            for (int i = 0; i < cases.size(); i++) {
                if (decoded[i] == null) continue;
                String res;
                try {
                    if (cases.get(i).kind.equals("pred")) {
                        BlockPredicate pred = (BlockPredicate) decoded[i];
                        StringBuilder sb = new StringBuilder();
                        for (BlockPos pos : positions) sb.append(pred.test(region, pos) ? '1' : '0');
                        res = sb.toString();
                    } else {
                        BlockStateProvider prov = (BlockStateProvider) decoded[i];
                        StringBuilder sb = new StringBuilder();
                        RandomSource rs = new LegacyRandomSource(seed * 31 + i);
                        for (int p = 0; p < positions.size(); p++) sb.append(p == 0 ? "" : ",").append(Block.BLOCK_STATE_REGISTRY.getId(prov.getState(region, rs, positions.get(p))));
                        sb.append('|');
                        long l1 = rs.nextLong();
                        rs = new LegacyRandomSource(seed * 31 + i);
                        for (int p = 0; p < positions.size(); p++) {
                            BlockState st = prov.getOptionalState(region, rs, positions.get(p));
                            sb.append(p == 0 ? "" : ",").append(st == null ? -1 : Block.BLOCK_STATE_REGISTRY.getId(st));
                        }
                        sb.append('|').append(l1).append(',').append(rs.nextLong());
                        res = sb.toString();
                    }
                } catch (Throwable t) {
                    res = "X " + t.getClass().getSimpleName();
                }
                out.println("RES " + i + " " + s + " " + res);
            }
        }
        out.close();
    }
}
