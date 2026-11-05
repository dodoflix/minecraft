// Golden generator for tests/golden/pos.rs. Run: scripts/vanilla-java.sh crates/mc-core/tests/fixtures/pos/PosOracle.java > crates/mc-core/tests/fixtures/pos/pos_golden.txt
// Line format: `<tag> <inputs...> => <outputs...>`; lists are `;`-separated, positions `x,y,z`.
import java.util.*;
import java.util.function.*;
import net.minecraft.core.*;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.minecraft.util.Mth;

public class PosOracle {
    static StringBuilder out = new StringBuilder();
    static void line(String s) { out.append(s).append('\n'); }
    static String p(Vec3i v) { return v.getX() + "," + v.getY() + "," + v.getZ(); }
    static String list(Iterable<? extends Vec3i> it, int limit) {
        StringBuilder sb = new StringBuilder();
        int n = 0;
        for (Vec3i v : it) { if (n++ >= limit) break; if (sb.length() > 0) sb.append(';'); sb.append(p(v)); }
        return sb.length() == 0 ? "-" : sb.toString();
    }
    static String listIt(Iterator<? extends Vec3i> it) {
        StringBuilder sb = new StringBuilder();
        while (it.hasNext()) { if (sb.length() > 0) sb.append(';'); sb.append(p(it.next())); }
        return sb.length() == 0 ? "-" : sb.toString();
    }
    static String cp(ChunkPos c) { return c.x() + "," + c.z(); }
    static String bb(BoundingBox b) { return b.minX() + "," + b.minY() + "," + b.minZ() + "," + b.maxX() + "," + b.maxY() + "," + b.maxZ(); }
    static int[] EXT = {Integer.MIN_VALUE, Integer.MIN_VALUE + 1, -33554433, -33554432, -33554431, -2049, -2048, -2047, -17, -16, -15, -1, 0, 1, 15, 16, 17, 2047, 2048, 2049, 33554430, 33554431, 33554432, 33554433, Integer.MAX_VALUE - 1, Integer.MAX_VALUE};

    public static void main(String[] args) {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        Random r = new Random(20260413L);

        // constants
        line("const " + BlockPos.PACKED_HORIZONTAL_LENGTH + " " + BlockPos.PACKED_Y_LENGTH + " " + BlockPos.MAX_HORIZONTAL_COORDINATE
            + " " + net.minecraft.world.level.chunk.status.ChunkPyramid.MAX_CHUNK_COORDINATE_VALUE + " " + ChunkPos.INVALID_CHUNK_POS + " => ok");

        // ---- packed BlockPos ----
        List<int[]> coords = new ArrayList<>();
        for (int x : EXT) coords.add(new int[]{x, EXT[(x & 0xff) % EXT.length], EXT[(x * 7 & 0xff) % EXT.length]});
        for (int i = 0; i < 100; i++) coords.add(new int[]{r.nextInt(), r.nextInt(), r.nextInt()});
        for (int i = 0; i < 100; i++) coords.add(new int[]{r.nextInt(70000000) - 35000000, r.nextInt(6000) - 3000, r.nextInt(70000000) - 35000000});
        for (int[] c : coords) {
            long l = BlockPos.asLong(c[0], c[1], c[2]);
            line("bp_aslong " + c[0] + " " + c[1] + " " + c[2] + " => " + l);
        }
        List<Long> longs = new ArrayList<>();
        longs.add(0L); longs.add(-1L); longs.add(Long.MIN_VALUE); longs.add(Long.MAX_VALUE); longs.add(1L); longs.add(4095L); longs.add(2048L);
        for (int i = 0; i < 100; i++) longs.add(r.nextLong());
        for (int[] c : coords) longs.add(BlockPos.asLong(c[0], c[1], c[2]));
        for (long l : longs) {
            BlockPos bp = BlockPos.of(l);
            line("bp_unpack " + l + " => " + BlockPos.getX(l) + " " + BlockPos.getY(l) + " " + BlockPos.getZ(l) + " " + p(bp)
                + " " + BlockPos.getFlatIndex(l) + " " + SectionPos.blockToSection(l) + " " + ChunkPos.pack(bp));
            int dx = r.nextInt(5) - 2, dy = r.nextInt(5) - 2, dz = r.nextInt(5) - 2;
            line("bp_offset " + l + " " + dx + " " + dy + " " + dz + " => " + BlockPos.offset(l, dx, dy, dz));
            Direction d = Direction.from3DDataValue(r.nextInt(6));
            line("bp_offset_dir " + l + " " + d.getName() + " => " + BlockPos.offset(l, d));
        }

        // ---- Vec3i / BlockPos arithmetic ----
        List<int[]> small = new ArrayList<>();
        for (int[] c : coords) small.add(c);
        for (int i = 0; i < 60; i++) small.add(new int[]{r.nextInt(2000) - 1000, r.nextInt(600) - 300, r.nextInt(2000) - 1000});
        for (int i = 0; i + 1 < small.size(); i += 1) {
            int[] a = small.get(i), b = small.get((i * 7 + 3) % small.size());
            Vec3i va = new Vec3i(a[0], a[1], a[2]), vb = new Vec3i(b[0], b[1], b[2]);
            Vec3i cr = va.cross(vb);
            line("vec3i " + p(va) + " " + p(vb) + " => " + va.hashCode() + " " + va.compareTo(vb) + " " + va.distManhattan(vb) + " "
                + va.distChessboard(vb) + " " + p(cr) + " " + va.distSqr(vb) + " " + va.closerThan(vb, 100.5) + " " + va.differsHorizontally(vb)
                + " " + p(va.offset(vb)) + " " + p(va.subtract(vb)) + " " + p(va.multiply(3)) + " " + p(va.multiply(0)) + " " + p(va.multiply(a[1], 5, -7))
                + " " + va + " " + va.toShortString());
            BlockPos pa = new BlockPos(a[0], a[1], a[2]);
            StringBuilder sb = new StringBuilder();
            for (Direction d : Direction.values()) sb.append(p(pa.relative(d))).append(' ').append(p(pa.relative(d, 3))).append(' ');
            for (Direction.Axis ax : Direction.Axis.values()) sb.append(p(pa.relative(ax, -4))).append(' ');
            for (Rotation rot : Rotation.values()) sb.append(p(pa.rotate(rot))).append(' ');
            sb.append(p(pa.above())).append(' ').append(p(pa.below(2))).append(' ').append(p(pa.north())).append(' ').append(p(pa.south(5))).append(' ')
              .append(p(pa.west())).append(' ').append(p(pa.east(2))).append(' ').append(p(pa.atY(77))).append(' ').append(p(pa.offset(vb))).append(' ')
              .append(p(pa.subtract(vb))).append(' ').append(p(pa.multiply(-2))).append(' ').append(p(pa.cross(vb))).append(' ').append(pa.get(Direction.Axis.Y)).append(' ')
              .append(p(BlockPos.min(pa, new BlockPos(b[0], b[1], b[2])))).append(' ').append(p(BlockPos.max(pa, new BlockPos(b[0], b[1], b[2])))).append(' ')
              .append(pa).append(' ').append(pa.mutable()).append(' ').append(pa.hashCode());
            line("bp_ops " + p(pa) + " " + p(vb) + " => " + sb);
        }
        // containing / clampLocationWithin / distToCenter
        double[] ds = {0, -0.0, 0.5, -0.5, 1, -1, 1.0E-5, 0.999999, -0.000001, 2.9999999999, 1234567.89, -1234567.89, 3.0E9, -3.0E9, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY};
        for (int i = 0; i < 120; i++) {
            double x = i < ds.length ? ds[i] : r.nextDouble() * 200 - 100;
            double y = i < ds.length ? ds[(i * 5 + 1) % ds.length] : r.nextDouble() * 200 - 100;
            double z = i < ds.length ? ds[(i * 3 + 2) % ds.length] : r.nextDouble() * 200 - 100;
            BlockPos c = BlockPos.containing(x, y, z);
            BlockPos q = new BlockPos(r.nextInt(7) - 3, r.nextInt(7) - 3, r.nextInt(7) - 3);
            Vec3 cl = q.clampLocationWithin(new Vec3(x, y, z));
            line("bp_containing " + Double.doubleToRawLongBits(x) + " " + Double.doubleToRawLongBits(y) + " " + Double.doubleToRawLongBits(z) + " " + p(q)
                + " => " + p(c) + " " + Double.doubleToRawLongBits(cl.x) + " " + Double.doubleToRawLongBits(cl.y) + " " + Double.doubleToRawLongBits(cl.z)
                + " " + Double.doubleToRawLongBits(q.distToCenterSqr(x, y, z)) + " " + Double.doubleToRawLongBits(q.distToLowCornerSqr(x, y, z))
                + " " + q.closerToCenterThan(new Vec3(x, y, z), 2.5) + " " + p(SectionPos.of(new Vec3(x, y, z)).origin()) + " " + SectionPos.posToSectionCoord(x));
        }

        // ---- MutableBlockPos script ----
        for (int i = 0; i < 40; i++) {
            int[] a = small.get(i * 5 % small.size());
            BlockPos.MutableBlockPos m = new BlockPos.MutableBlockPos(a[0], a[1], a[2]);
            StringBuilder sb = new StringBuilder();
            m.move(Direction.NORTH); sb.append(m).append(' ');
            m.move(Direction.UP, 3); sb.append(m).append(' ');
            m.move(-2, 5, 7); sb.append(m).append(' ');
            m.move(new Vec3i(1, 2, 3)); sb.append(m).append(' ');
            m.setWithOffset(new Vec3i(a[2], a[0], a[1]), Direction.WEST); sb.append(m).append(' ');
            m.setWithOffset(new Vec3i(a[2], a[0], a[1]), 4, 5, 6); sb.append(m).append(' ');
            m.setWithOffset(new Vec3i(a[2], a[0], a[1]), new Vec3i(-1, -2, -3)); sb.append(m).append(' ');
            m.clamp(Direction.Axis.X, -5, 5); sb.append(m).append(' ');
            m.clamp(Direction.Axis.Y, 0, 10); sb.append(m).append(' ');
            m.clamp(Direction.Axis.Z, -3, 3); sb.append(m).append(' ');
            m.set(1.7, -2.3, 9.99); sb.append(m).append(' ');
            m.set(BlockPos.asLong(a[0], a[1], a[2])); sb.append(m).append(' ');
            m.set(AxisCycle.FORWARD, 1, 2, 3); sb.append(m).append(' ');
            m.set(AxisCycle.BACKWARD, 1, 2, 3); sb.append(m).append(' ');
            m.setX(9).setY(8).setZ(7); sb.append(m).append(' ');
            sb.append(m.immutable()).append(' ').append(m.offset(1, 1, 1)).append(' ').append(m.relative(Direction.EAST, 2)).append(' ');
            sb.append(m.rotate(Rotation.CLOCKWISE_90)).append(' ').append(m.multiply(4)).append(' ').append(m.equals(new BlockPos(9, 8, 7))).append(' ').append(m.hashCode());
            line("mutable " + p(new BlockPos(a[0], a[1], a[2])) + " => " + sb);
        }
        {
            BlockPos.MutableBlockPos m = new BlockPos.MutableBlockPos(1.9, -0.1, 3.5);
            line("mutable_dbl => " + m);
        }

        // ---- iterators ----
        Direction[] hz = {Direction.NORTH, Direction.EAST, Direction.SOUTH, Direction.WEST};
        int[][] centers = {{0, 0, 0}, {10, -3, 5}, {-7, 64, 100}};
        for (int[] c : centers) {
            BlockPos ce = new BlockPos(c[0], c[1], c[2]);
            for (int radius = 0; radius <= 3; radius++) {
                for (Direction d1 : Direction.values()) {
                    for (Direction d2 : Direction.values()) {
                        if (d1.getAxis() == d2.getAxis()) continue;
                        if (radius > 1 && (d1.ordinal() + d2.ordinal()) % 3 != 0) continue;
                        line("spiral " + p(ce) + " " + radius + " " + d1.getName() + " " + d2.getName() + " => " + list(BlockPos.spiralAround(ce, radius, d1, d2), 10000));
                    }
                }
            }
            for (int reach = 0; reach <= 3; reach++) {
                line("manhattan " + p(ce) + " " + reach + " => " + list(BlockPos.withinManhattan(ce, reach), 10000));
            }
            int[][] reaches = {{1, 2, 3}, {3, 0, 2}, {0, 0, 0}, {2, 2, 0}, {4, 1, 1}, {-1, 2, 2}};
            for (int[] re : reaches) {
                line("clipped_manhattan " + p(ce) + " " + re[0] + " " + re[1] + " " + re[2] + " => " + list(BlockPos.withinClippedManhattan(ce, re[0], re[1], re[2]), 10000));
                line("box_manhattan " + p(ce) + " " + re[0] + " " + re[1] + " " + re[2] + " => " + list(BlockPos.withinBoxByManhattanDistance(ce, re[0], re[1], re[2]), 10000));
            }
        }
        int[][] boxes = {{0, 0, 0, 2, 1, 3}, {-1, -1, -1, 1, 1, 1}, {5, 5, 5, 5, 5, 5}, {3, 2, 1, 0, 0, 0}, {0, 0, 0, -1, 0, 0}, {-2, 70, 3, 0, 72, 4}};
        for (int[] b : boxes) {
            BlockPos a = new BlockPos(b[0], b[1], b[2]), c = new BlockPos(b[3], b[4], b[5]);
            line("between_closed " + p(a) + " " + p(c) + " => " + list(BlockPos.betweenClosed(a, c), 10000));
            line("between_closed_xyz " + b[0] + " " + b[1] + " " + b[2] + " " + b[3] + " " + b[4] + " " + b[5] + " => " + list(BlockPos.betweenClosed(b[0], b[1], b[2], b[3], b[4], b[5]), 10000));
            line("between_closed_box " + bb(new BoundingBox(Math.min(b[0], b[3]), Math.min(b[1], b[4]), Math.min(b[2], b[5]), Math.max(b[0], b[3]), Math.max(b[1], b[4]), Math.max(b[2], b[5])))
                + " => " + listIt(BlockPos.betweenClosedStream(new BoundingBox(Math.min(b[0], b[3]), Math.min(b[1], b[4]), Math.min(b[2], b[5]), Math.max(b[0], b[3]), Math.max(b[1], b[4]), Math.max(b[2], b[5]))).iterator()));
        }
        double[][] aabbs = {{0.5, 0.5, 0.5, 2.5, 1.2, 0.7}, {-1.5, -0.2, 3, 0.1, 0.2, 4.9}};
        for (double[] a : aabbs) {
            AABB box = new AABB(a[0], a[1], a[2], a[3], a[4], a[5]);
            line("between_closed_aabb " + Double.doubleToRawLongBits(a[0]) + " " + Double.doubleToRawLongBits(a[1]) + " " + Double.doubleToRawLongBits(a[2]) + " "
                + Double.doubleToRawLongBits(a[3]) + " " + Double.doubleToRawLongBits(a[4]) + " " + Double.doubleToRawLongBits(a[5]) + " => " + list(BlockPos.betweenClosed(box), 10000)
                + " | " + listIt(BlockPos.betweenClosedStream(box).iterator()));
        }
        for (int[] col : new int[][]{{0, 0, 0, 2}, {5, 10, -5, 7}, {0, 3, 0, 0}, {1, 1, 1, -2}}) {
            line("neighbor_column " + col[0] + " " + col[1] + " " + col[2] + " " + col[3] + " => " + list(BlockPos.neighborColumn(col[0], col[1], col[2], col[3]), 10000));
        }
        double[][] dirs = {{1, 1, 1}, {-1, 1, 1}, {1, -1, -1}, {-1, -1, -1}, {0, 0, 0}, {0.5, 2, -3}, {-4, 0.1, 0.2}, {3, -2, 0}, {-0.0, 1, -0.0}, {1, 0, 2}};
        for (double[] dv : dirs) {
            int[][] cc = {{0, 0, 0, 1, 2, 1}, {3, 4, 5, 1, 2, 3}, {-1, -1, -1, -1, -1, -1}};
            for (int[] c : cc) {
                Vec3 v = new Vec3(dv[0], dv[1], dv[2]);
                line("corners_dir " + c[0] + " " + c[1] + " " + c[2] + " " + c[3] + " " + c[4] + " " + c[5] + " " + Double.doubleToRawLongBits(dv[0]) + " " + Double.doubleToRawLongBits(dv[1]) + " " + Double.doubleToRawLongBits(dv[2])
                    + " => " + list(BlockPos.betweenCornersInDirection(c[0], c[1], c[2], c[3], c[4], c[5], v), 10000));
            }
            AABB box = new AABB(-0.5, 0.2, 1.7, 1.3, 2.4, 2.1);
            line("corners_dir_aabb " + Double.doubleToRawLongBits(dv[0]) + " " + Double.doubleToRawLongBits(dv[1]) + " " + Double.doubleToRawLongBits(dv[2]) + " => "
                + list(BlockPos.betweenCornersInDirection(box, new Vec3(dv[0], dv[1], dv[2])), 10000));
        }
        // random_between_closed: same RNG sequence
        for (long seed : new long[]{0L, 12345L, -987654321L}) {
            LegacyRandomSource rs = new LegacyRandomSource(seed);
            line("random_between " + seed + " 7 -3 4 5 5 10 9 => " + list(BlockPos.randomBetweenClosed(rs, 7, -3, 4, 5, 5, 10, 9), 10000) + " next=" + rs.nextInt());
            LegacyRandomSource rs2 = new LegacyRandomSource(seed);
            line("random_cube " + seed + " 6 1,2,3 4 => " + list(BlockPos.randomInCube(rs2, 6, new BlockPos(1, 2, 3), 4), 10000) + " next=" + rs2.nextInt());
        }
        // breadth first traversal: neighbours = 4 horizontal + above; status by hash of pos
        for (int variant = 0; variant < 4; variant++) {
            final int v = variant;
            List<String> visited = new ArrayList<>();
            int maxDepth = 2 + variant, maxCount = 12 + variant * 20;
            int res = BlockPos.breadthFirstTraversal(new BlockPos(0, 0, 0), maxDepth, maxCount,
                (pos, consumer) -> { for (Direction d : hz) consumer.accept(pos.relative(d)); consumer.accept(pos.above()); consumer.accept(pos.relative(Direction.NORTH)); },
                pos -> {
                    visited.add(p(pos));
                    int h = Math.floorMod(pos.getX() * 31 + pos.getY() * 17 + pos.getZ() * 13, 11);
                    if (h == 3) return BlockPos.TraversalNodeStatus.SKIP;
                    if (v == 3 && h == 9) return BlockPos.TraversalNodeStatus.STOP;
                    return BlockPos.TraversalNodeStatus.ACCEPT;
                });
            line("bfs " + variant + " " + maxDepth + " " + maxCount + " => " + res + " " + String.join(";", visited));
        }

        // ---- SectionPos ----
        for (int[] c : coords) {
            SectionPos s = SectionPos.of(c[0], c[1], c[2]);
            BlockPos b = new BlockPos(c[0], c[1], c[2]);
            short rel = SectionPos.sectionRelativePos(b);
            short rnd = (short) r.nextInt();
            long l = s.asLong();
            line("section " + c[0] + " " + c[1] + " " + c[2] + " " + rnd + " => " + l + " " + SectionPos.x(l) + " " + SectionPos.y(l) + " " + SectionPos.z(l)
                + " " + p(SectionPos.of(l)) + " " + SectionPos.asLong(b) + " " + p(SectionPos.of(b)) + " " + s.minBlockX() + " " + s.minBlockY() + " " + s.minBlockZ()
                + " " + s.maxBlockX() + " " + s.maxBlockY() + " " + s.maxBlockZ() + " " + p(s.origin()) + " " + p(s.center()) + " " + cp(s.chunk())
                + " " + SectionPos.getZeroNode(l) + " " + SectionPos.getZeroNode(c[0], c[2]) + " " + SectionPos.sectionToChunk(l) + " " + rel
                + " " + SectionPos.sectionRelativeX(rel) + " " + SectionPos.sectionRelativeY(rel) + " " + SectionPos.sectionRelativeZ(rel)
                + " " + SectionPos.sectionRelativeX(rnd) + " " + SectionPos.sectionRelativeY(rnd) + " " + SectionPos.sectionRelativeZ(rnd)
                + " " + s.relativeToBlockX(rnd) + " " + s.relativeToBlockY(rnd) + " " + s.relativeToBlockZ(rnd) + " " + p(s.relativeToBlockPos(rnd))
                + " " + SectionPos.blockToSectionCoord(c[0]) + " " + SectionPos.sectionRelative(c[1]) + " " + SectionPos.sectionToBlockCoord(c[2]) + " " + SectionPos.sectionToBlockCoord(c[0], 9)
                + " " + p(s.offset(1, -2, 3)) + " " + SectionPos.offset(l, 1, -2, 3) + " " + SectionPos.offset(l, Direction.WEST) + " " + s + " " + s.hashCode());
        }
        for (double d : ds) line("section_dbl " + Double.doubleToRawLongBits(d) + " => " + SectionPos.blockToSectionCoord(d) + " " + SectionPos.posToSectionCoord(d));
        for (int[] b : new int[][]{{0, 0, 0}, {15, 15, 15}, {16, 16, 16}, {-1, -1, -1}, {-16, 0, 16}, {5, 20, -3}}) {
            List<Long> got = new ArrayList<>();
            SectionPos.aroundAndAtBlockPos(new BlockPos(b[0], b[1], b[2]), got::add);
            List<Long> got2 = new ArrayList<>();
            SectionPos.aroundAndAtBlockPos(BlockPos.asLong(b[0], b[1], b[2]), got2::add);
            line("section_around " + b[0] + " " + b[1] + " " + b[2] + " => " + got + " " + got2);
        }
        SectionPos sc = SectionPos.of(2, -1, 3);
        line("section_cube => " + listIt(SectionPos.cube(sc, 1).iterator()).toString());
        line("section_around_chunk => " + listIt(SectionPos.aroundChunk(new ChunkPos(1, -2), 1, -2, 0).iterator()));
        line("section_blocks_inside => " + listIt(SectionPos.of(-1, 0, 1).blocksInside().iterator()));

        // ---- ChunkPos ----
        for (int[] c : coords) {
            ChunkPos cpos = new ChunkPos(c[0], c[2]);
            long k = cpos.pack();
            BlockPos b = new BlockPos(c[1], c[0], c[2]);
            line("chunk " + c[0] + " " + c[2] + " " + c[1] + " => " + k + " " + cp(ChunkPos.unpack(k)) + " " + ChunkPos.getX(k) + " " + ChunkPos.getZ(k)
                + " " + cpos.hashCode() + " " + cpos.isValid() + " " + cpos.getMiddleBlockX() + " " + cpos.getMiddleBlockZ() + " " + cpos.getMinBlockX() + " " + cpos.getMinBlockZ()
                + " " + cpos.getMaxBlockX() + " " + cpos.getMaxBlockZ() + " " + cpos.getRegionX() + " " + cpos.getRegionZ() + " " + ChunkPos.getRegionX(k) + " " + ChunkPos.getRegionZ(k)
                + " " + cpos.getRegionLocalX() + " " + cpos.getRegionLocalZ() + " " + p(cpos.getBlockAt(3, c[1], 4)) + " " + cpos.getBlockX(5) + " " + cpos.getBlockZ(6)
                + " " + p(cpos.getMiddleBlockPosition(c[1])) + " " + cpos.contains(b) + " " + cpos + " " + p(cpos.getWorldPosition()) + " " + cp(ChunkPos.containing(b)) + " " + ChunkPos.pack(b)
                + " " + ChunkPos.fromSectionNode(SectionPos.asLong(c[0], c[1], c[2])) + " " + cp(ChunkPos.minFromRegion(c[0], c[2])) + " " + cp(ChunkPos.maxFromRegion(c[0], c[2]))
                + " " + ChunkPos.isValid(c[0], c[2]) + " " + ChunkPos.hash(c[2], c[0]));
            ChunkPos o = new ChunkPos(c[2], c[1]);
            line("chunk_dist " + cp(cpos) + " " + cp(o) + " => " + cpos.getChessboardDistance(o) + " " + cpos.distanceSquared(o) + " " + cpos.distanceSquared(o.pack()));
            line("chunk_contains " + p(new BlockPos(c[0], 0, c[2])) + " " + cp(new ChunkPos(c[0] >> 4, c[2] >> 4)) + " => " + new ChunkPos(c[0] >> 4, c[2] >> 4).contains(new BlockPos(c[0], 0, c[2])));
        }
        for (int[] rc : new int[][]{{0, 0, 0, 0}, {-1, -1, 1, 1}, {3, 4, 0, 6}, {2, 2, 2, 2}, {5, 0, 3, 2}}) {
            ChunkPos a = new ChunkPos(rc[0], rc[1]), b = new ChunkPos(rc[2], rc[3]);
            StringBuilder sb = new StringBuilder();
            ChunkPos.rangeClosed(a, b).forEach(c -> sb.append(cp(c)).append(';'));
            line("chunk_range " + cp(a) + " " + cp(b) + " => " + sb);
        }
        for (int rad = 0; rad <= 2; rad++) {
            StringBuilder sb = new StringBuilder();
            ChunkPos.rangeClosed(new ChunkPos(7, -7), rad).forEach(c -> sb.append(cp(c)).append(';'));
            line("chunk_range_radius " + rad + " => " + sb);
        }

        // ---- QuartPos ----
        for (int x : EXT) line("quart " + x + " => " + QuartPos.fromBlock(x) + " " + QuartPos.quartLocal(x) + " " + QuartPos.toBlock(x) + " " + QuartPos.fromSection(x) + " " + QuartPos.toSection(x));

        // ---- Cursor3D ----
        for (int[] b : boxes) {
            Cursor3D cur = new Cursor3D(b[0], b[1], b[2], b[3], b[4], b[5]);
            StringBuilder sb = new StringBuilder();
            int n = 0;
            while (cur.advance() && n++ < 200) sb.append(cur.nextX()).append(',').append(cur.nextY()).append(',').append(cur.nextZ()).append(',').append(cur.getNextType()).append(';');
            line("cursor3d " + b[0] + " " + b[1] + " " + b[2] + " " + b[3] + " " + b[4] + " " + b[5] + " => " + (sb.length() == 0 ? "-" : sb));
        }

        // ---- BoundingBox ----
        List<int[]> bbs = new ArrayList<>();
        bbs.add(new int[]{0, 0, 0, 0, 0, 0});
        bbs.add(new int[]{-5, 60, -5, 5, 70, 5});
        bbs.add(new int[]{3, 3, 3, 1, 1, 1});
        bbs.add(new int[]{Integer.MIN_VALUE, Integer.MIN_VALUE, Integer.MIN_VALUE, Integer.MAX_VALUE, Integer.MAX_VALUE, Integer.MAX_VALUE});
        bbs.add(new int[]{-17, 0, 33, 31, 15, 64});
        for (int i = 0; i < 40; i++) {
            int[] b = new int[6];
            for (int j = 0; j < 6; j++) b[j] = r.nextInt(400) - 200;
            bbs.add(b);
        }
        for (int i = 0; i < bbs.size(); i++) {
            int[] a = bbs.get(i), o = bbs.get((i * 3 + 1) % bbs.size());
            BoundingBox A = new BoundingBox(a[0], a[1], a[2], a[3], a[4], a[5]);
            BoundingBox B = new BoundingBox(o[0], o[1], o[2], o[3], o[4], o[5]);
            BlockPos pt = new BlockPos(r.nextInt(20) - 10, r.nextInt(20) - 10 + 65, r.nextInt(20) - 10);
            BoundingBox mv = new BoundingBox(a[0], a[1], a[2], a[3], a[4], a[5]);
            mv.move(3, -4, 5);
            BoundingBox mv2 = new BoundingBox(a[0], a[1], a[2], a[3], a[4], a[5]);
            mv2.move(new Vec3i(-1, 2, -3));
            BoundingBox en = new BoundingBox(a[0], a[1], a[2], a[3], a[4], a[5]);
            en.encapsulate(B);
            BoundingBox ep = new BoundingBox(a[0], a[1], a[2], a[3], a[4], a[5]);
            ep.encapsulate(pt);
            StringBuilder corners = new StringBuilder();
            A.forAllCorners(c -> corners.append(p(c)).append(';'));
            StringBuilder chunks = new StringBuilder();
            if ((long) A.maxX() - A.minX() < 200 && (long) A.maxZ() - A.minZ() < 200) A.intersectingChunks().forEach(c -> chunks.append(cp(c)).append(';'));
            line("bbox " + bb(A) + " " + bb(B) + " " + p(pt) + " => " + A.intersects(B) + " " + A.intersects(B.minX(), B.minY(), B.minZ(), B.maxX(), B.maxY(), B.maxZ())
                + " " + A.intersects(B.minX(), B.minZ(), B.maxX(), B.maxZ()) + " " + bb(BoundingBox.encapsulating(A, B)) + " " + bb(en) + " " + bb(ep) + " " + bb(mv) + " " + bb(mv2)
                + " " + bb(A.moved(7, 8, -9)) + " " + bb(A.inflatedBy(2)) + " " + bb(A.inflatedBy(1, -3, 4)) + " " + A.isInside(pt) + " " + A.isInside(pt.getX(), pt.getY(), pt.getZ())
                + " " + p(A.getLength()) + " " + A.getXSpan() + " " + A.getYSpan() + " " + A.getZSpan() + " " + p(A.getCenter()) + " " + corners + " " + A + " " + A.equals(B) + " " + A.hashCode()
                + " " + bb(BoundingBox.fromCorners(new Vec3i(A.minX(), A.minY(), A.minZ()), new Vec3i(B.minX(), B.minY(), B.minZ()))) + " " + bb(new BoundingBox(pt)) + " " + (chunks.length() == 0 ? "-" : chunks));
        }
        line("bbox_infinite => " + bb(BoundingBox.infinite()));
        for (Direction d : Direction.values()) {
            line("bbox_orient " + d.getName() + " => " + bb(BoundingBox.orientBox(10, 20, 30, 1, 2, 3, 4, 5, 6, d)) + " " + bb(BoundingBox.orientBox(-3, 0, 7, 0, 0, 0, 1, 1, 1, d))
                + " " + bb(BoundingBox.orientBox(0, 64, 0, -2, 1, -4, 7, 3, 9, d)));
        }
        List<BlockPos> plist = new ArrayList<>();
        for (int i = 0; i < 6; i++) plist.add(new BlockPos(r.nextInt(50) - 25, r.nextInt(50), r.nextInt(50) - 25));
        line("bbox_encap_pos " + String.join(";", plist.stream().map(PosOracle::p).toList()) + " => " + bb(BoundingBox.encapsulatingPositions(plist).get()) + " empty=" + BoundingBox.encapsulatingPositions(new ArrayList<BlockPos>()).isPresent());
        List<BoundingBox> blist = new ArrayList<>();
        for (int i = 0; i < 4; i++) blist.add(new BoundingBox(r.nextInt(50), r.nextInt(50), r.nextInt(50), r.nextInt(50) + 50, r.nextInt(50) + 50, r.nextInt(50) + 50));
        line("bbox_encap_boxes " + String.join(";", blist.stream().map(PosOracle::bb).toList()) + " => " + bb(BoundingBox.encapsulatingBoxes(blist).get()) + " empty=" + BoundingBox.encapsulatingBoxes(new ArrayList<BoundingBox>()).isPresent());

        System.out.print(out);
    }
}
