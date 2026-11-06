import com.mojang.datafixers.util.Pair;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.nio.file.*;
import java.util.*;
import java.util.function.Predicate;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.SectionPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.Bootstrap;
import net.minecraft.SharedConstants;
import net.minecraft.util.datafix.DataFixTypes;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.ai.village.poi.*;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.chunk.storage.ChunkIOErrorReporter;
import net.minecraft.world.level.chunk.storage.RegionStorageInfo;
import net.minecraft.world.level.chunk.storage.SimpleRegionStorage;

/**
 * Golden generator for PoiManager (crates/mc-game/tests/golden/poi_golden.rs):
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/poi_oracle.txt crates/mc-game/tests/oracle/PoiOracle.java
 * Output: crates/mc-game/tests/fixtures/poi_oracle.txt (stdout)
 * Per scenario: "scenario N", then "<op> => <result>" lines (the Rust test replays the ops), "chunk cx cz <hex>" (uncompressed
 * NBT of every poi chunk after flushAll+close) and, after "post", queries against a fresh manager that loaded the written files.
 * Query results that span several POI types are sorted by type name inside one section order only when stated: the type order inside
 * a section is a HashMap of identity hashes in vanilla, so multi-type results are canonicalised by (type name) groups kept in order of
 * first appearance sorted by name; single-type results are exact vanilla order.
 */
public class PoiOracle {
    static final String[] TYPES = {"minecraft:home", "minecraft:meeting", "minecraft:nether_portal", "minecraft:butcher", "minecraft:lodestone"};
    static final RegistryStorageInfoHolder INFO = new RegistryStorageInfoHolder();
    /** Created after bootstrap (Level.OVERWORLD touches the registries). */
    static class RegistryStorageInfoHolder { RegionStorageInfo info; }

    static Holder<PoiType> holder(String name) {
        return BuiltInRegistries.POINT_OF_INTEREST_TYPE.getOrThrow(ResourceKey.create(Registries.POINT_OF_INTEREST_TYPE, Identifier.parse(name)));
    }

    static PoiManager open(Path dir) {
        ChunkIOErrorReporter reporter = new ChunkIOErrorReporter() {
            public void reportChunkLoadFailure(Throwable t, RegionStorageInfo i, ChunkPos p) { throw new RuntimeException(t); }
            public void reportChunkSaveFailure(Throwable t, RegionStorageInfo i, ChunkPos p) { throw new RuntimeException(t); }
        };
        return new PoiManager(INFO.info, dir, DataFixers.getDataFixer(), true, RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY), reporter, LevelHeightAccessor.create(-64, 384));
    }

    static Predicate<Holder<PoiType>> pred(String t) {
        return t.equals("*") ? h -> true : h -> h.is(ResourceKey.create(Registries.POINT_OF_INTEREST_TYPE, Identifier.parse(t)));
    }

    static PoiManager.Occupancy occ(String s) { return PoiManager.Occupancy.valueOf(s); }

    static String rec(PoiRecord r) {
        BlockPos p = r.getPos();
        return p.getX() + "," + p.getY() + "," + p.getZ() + "," + r.getPoiType().getRegisteredName() + "," + r.getFreeTickets();
    }

    static String pos(Optional<BlockPos> p) { return p.map(b -> b.getX() + "," + b.getY() + "," + b.getZ()).orElse("none"); }

    /** Stable sort by type name: the cross-type order of a section is an identity-hash HashMap in vanilla. */
    static String list(java.util.stream.Stream<PoiRecord> s) {
        List<PoiRecord> l = new ArrayList<>(s.toList());
        l.sort(Comparator.comparing(r -> r.getPoiType().getRegisteredName()));
        return "[" + String.join(";", l.stream().map(PoiOracle::rec).toList()) + "]";
    }

    static String run(PoiManager m, String line) {
        String[] a = line.split(" ");
        switch (a[0]) {
            case "add": {
                PoiRecord r = m.add(new BlockPos(i(a, 1), i(a, 2), i(a, 3)), holder(a[4]));
                return r == null ? "null" : rec(r);
            }
            case "remove": m.remove(new BlockPos(i(a, 1), i(a, 2), i(a, 3))); return "void";
            case "release": try { return String.valueOf(m.release(new BlockPos(i(a, 1), i(a, 2), i(a, 3)))); } catch (IllegalStateException e) { return "ISE"; }
            case "take": return pos(m.take(pred(a[5]), (h, p) -> true, new BlockPos(i(a, 1), i(a, 2), i(a, 3)), i(a, 4)));
            case "range": return list(m.getInRange(pred(a[6]), new BlockPos(i(a, 1), i(a, 2), i(a, 3)), i(a, 4), occ(a[5])));
            case "square": return list(m.getInSquare(pred(a[6]), new BlockPos(i(a, 1), i(a, 2), i(a, 3)), i(a, 4), occ(a[5])));
            case "count": return String.valueOf(m.getCountInRange(pred(a[6]), new BlockPos(i(a, 1), i(a, 2), i(a, 3)), i(a, 4), occ(a[5])));
            case "closest": return pos(m.findClosest(pred(a[6]), new BlockPos(i(a, 1), i(a, 2), i(a, 3)), i(a, 4), occ(a[5])));
            case "closestwt": {
                Optional<Pair<Holder<PoiType>, BlockPos>> r = m.findClosestWithType(pred(a[6]), new BlockPos(i(a, 1), i(a, 2), i(a, 3)), i(a, 4), occ(a[5]));
                return r.map(p -> p.getFirst().getRegisteredName() + "@" + p.getSecond().getX() + "," + p.getSecond().getY() + "," + p.getSecond().getZ()).orElse("none");
            }
            case "find": return pos(m.find(pred(a[6]), p -> true, new BlockPos(i(a, 1), i(a, 2), i(a, 3)), i(a, 4), occ(a[5])));
            case "findall": return "[" + String.join(";", m.findAll(pred(a[6]), p -> true, new BlockPos(i(a, 1), i(a, 2), i(a, 3)), i(a, 4), occ(a[5])).map(p -> p.getX() + "," + p.getY() + "," + p.getZ()).toList()) + "]";
            case "closestfirst": return "[" + String.join(";", m.findAllClosestFirstWithType(pred(a[6]), p -> true, new BlockPos(i(a, 1), i(a, 2), i(a, 3)), i(a, 4), occ(a[5])).map(p -> p.getFirst().getRegisteredName() + "@" + p.getSecond().getX() + "," + p.getSecond().getY() + "," + p.getSecond().getZ()).toList()) + "]";
            case "random": return pos(m.getRandom(pred(a[6]), p -> true, occ(a[5]), new BlockPos(i(a, 1), i(a, 2), i(a, 3)), i(a, 4), new net.minecraft.world.level.levelgen.LegacyRandomSource(Long.parseLong(a[7]))));
            case "exists": return String.valueOf(m.exists(new BlockPos(i(a, 1), i(a, 2), i(a, 3)), pred(a[4])));
            case "gettype": return m.getType(new BlockPos(i(a, 1), i(a, 2), i(a, 3))).map(Holder::getRegisteredName).orElse("none");
            case "village": return String.valueOf(m.sectionsToVillage(SectionPos.of(i(a, 1), i(a, 2), i(a, 3))));
            case "inchunk": return list(m.getInChunk(pred(a[4]), new ChunkPos(i(a, 1), i(a, 2)), occ(a[3])));
            case "flush": m.flush(new ChunkPos(i(a, 1), i(a, 2))); settle(); return "void";
            case "tick": m.tick(() -> true); settle(); return "void";
            default: throw new IllegalArgumentException(line);
        }
    }

    /** The IOWorker stores asynchronously and can lose a write submitted while the same chunk is being stored: let it finish. */
    static void settle() {
        try { Thread.sleep(50); } catch (InterruptedException e) { throw new RuntimeException(e); }
    }

    static int i(String[] a, int k) { return Integer.parseInt(a[k]); }

    static String hex(CompoundTag tag) throws Exception {
        ByteArrayOutputStream bo = new ByteArrayOutputStream();
        NbtIo.write(tag, new DataOutputStream(bo));
        StringBuilder sb = new StringBuilder();
        for (byte b : bo.toByteArray()) sb.append(String.format(java.util.Locale.ROOT, "%02x", b));
        return sb.toString();
    }

    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        // the data pack tag `point_of_interest_type/village` (not loaded outside a server)
        List<Holder<PoiType>> village = new ArrayList<>();
        for (String n : new String[]{"armorer", "butcher", "cartographer", "cleric", "farmer", "fisherman", "fletcher", "leatherworker", "librarian", "mason", "shepherd", "toolsmith", "weaponsmith", "home", "meeting"}) village.add(holder("minecraft:" + n));
        net.minecraft.core.MappedRegistry<PoiType> reg = (net.minecraft.core.MappedRegistry<PoiType>) BuiltInRegistries.POINT_OF_INTEREST_TYPE;
        java.lang.reflect.Field frozen = net.minecraft.core.MappedRegistry.class.getDeclaredField("frozen");
        frozen.setAccessible(true);
        frozen.setBoolean(reg, false);
        reg.bindTags(Map.of(net.minecraft.tags.PoiTypeTags.VILLAGE, village));
        java.lang.reflect.Method refresh = net.minecraft.core.MappedRegistry.class.getDeclaredMethod("refreshTagsInHolders");
        refresh.setAccessible(true);
        refresh.invoke(reg);
        frozen.setBoolean(reg, true);
        INFO.info = new RegionStorageInfo("test", Level.OVERWORLD, "poi");
        long[] seeds = {1, 2, 3};
        for (long seed : seeds) {
            Random r = new Random(seed);
            Path dir = Files.createTempDirectory("poi-oracle");
            PoiManager m = open(dir);
            System.out.println("scenario " + seed);
            Set<Long> chunks = new TreeSet<>();
            int span = 40 + (int) seed * 20; // blocks around the origin
            int ops = 600;
            for (int n = 0; n < ops; n++) {
                int x = r.nextInt(span * 2) - span, y = r.nextInt(40) - 10, z = r.nextInt(span * 2) - span;
                String t = TYPES[r.nextInt(TYPES.length)];
                String concrete = TYPES[r.nextInt(TYPES.length)];
                String q = r.nextBoolean() ? "*" : concrete;
                String o = new String[]{"HAS_SPACE", "IS_OCCUPIED", "ANY"}[r.nextInt(3)];
                int radius = 1 + r.nextInt(40);
                String line;
                switch (r.nextInt(18)) {
                    case 0, 1, 2, 3, 4, 5 -> line = "add " + x + " " + y + " " + z + " " + t;
                    case 6, 7 -> line = "remove " + x + " " + y + " " + z;
                    case 8 -> line = "release " + x + " " + y + " " + z;
                    case 9 -> line = "take " + x + " " + y + " " + z + " " + radius + " " + concrete;
                    case 10 -> line = "range " + x + " " + y + " " + z + " " + radius + " " + o + " " + q;
                    case 11 -> line = "square " + x + " " + y + " " + z + " " + radius + " " + o + " " + q;
                    case 12 -> line = "closest " + x + " " + y + " " + z + " " + radius + " " + o + " " + concrete;
                    case 13 -> line = "find " + x + " " + y + " " + z + " " + radius + " " + o + " " + concrete;
                    case 14 -> line = "count " + x + " " + y + " " + z + " " + radius + " " + o + " " + q;
                    case 15 -> line = "exists " + x + " " + y + " " + z + " " + q;
                    case 16 -> line = "village " + (x >> 4) + " " + (y >> 4) + " " + (z >> 4);
                    default -> line = "tick";
                }
                // a position that was never registered makes `remove` log an error and `release` throw: both are exercised
                if (line.startsWith("add") || line.startsWith("remove") || line.startsWith("release") || line.startsWith("take")) chunks.add(ChunkPos.pack(x >> 4, z >> 4));
                System.out.println(line + " => " + run(m, line));
            }
            for (long c : chunks) {
                ChunkPos p = ChunkPos.unpack(c);
                for (int dx = -1; dx <= 1; dx++) for (int dz = -1; dz <= 1; dz++) for (int sy = -1; sy <= 1; sy++) {
                    String line = "village " + (p.x() + dx) + " " + sy + " " + (p.z() + dz);
                    System.out.println(line + " => " + run(m, line));
                }
            }
            settle();
            m.flushAll();
            m.close();
            SimpleRegionStorage storage = new SimpleRegionStorage(INFO.info, dir, DataFixers.getDataFixer(), true, DataFixTypes.POI_CHUNK);
            for (long c : chunks) {
                ChunkPos p = ChunkPos.unpack(c);
                Optional<CompoundTag> tag = storage.read(p).join();
                if (tag.isPresent()) System.out.println("chunk " + p.x() + " " + p.z() + " " + hex(tag.get()));
            }
            storage.close();
            System.out.println("post");
            PoiManager m2 = open(dir);
            for (long c : chunks) {
                ChunkPos p = ChunkPos.unpack(c);
                for (String q : new String[]{"*", "minecraft:home", "minecraft:meeting"}) {
                    String line = "range " + (p.x() * 16 + 8) + " 5 " + (p.z() * 16 + 8) + " 24 ANY " + q;
                    System.out.println(line + " => " + run(m2, line));
                }
                for (int dx = -1; dx <= 1; dx++) for (int sy = -1; sy <= 0; sy++) {
                    String line2 = "village " + (p.x() + dx) + " " + sy + " " + p.z();
                    System.out.println(line2 + " => " + run(m2, line2));
                }
            }
            m2.close();
        }
    }
}
