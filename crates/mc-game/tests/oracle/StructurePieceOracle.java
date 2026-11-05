import java.io.*;
import java.util.*;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.StructureManager;
import net.minecraft.world.level.WorldGenLevel;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.ChunkGenerator;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.ScatteredFeaturePiece;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext;
import net.minecraft.world.level.levelgen.structure.pieces.StructurePieceType;
import net.minecraft.world.level.storage.loot.LootTable;

/**
 * Oracle of crates/mc-game/tests/structure_piece_golden.rs: scripts of the protected helpers of vanilla's StructurePiece
 * (placeBlock/generateBox/generateMaybeBox/generateAirBox/sphere/fillColumnDown/createChest/createDispenser/BlockSelector,
 * ScatteredFeaturePiece ground height) on test subclasses over the synthetic terrain of FeatureOracleBase.
 * Usage (repo root): scripts/vanilla-java.sh crates/mc-game/tests/oracle/StructurePieceOracle.java crates/mc-game/tests/fixtures/structure_piece.txt
 * Output like GenLevelOracle: SCENARIO / "> op" / "= result" / "D dump" / END.
 */
public class StructurePieceOracle {
    static final String[] STATES = {
        "minecraft:stone", "minecraft:air", "minecraft:cave_air", "minecraft:water", "minecraft:oak_planks", "minecraft:cobblestone",
        "minecraft:oak_stairs[facing=north,half=bottom,shape=straight,waterlogged=false]",
        "minecraft:stone_brick_stairs[facing=east,half=top,shape=inner_left,waterlogged=false]",
        "minecraft:torch", "minecraft:wall_torch[facing=east]", "minecraft:ladder[facing=south,waterlogged=false]",
        "minecraft:iron_bars[east=false,north=true,south=false,waterlogged=false,west=false]",
        "minecraft:nether_brick_fence[east=false,north=false,south=false,waterlogged=false,west=true]",
        "minecraft:oak_door[facing=east,half=lower,hinge=left,open=false,powered=false]",
        "minecraft:chest[facing=east,type=single,waterlogged=false]", "minecraft:furnace[facing=south,lit=false]",
        "minecraft:oak_log[axis=x]", "minecraft:glow_lichen[down=true,east=false,north=false,south=false,up=false,waterlogged=false,west=false]",
        "minecraft:seagrass", "minecraft:lava", "minecraft:dispenser[facing=north,triggered=false]", "minecraft:oak_fence[east=false,north=true,south=false,waterlogged=false,west=false]",
    };
    static final String[] LOOT = {"minecraft:chests/simple_dungeon", "minecraft:chests/village/village_weaponsmith", "minecraft:chests/stronghold_corridor"};
    static final String[] FACINGS = {"north", "south", "east", "west", "up", "down"};
    static final String[] ORIENT = {"null", "north", "south", "east", "west"};

    static class TestPiece extends StructurePiece {
        TestPiece(BoundingBox bb, Direction o) {
            super(StructurePieceType.MINE_SHAFT_ROOM, 3, bb);
            setOrientation(o);
        }
        @Override protected void addAdditionalSaveData(StructurePieceSerializationContext c, CompoundTag t) {}
        @Override public void postProcess(WorldGenLevel l, StructureManager m, ChunkGenerator g, RandomSource r, BoundingBox b, ChunkPos p, BlockPos ref) {}
        void box(WorldGenLevel l, BoundingBox cbb, int[] c, BlockState e, BlockState f, boolean skip) { generateBox(l, cbb, c[0], c[1], c[2], c[3], c[4], c[5], e, f, skip); }
        void air(WorldGenLevel l, BoundingBox cbb, int[] c) { generateAirBox(l, cbb, c[0], c[1], c[2], c[3], c[4], c[5]); }
        void maybe(WorldGenLevel l, BoundingBox cbb, RandomSource r, float p, int[] c, BlockState e, BlockState f, boolean skip, boolean inside) { generateMaybeBox(l, cbb, r, p, c[0], c[1], c[2], c[3], c[4], c[5], e, f, skip, inside); }
        void maybeBlock(WorldGenLevel l, BoundingBox cbb, RandomSource r, float p, int x, int y, int z, BlockState s) { maybeGenerateBlock(l, cbb, r, p, x, y, z, s); }
        void sphere(WorldGenLevel l, BoundingBox cbb, int[] c, BlockState f, boolean skip) { generateUpperHalfSphere(l, cbb, c[0], c[1], c[2], c[3], c[4], c[5], f, skip); }
        void column(WorldGenLevel l, BoundingBox cbb, BlockState s, int x, int y, int z) { fillColumnDown(l, s, x, y, z, cbb); }
        boolean chest(WorldGenLevel l, BoundingBox cbb, RandomSource r, int x, int y, int z, String loot) { return createChest(l, cbb, r, x, y, z, ResourceKey.create(net.minecraft.core.registries.Registries.LOOT_TABLE, Identifier.parse(loot))); }
        boolean dispenser(WorldGenLevel l, BoundingBox cbb, RandomSource r, int x, int y, int z, Direction d, String loot) { return createDispenser(l, cbb, r, x, y, z, d, ResourceKey.create(net.minecraft.core.registries.Registries.LOOT_TABLE, Identifier.parse(loot))); }
        void place(WorldGenLevel l, BoundingBox cbb, BlockState s, int x, int y, int z) { placeBlock(l, s, x, y, z, cbb); }
        BlockState get(WorldGenLevel l, BoundingBox cbb, int x, int y, int z) { return getBlock(l, x, y, z, cbb); }
        boolean interior(WorldGenLevel l, BoundingBox cbb, int x, int y, int z) { return isInterior(l, x, y, z, cbb); }
        void sel(WorldGenLevel l, BoundingBox cbb, int[] c, boolean skip, RandomSource r, BlockState a, BlockState b) {
            generateBox(l, cbb, c[0], c[1], c[2], c[3], c[4], c[5], skip, r, new StructurePiece.BlockSelector() {
                @Override public void next(RandomSource rr, int x, int y, int z, boolean edge) {
                    this.next = edge ? (rr.nextFloat() < 0.3F ? a : b) : (rr.nextInt(4) == 0 ? a : b);
                }
            });
        }
        String describe() { return getBoundingBox() + " " + getOrientation() + " " + getRotation() + " " + getMirror(); }
    }

    static class TestScattered extends ScatteredFeaturePiece {
        TestScattered(int x, int y, int z, int w, int h, int d, Direction dir) { super(StructurePieceType.DESERT_PYRAMID_PIECE, x, y, z, w, h, d, dir); }
        @Override public void postProcess(WorldGenLevel l, StructureManager m, ChunkGenerator g, RandomSource r, BoundingBox b, ChunkPos p, BlockPos ref) {}
        boolean avg(WorldGenLevel l, BoundingBox cbb, int off) { return updateAverageGroundHeight(l, cbb, off); }
        boolean low(WorldGenLevel l, int off) { return updateHeightPositionToLowestGroundHeight(l, off); }
        String describe() { return getBoundingBox() + " " + heightPosition; }
    }

    static BlockState state(String s) throws Exception {
        return BlockStateParser.parseForBlock(BuiltInRegistries.BLOCK, s, false).blockState();
    }

    static int i(String s) { return Integer.parseInt(s); }
    static int[] ints(String[] t, int from) { int[] c = new int[6]; for (int k = 0; k < 6; k++) c[k] = i(t[from + k]); return c; }
    static Direction dir(String s) { return s.equals("null") ? null : Direction.byName(s); }

    static class Ctx {
        FeatureOracleBase.Region region;
        WorldgenRandom random;
        BoundingBox chunkBB;
        TestPiece piece;
        TestScattered scattered;
    }

    static String exec(Ctx c, String op) throws Exception {
        String[] t = op.split(" ");
        WorldGenLevel l = c.region;
        switch (t[0]) {
            case "chunkbb": c.chunkBB = new BoundingBox(i(t[1]), i(t[2]), i(t[3]), i(t[4]), i(t[5]), i(t[6])); return "ok";
            case "piece": c.piece = new TestPiece(new BoundingBox(i(t[2]), i(t[3]), i(t[4]), i(t[5]), i(t[6]), i(t[7])), dir(t[1])); return c.piece.describe();
            case "scattered": c.scattered = new TestScattered(i(t[1]), i(t[2]), i(t[3]), i(t[4]), i(t[5]), i(t[6]), dir(t[7])); return c.scattered.describe();
            case "avg": { boolean b = c.scattered.avg(l, c.chunkBB, i(t[1])); return b + " " + c.scattered.describe(); }
            case "low": { boolean b = c.scattered.low(l, i(t[1])); return b + " " + c.scattered.describe(); }
            case "place": c.piece.place(l, c.chunkBB, state(t[4]), i(t[1]), i(t[2]), i(t[3])); return "ok";
            case "get": return BlockStateParser.serialize(c.piece.get(l, c.chunkBB, i(t[1]), i(t[2]), i(t[3])));
            case "interior": return "" + c.piece.interior(l, c.chunkBB, i(t[1]), i(t[2]), i(t[3]));
            case "box": c.piece.box(l, c.chunkBB, ints(t, 1), state(t[7]), state(t[8]), t[9].equals("true")); return "ok";
            case "air": c.piece.air(l, c.chunkBB, ints(t, 1)); return "ok";
            case "maybebox": c.piece.maybe(l, c.chunkBB, c.random, Float.parseFloat(t[1]), ints(t, 2), state(t[8]), state(t[9]), t[10].equals("true"), t[11].equals("true")); return "ok";
            case "maybeblock": c.piece.maybeBlock(l, c.chunkBB, c.random, Float.parseFloat(t[1]), i(t[2]), i(t[3]), i(t[4]), state(t[5])); return "ok";
            case "sphere": c.piece.sphere(l, c.chunkBB, ints(t, 1), state(t[7]), t[8].equals("true")); return "ok";
            case "column": c.piece.column(l, c.chunkBB, state(t[4]), i(t[1]), i(t[2]), i(t[3])); return "ok";
            case "chest": return "" + c.piece.chest(l, c.chunkBB, c.random, i(t[1]), i(t[2]), i(t[3]), t[4]);
            case "dispenser": return "" + c.piece.dispenser(l, c.chunkBB, c.random, i(t[1]), i(t[2]), i(t[3]), Direction.byName(t[4]), t[5]);
            case "sel": c.piece.sel(l, c.chunkBB, ints(t, 1), t[7].equals("true"), c.random, state(t[8]), state(t[9])); return "ok";
            case "move": c.piece.move(i(t[1]), i(t[2]), i(t[3])); return c.piece.describe();
            case "rand": return "" + c.random.nextLong();
            default: throw new IllegalArgumentException(op);
        }
    }

    static String st(Random r) { return STATES[r.nextInt(STATES.length)]; }
    static int hi(Random r, int lo, int hi) { return lo + r.nextInt(hi - lo + 1); }
    static String box(Random r, int span) {
        int x0 = hi(r, -2, 14), y0 = hi(r, -2, 8), z0 = hi(r, -2, 14);
        return x0 + " " + y0 + " " + z0 + " " + (x0 + r.nextInt(span)) + " " + (y0 + r.nextInt(6)) + " " + (z0 + r.nextInt(span));
    }

    static List<String> script(Random r, int cx, int cz, int ops) {
        List<String> out = new ArrayList<>();
        for (int n = 0; n < ops; n++) {
            int k = r.nextInt(100);
            if (k < 8) {
                int x0 = cx * 16 + hi(r, -20, 20), z0 = cz * 16 + hi(r, -20, 20), y0 = hi(r, 55, 80);
                out.add("piece " + ORIENT[r.nextInt(ORIENT.length)] + " " + x0 + " " + y0 + " " + z0 + " " + (x0 + hi(r, 4, 24)) + " " + (y0 + hi(r, 3, 12)) + " " + (z0 + hi(r, 4, 24)));
            } else if (k < 11) {
                int x0 = cx * 16 + hi(r, -16, 16), z0 = cz * 16 + hi(r, -16, 16);
                out.add("chunkbb " + x0 + " " + hi(r, -20, 60) + " " + z0 + " " + (x0 + hi(r, 3, 24)) + " " + hi(r, 61, 120) + " " + (z0 + hi(r, 3, 24)));
            } else if (k < 14) {
                out.add("scattered " + (cx * 16 + hi(r, -10, 10)) + " " + hi(r, 50, 90) + " " + (cz * 16 + hi(r, -10, 10)) + " " + hi(r, 3, 21) + " " + hi(r, 3, 14) + " " + hi(r, 3, 21) + " " + ORIENT[1 + r.nextInt(4)]);
                out.add(r.nextBoolean() ? "avg " + hi(r, -3, 3) : "low " + hi(r, -3, 3));
            } else if (k < 24) out.add("place " + hi(r, -3, 20) + " " + hi(r, -3, 12) + " " + hi(r, -3, 20) + " " + st(r));
            else if (k < 28) out.add("get " + hi(r, -3, 20) + " " + hi(r, -3, 12) + " " + hi(r, -3, 20));
            else if (k < 31) out.add("interior " + hi(r, -3, 20) + " " + hi(r, -3, 12) + " " + hi(r, -3, 20));
            else if (k < 46) out.add("box " + box(r, 8) + " " + st(r) + " " + st(r) + " " + r.nextBoolean());
            else if (k < 52) out.add("air " + box(r, 6));
            else if (k < 60) out.add("maybebox " + (r.nextInt(4) == 0 ? "1.0" : "0." + hi(r, 1, 9)) + " " + box(r, 7) + " " + st(r) + " " + st(r) + " " + r.nextBoolean() + " " + r.nextBoolean());
            else if (k < 65) out.add("maybeblock 0." + hi(r, 1, 9) + " " + hi(r, -3, 20) + " " + hi(r, -3, 12) + " " + hi(r, -3, 20) + " " + st(r));
            else if (k < 70) out.add("sphere " + box(r, 9) + " " + st(r) + " " + r.nextBoolean());
            else if (k < 75) out.add("column " + hi(r, -3, 20) + " " + hi(r, -3, 14) + " " + hi(r, -3, 20) + " " + st(r));
            else if (k < 82) out.add("chest " + hi(r, -3, 20) + " " + hi(r, -3, 12) + " " + hi(r, -3, 20) + " " + LOOT[r.nextInt(LOOT.length)]);
            else if (k < 86) out.add("dispenser " + hi(r, -3, 20) + " " + hi(r, -3, 12) + " " + hi(r, -3, 20) + " " + FACINGS[r.nextInt(FACINGS.length)] + " " + LOOT[r.nextInt(LOOT.length)]);
            else if (k < 93) out.add("sel " + box(r, 8) + " " + r.nextBoolean() + " " + st(r) + " " + st(r));
            else if (k < 96) out.add("move " + hi(r, -4, 4) + " " + hi(r, -4, 4) + " " + hi(r, -4, 4));
            else out.add("rand");
        }
        return out;
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        Random rnd = new Random(0x57C7L);
        for (int id = 0; id < 12; id++) {
            long seed = rnd.nextLong();
            int cx = rnd.nextInt(200) - 100, cz = rnd.nextInt(200) - 100;
            out.println("SCENARIO " + id + " " + seed + " " + cx + " " + cz);
            Ctx c = new Ctx();
            c.region = FeatureOracleBase.newRegion(seed, cx, cz, ChunkStatus.FEATURES, 2);
            c.random = new WorldgenRandom(new LegacyRandomSource(seed ^ 0x5DEECE66DL));
            c.chunkBB = new BoundingBox(cx * 16, FeatureOracleBase.MIN_Y + 1, cz * 16, cx * 16 + 15, FeatureOracleBase.MIN_Y + FeatureOracleBase.HEIGHT - 1, cz * 16 + 15);
            List<String> script = script(rnd, cx, cz, 300);
            script.add(0, "piece north " + (cx * 16) + " 64 " + (cz * 16) + " " + (cx * 16 + 10) + " 70 " + (cz * 16 + 10));
            script.add(1, "scattered " + (cx * 16) + " 64 " + (cz * 16) + " 7 5 7 north");
            FeatureOracleBase.Snapshot before = FeatureOracleBase.snapshot(c.region);
            for (String op : script) {
                out.println("> " + op); out.flush();
                out.println("= " + exec(c, op));
            }
            for (String line : FeatureOracleBase.dump(c.region, before)) out.println("D " + line);
            out.println("END");
        }
        out.close();
    }
}
