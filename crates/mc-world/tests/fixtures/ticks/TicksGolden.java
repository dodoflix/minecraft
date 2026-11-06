// Golden generator for ticks::{LevelChunkTicks, ProtoChunkTicks, LevelTicks}: random op scripts executed by the real
// vanilla classes; each fixture line is `<scenario>\t<op>\t<result>` and the Rust test replays the ops.
// Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/ticks/ticks_golden.txt crates/mc-world/tests/fixtures/ticks/TicksGolden.java crates/mc-world/tests/fixtures/ticks/ticks_golden.txt
// Output: crates/mc-world/tests/fixtures/ticks/ticks_golden.txt
import java.io.PrintWriter;
import java.util.*;
import java.util.stream.Collectors;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Vec3i;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.ticks.*;

public class TicksGolden {
    static Block[] BLOCKS;
    static PrintWriter out;
    static String scen;

    static String name(Block b) { return BuiltInRegistries.BLOCK.getKey(b).getPath(); }
    static Block block(String n) { return BLOCKS[Arrays.asList("stone", "dirt", "sand", "redstone_wire", "oak_sign").indexOf(n)]; }
    static String st(ScheduledTick<Block> t) {
        return t == null ? "null" : name(t.type()) + "," + t.pos().getX() + "," + t.pos().getY() + "," + t.pos().getZ() + "," + t.triggerTick() + "," + t.priority().getValue() + "," + t.subTickOrder();
    }
    static String sv(SavedTick<Block> t) {
        return name(t.type()) + "," + t.pos().getX() + "," + t.pos().getY() + "," + t.pos().getZ() + "," + t.delay() + "," + t.priority().getValue();
    }
    static String stl(Collection<ScheduledTick<Block>> l) { return l.stream().map(TicksGolden::st).collect(Collectors.joining(";")); }
    static String svl(Collection<SavedTick<Block>> l) { return l.stream().map(TicksGolden::sv).collect(Collectors.joining(";")); }
    static void emit(String op, String res) { out.println(scen + "\t" + op + "\t" + res); }

    static SavedTick<Block> randSaved(Random r, int range, int cx, int cz) {
        BlockPos p = new BlockPos(cx * 16 + r.nextInt(range), r.nextInt(3), cz * 16 + r.nextInt(range));
        return new SavedTick<>(BLOCKS[r.nextInt(BLOCKS.length)], p, r.nextInt(10) - 2, TickPriority.byValue(r.nextInt(7) - 3));
    }

    static void chunkScenario(int seed, boolean pending, boolean proto) {
        Random r = new Random(seed);
        scen = (proto ? "PROTO " : "CHUNK ") + seed;
        int range = 1 + r.nextInt(3); // tiny area -> many duplicates
        if (proto) {
            List<SavedTick<Block>> init = new ArrayList<>();
            if (pending) for (int i = 0; i < 12; i++) init.add(randSaved(r, range, 0, 0));
            ProtoChunkTicks<Block> p = ProtoChunkTicks.load(init);
            emit("LOAD " + svl(init), "" + p.count() + "|" + svl(p.scheduledTicks()));
            for (int i = 0; i < 60; i++) {
                switch (r.nextInt(4)) {
                    case 0, 1 -> {
                        Block b = BLOCKS[r.nextInt(BLOCKS.length)];
                        BlockPos pos = new BlockPos(r.nextInt(range), r.nextInt(3), r.nextInt(range));
                        TickPriority pr = TickPriority.byValue(r.nextInt(7) - 3);
                        long trig = r.nextInt(10);
                        p.schedule(new ScheduledTick<>(b, pos, trig, pr, r.nextInt(5)));
                        emit("S " + name(b) + " " + pos.getX() + " " + pos.getY() + " " + pos.getZ() + " " + trig + " " + pr.getValue() + " 0", "ok");
                    }
                    case 2 -> {
                        Block b = BLOCKS[r.nextInt(BLOCKS.length)];
                        BlockPos pos = new BlockPos(r.nextInt(range), r.nextInt(3), r.nextInt(range));
                        emit("H " + name(b) + " " + pos.getX() + " " + pos.getY() + " " + pos.getZ(), "" + p.hasScheduledTick(pos, b));
                    }
                    default -> emit("A 5", p.count() + "|" + svl(p.pack(5)));
                }
            }
            return;
        }
        LevelChunkTicks<Block> c;
        if (pending) {
            List<SavedTick<Block>> init = new ArrayList<>();
            for (int i = 0; i < 1 + r.nextInt(8); i++) init.add(randSaved(r, range, 0, 0));
            c = new LevelChunkTicks<>(init);
            emit("NEWPENDING " + svl(init), "" + c.count());
        } else {
            c = new LevelChunkTicks<>();
            emit("NEW", "0");
        }
        for (int i = 0; i < 120; i++) {
            int k = r.nextInt(100);
            if (k < 45) {
                Block b = BLOCKS[r.nextInt(BLOCKS.length)];
                BlockPos pos = new BlockPos(r.nextInt(range), r.nextInt(3), r.nextInt(range));
                TickPriority pr = TickPriority.byValue(r.nextInt(7) - 3);
                long trig = r.nextInt(8), sub = r.nextInt(6) - 2;
                c.schedule(new ScheduledTick<>(b, pos, trig, pr, sub));
                emit("S " + name(b) + " " + pos.getX() + " " + pos.getY() + " " + pos.getZ() + " " + trig + " " + pr.getValue() + " " + sub, "" + c.count());
            } else if (k < 55) {
                Block b = BLOCKS[r.nextInt(BLOCKS.length)];
                BlockPos pos = new BlockPos(r.nextInt(range), r.nextInt(3), r.nextInt(range));
                emit("H " + name(b) + " " + pos.getX() + " " + pos.getY() + " " + pos.getZ(), "" + c.hasScheduledTick(pos, b));
            } else if (k < 62) emit("K", st(c.peek()));
            else if (k < 70) emit("P", st(c.poll()));
            else if (k < 80) {
                int m = 2 + r.nextInt(3), rem = r.nextInt(m);
                c.removeIf(t -> Math.floorMod(t.pos().getX() + t.pos().getY() + t.pos().getZ(), m) == rem);
                emit("R " + m + " " + rem, c.count() + "|" + stl(c.getAll().collect(Collectors.toList())));
            } else if (k < 84) {
                long cur = r.nextInt(5);
                c.unpack(cur);
                emit("U " + cur, c.count() + "|" + stl(c.getAll().collect(Collectors.toList())));
            } else if (k < 92) {
                long cur = r.nextInt(8);
                emit("A " + cur, svl(c.pack(cur)));
            } else emit("G", c.count() + "|" + stl(c.getAll().collect(Collectors.toList())));
        }
    }

    static int[] MOD = {0};
    static boolean allowed(long p) { return MOD[0] == 0 || Math.floorMod(ChunkPos.getX(p) + ChunkPos.getZ(p), MOD[0]) != 0; }

    static void levelScenario(int seed, int chunkRange, int nOps, int range) {
        Random r = new Random(seed);
        scen = "LEVEL " + seed;
        LevelTicks<Block> lt = new LevelTicks<>(TicksGolden::allowed);
        long sub = 0;
        long cur = 0;
        emit("NEW", "0");
        Set<Long> loaded = new HashSet<>();
        // initial containers
        for (int cx = -chunkRange; cx <= chunkRange; cx++) for (int cz = -chunkRange; cz <= chunkRange; cz++) {
            if (r.nextInt(4) == 0) continue;
            List<SavedTick<Block>> init = new ArrayList<>();
            for (int i = r.nextInt(4); i > 0; i--) init.add(randSaved(r, 16, cx, cz));
            LevelChunkTicks<Block> c = new LevelChunkTicks<>(init);
            c.unpack(cur);
            lt.addContainer(new ChunkPos(cx, cz), c);
            loaded.add(ChunkPos.pack(cx, cz));
            emit("ADD " + cx + " " + cz + " " + cur + " " + svl(init), "" + lt.count());
        }
        for (int i = 0; i < nOps; i++) {
            int k = r.nextInt(100);
            int span = chunkRange * 16 + 16;
            if (k < 50) {
                Block b = BLOCKS[r.nextInt(BLOCKS.length)];
                BlockPos pos = new BlockPos(r.nextInt(2 * span) - span, r.nextInt(3), r.nextInt(2 * span) - span);
                TickPriority pr = TickPriority.byValue(r.nextInt(7) - 3);
                long trig = cur + r.nextInt(6) - 1;
                long s = r.nextInt(4) == 0 ? r.nextInt(3) : sub++;
                lt.schedule(new ScheduledTick<>(b, pos, trig, pr, s));
                emit("S " + name(b) + " " + pos.getX() + " " + pos.getY() + " " + pos.getZ() + " " + trig + " " + pr.getValue() + " " + s, "" + lt.count());
            } else if (k < 75) {
                cur += r.nextInt(3);
                int max = new int[] {1, 2, 3, 5, 100000}[r.nextInt(5)];
                MOD[0] = r.nextInt(4) == 0 ? 2 + r.nextInt(2) : 0;
                List<String> ran = new ArrayList<>();
                lt.tick(cur, max, (p, t) -> ran.add(name(t) + "," + p.getX() + "," + p.getY() + "," + p.getZ()));
                emit("T " + cur + " " + max + " " + MOD[0], String.join(";", ran) + "|" + lt.count());
            } else if (k < 80) {
                Block b = BLOCKS[r.nextInt(BLOCKS.length)];
                BlockPos pos = new BlockPos(r.nextInt(2 * span) - span, r.nextInt(3), r.nextInt(2 * span) - span);
                emit("H " + name(b) + " " + pos.getX() + " " + pos.getY() + " " + pos.getZ(), "" + lt.hasScheduledTick(pos, b));
            } else if (k < 86) {
                int cx = r.nextInt(2 * chunkRange + 1) - chunkRange, cz = r.nextInt(2 * chunkRange + 1) - chunkRange;
                lt.removeContainer(new ChunkPos(cx, cz));
                loaded.remove(ChunkPos.pack(cx, cz));
                emit("REM " + cx + " " + cz, "" + lt.count());
            } else if (k < 90) {
                int cx = r.nextInt(2 * chunkRange + 1) - chunkRange, cz = r.nextInt(2 * chunkRange + 1) - chunkRange;
                if (loaded.contains(ChunkPos.pack(cx, cz))) continue;
                List<SavedTick<Block>> init = new ArrayList<>();
                for (int j = r.nextInt(4); j > 0; j--) init.add(randSaved(r, 16, cx, cz));
                LevelChunkTicks<Block> c = new LevelChunkTicks<>(init);
                c.unpack(cur);
                lt.addContainer(new ChunkPos(cx, cz), c);
                loaded.add(ChunkPos.pack(cx, cz));
                emit("ADD " + cx + " " + cz + " " + cur + " " + svl(init), "" + lt.count());
            } else if (k < 95) {
                int x0 = r.nextInt(2 * span) - span, z0 = r.nextInt(2 * span) - span;
                BoundingBox bb = new BoundingBox(x0, 0, z0, x0 + r.nextInt(40), 2, z0 + r.nextInt(40));
                lt.clearArea(bb);
                emit("CL " + bb.minX() + " " + bb.minZ() + " " + bb.maxX() + " " + bb.maxZ(), "" + lt.count());
            } else {
                int x0 = r.nextInt(2 * span) - span, z0 = r.nextInt(2 * span) - span;
                BoundingBox bb = new BoundingBox(x0, 0, z0, x0 + r.nextInt(20), 2, z0 + r.nextInt(20));
                Vec3i off = new Vec3i(r.nextInt(33) - 16, 0, r.nextInt(33) - 16);
                lt.copyArea(bb, off);
                emit("CP " + bb.minX() + " " + bb.minZ() + " " + bb.maxX() + " " + bb.maxZ() + " " + off.getX() + " " + off.getZ(), "" + lt.count());
            }
        }
        // drain everything at the end
        for (int i = 0; i < 12; i++) {
            cur += 1;
            MOD[0] = 0;
            List<String> ran = new ArrayList<>();
            lt.tick(cur, 100000, (p, t) -> ran.add(name(t) + "," + p.getX() + "," + p.getY() + "," + p.getZ()));
            emit("T " + cur + " 100000 0", String.join(";", ran) + "|" + lt.count());
        }
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        BLOCKS = new Block[] {Blocks.STONE, Blocks.DIRT, Blocks.SAND, Blocks.REDSTONE_WIRE, Blocks.OAK_SIGN};
        try (PrintWriter o = new PrintWriter(args[0])) {
            out = o;
            for (int s = 1; s <= 40; s++) chunkScenario(s, s % 2 == 0, false);
            for (int s = 1; s <= 10; s++) chunkScenario(s, s % 2 == 0, true);
            for (int s = 1; s <= 25; s++) levelScenario(s, 1 + s % 3, 300, 16);
            for (int s = 100; s <= 103; s++) levelScenario(s, 6, 2500, 16);
        }
    }
}
