import java.io.*;
import java.util.*;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.TagParser;
import net.minecraft.resources.Identifier;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.ticks.TickPriority;

/**
 * Oracle of crates/mc-game-core/tests/golden/gen_level_golden.rs: random operation scripts (block placement/removal incl. block-entity
 * blocks, far/out-of-zone writes, heights, biomes, ticks, post-processing, block entity NBT, entities, region random) executed
 * on the real vanilla WorldGenRegion over the synthetic terrain of FeatureOracleBase, followed by FeatureOracleBase.dump.
 * Usage (repo root): scripts/vanilla-java.sh crates/mc-game/tests/oracle/GenLevelOracle.java crates/mc-game/tests/fixtures/gen_level.txt
 *
 * Output: {@code SCENARIO id seed cx cz step chunkRadius primeWg}, then per operation {@code > op} / {@code = result}, then the dump
 * lines as {@code D <line>}, then {@code END}. The Rust test replays every {@code >} line and compares.
 */
public class GenLevelOracle {
    static final String[] STATES = {
        "minecraft:stone", "minecraft:air", "minecraft:water", "minecraft:lava", "minecraft:sand", "minecraft:bedrock",
        "minecraft:grass_block[snowy=false]", "minecraft:oak_log[axis=y]",
        "minecraft:oak_stairs[facing=north,half=top,shape=straight,waterlogged=false]",
        "minecraft:oak_leaves[distance=3,persistent=false,waterlogged=false]",
        "minecraft:chest[facing=east,type=single,waterlogged=false]", "minecraft:furnace[facing=south,lit=false]",
        "minecraft:spawner", "minecraft:barrel[facing=up,open=false]",
        "minecraft:brown_mushroom", "minecraft:red_mushroom", "minecraft:soul_sand", "minecraft:magma_block",
    };
    static final int[] FLAGS = {3, 2, 18, 16, 19, 0, 1, 34};
    static final String[] TICK_BLOCKS = {"minecraft:sand", "minecraft:water", "minecraft:lava", "minecraft:stone", "minecraft:gravel"};
    static final String[] TICK_FLUIDS = {"minecraft:water", "minecraft:lava", "minecraft:flowing_water"};
    static final String[] HEIGHTMAPS = {"WORLD_SURFACE_WG", "OCEAN_FLOOR_WG", "WORLD_SURFACE", "OCEAN_FLOOR", "MOTION_BLOCKING", "MOTION_BLOCKING_NO_LEAVES"};
    static final String[] BE_SNBT = {
        "{id:\"minecraft:chest\",Items:[{count:1,id:\"minecraft:stick\",Slot:0b}],x:%d,y:%d,z:%d}",
        "{id:\"minecraft:furnace\",BurnTime:12s,x:%d,y:%d,z:%d}",
        "{id:\"minecraft:mob_spawner\",x:%d,y:%d,z:%d,Delay:20s}",
    };
    static final String[] ENTITY_SNBT = {
        "{id:\"minecraft:zombie\",Pos:[%s,%s,%s],Rotation:[0.0f,0.0f]}",
        "{id:\"minecraft:armor_stand\",Pos:[%s,%s,%s]}",
    };

    static int hi(Random r, int lo, int hi) {
        return lo + r.nextInt(hi - lo + 1);
    }

    static String pos(Random r, int x0, int x1, int z0, int z1, boolean anyY) {
        int y = anyY && r.nextInt(12) == 0 ? hi(r, FeatureOracleBase.MIN_Y - 6, FeatureOracleBase.MIN_Y + FeatureOracleBase.HEIGHT + 6) : hi(r, 40, 100);
        return hi(r, x0, x1) + " " + y + " " + hi(r, z0, z1);
    }

    static String exec(FeatureOracleBase.Region region, String op) throws Exception {
        String[] t = op.split(" ", 6);
        switch (t[0]) {
            case "set": {
                String[] u = op.split(" ");
                BlockState state = BlockStateParser.parseForBlock(BuiltInRegistries.BLOCK, u[4], false).blockState();
                return "" + region.setBlock(new BlockPos(i(u[1]), i(u[2]), i(u[3])), state, i(u[5]), 512);
            }
            case "remove": return "" + region.removeBlock(bp(t), false);
            case "destroy": return "" + region.destroyBlock(bp(t), true);
            case "get": return BlockStateParser.serialize(region.getBlockState(bp(t)));
            case "fluid": {
                FluidState f = region.getFluidState(bp(t));
                return f.getAmount() + " " + f.isSource();
            }
            case "height": return "" + region.getHeight(Heightmap.Types.valueOf(t[1]), i(t[2]), i(t[3]));
            case "biome": return region.getBiome(new BlockPos(i(t[1]), i(t[2]), i(t[3]))).unwrapKey().orElseThrow().identifier().getPath();
            case "noisebiome": return region.getNoiseBiome(i(t[1]), i(t[2]), i(t[3])).unwrapKey().orElseThrow().identifier().getPath();
            case "uncached": return region.getUncachedNoiseBiome(i(t[1]), i(t[2]), i(t[3])).unwrapKey().orElseThrow().identifier().getPath();
            case "canwrite": return "" + region.ensureCanWrite(bp(t));
            case "within": return "" + region.isWithinWriteZone(bp(t));
            case "haschunk": return "" + region.hasChunk(i(t[1]), i(t[2]));
            case "postproc": {
                BlockPos p = bp(t);
                region.getChunk(p).markPosForPostProcessing(p);
                return "ok";
            }
            case "setbe": {
                // "setbe <snbt>" (position inside the tag)
                CompoundTag tag = TagParser.parseCompoundFully(op.substring(6));
                region.getChunk(new BlockPos(tag.getIntOr("x", 0), tag.getIntOr("y", 0), tag.getIntOr("z", 0))).setBlockEntityNbt(tag);
                return "ok";
            }
            case "getbe": {
                BlockPos p = bp(t);
                CompoundTag tag = region.getChunk(p).getBlockEntityNbt(p);
                return tag == null ? "-" : tag.toString();
            }
            case "rmbe": {
                BlockPos p = bp(t);
                region.getChunk(p).removeBlockEntity(p);
                return "ok";
            }
            case "entity": {
                CompoundTag tag = TagParser.parseCompoundFully(op.substring(7));
                var pos = tag.getListOrEmpty("Pos");
                int xc = (int) Math.floor(pos.getDoubleOr(0, 0)) >> 4, zc = (int) Math.floor(pos.getDoubleOr(2, 0)) >> 4;
                ((net.minecraft.world.level.chunk.ProtoChunk) region.getChunk(xc, zc)).addEntity(tag);
                return "ok";
            }
            case "tickb": {
                String[] u = op.split(" ");
                Block b = BuiltInRegistries.BLOCK.getValue(Identifier.parse(u[4]));
                region.scheduleTick(bp(t), b, i(u[5]), TickPriority.byValue(i(u[6])));
                return "ok";
            }
            case "tickf": {
                String[] u = op.split(" ");
                Fluid f = BuiltInRegistries.FLUID.getValue(Identifier.parse(u[4]));
                region.scheduleTick(bp(t), f, i(u[5]), TickPriority.byValue(i(u[6])));
                return "ok";
            }
            case "hastickb": return "" + region.getBlockTicks().hasScheduledTick(bp(t), BuiltInRegistries.BLOCK.getValue(Identifier.parse(op.split(" ")[4])));
            case "hastickf": return "" + region.getFluidTicks().hasScheduledTick(bp(t), BuiltInRegistries.FLUID.getValue(Identifier.parse(op.split(" ")[4])));
            case "rand": return "" + region.getRandom().nextInt(i(t[1]));
            case "subtick": return "" + region.nextSubTickCount();
            default: throw new IllegalArgumentException(op);
        }
    }

    static int i(String s) { return Integer.parseInt(s); }
    static BlockPos bp(String[] t) { return new BlockPos(i(t[1]), i(t[2]), i(t[3])); }

    static List<String> script(Random r, int cx, int cz, int radius, int ops) {
        int x0 = (cx - radius) * 16, x1 = (cx + radius) * 16 + 15, z0 = (cz - radius) * 16, z1 = (cz + radius) * 16 + 15;
        // biomes are only requested with status BIOMES: chunks within distance 1 of the center (deeper ones crash like vanilla)
        int br = Math.min(radius, 1);
        int nx0 = (cx - br) * 16, nx1 = (cx + br) * 16 + 15, nz0 = (cz - br) * 16, nz1 = (cz + br) * 16 + 15;
        int bx0 = nx0 + 8 <= nx1 - 8 ? nx0 + 8 : cx * 16 + 8, bx1 = nx0 + 8 <= nx1 - 8 ? nx1 - 8 : cx * 16 + 8;
        int bz0 = nz0 + 8 <= nz1 - 8 ? nz0 + 8 : cz * 16 + 8, bz1 = nz0 + 8 <= nz1 - 8 ? nz1 - 8 : cz * 16 + 8;
        List<String> out = new ArrayList<>();
        for (int n = 0; n < ops; n++) {
            int k = r.nextInt(100);
            String p = pos(r, x0, x1, z0, z1, true);
            String[] pp = p.split(" ");
            if (k < 40) out.add("set " + p + " " + STATES[r.nextInt(STATES.length)] + " " + FLAGS[r.nextInt(FLAGS.length)]);
            else if (k < 45) out.add("remove " + p);
            else if (k < 49) out.add("destroy " + p);
            else if (k < 54) out.add("get " + p);
            else if (k < 57) out.add("fluid " + p);
            else if (k < 62) out.add("height " + HEIGHTMAPS[r.nextInt(HEIGHTMAPS.length)] + " " + hi(r, x0, x1) + " " + hi(r, z0, z1));
            else if (k < 67 && radius == 0) out.add("uncached " + hi(r, x0 >> 2, x1 >> 2) + " " + hi(r, -16, 70) + " " + hi(r, z0 >> 2, z1 >> 2)); // STRUCTURE_STARTS (radius 0) never sees BIOMES
            else if (k < 65) out.add("biome " + hi(r, bx0, bx1) + " " + hi(r, -60, 300) + " " + hi(r, bz0, bz1));
            else if (k < 67) out.add("noisebiome " + hi(r, nx0 >> 2, nx1 >> 2) + " " + hi(r, -16, 70) + " " + hi(r, nz0 >> 2, nz1 >> 2));
            else if (k < 68) out.add("uncached " + hi(r, x0 >> 2, x1 >> 2) + " " + hi(r, -16, 70) + " " + hi(r, z0 >> 2, z1 >> 2));
            else if (k < 70) out.add("canwrite " + p);
            else if (k < 71) out.add("within " + p);
            else if (k < 72) out.add("haschunk " + hi(r, cx - 10, cx + 10) + " " + hi(r, cz - 10, cz + 10));
            else if (k < 76) out.add("postproc " + p);
            else if (k < 80) out.add("setbe " + String.format(BE_SNBT[r.nextInt(BE_SNBT.length)], i(pp[0]), i(pp[1]), i(pp[2])));
            else if (k < 83) out.add("getbe " + p);
            else if (k < 85) out.add("rmbe " + p);
            else if (k < 87) out.add("entity " + String.format(ENTITY_SNBT[r.nextInt(ENTITY_SNBT.length)], (hi(r, x0, x1) + 0.5) + "d", (hi(r, 40, 100) + 0.0) + "d", (hi(r, z0, z1) + 0.25) + "d"));
            else if (k < 91) out.add("tickb " + p + " " + TICK_BLOCKS[r.nextInt(TICK_BLOCKS.length)] + " " + hi(r, 0, 40) + " " + hi(r, -3, 3));
            else if (k < 94) out.add("tickf " + p + " " + TICK_FLUIDS[r.nextInt(TICK_FLUIDS.length)] + " " + hi(r, 0, 40) + " " + hi(r, -3, 3));
            else if (k < 96) out.add("hastickb " + p + " " + TICK_BLOCKS[r.nextInt(TICK_BLOCKS.length)]);
            else if (k < 97) out.add("hastickf " + p + " " + TICK_FLUIDS[r.nextInt(TICK_FLUIDS.length)]);
            else if (k < 99) out.add("rand " + hi(r, 2, 1000));
            else out.add("subtick");
        }
        return out;
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        // step status, chunk radius of the synthetic region
        Object[][] configs = {{ChunkStatus.FEATURES, 2}, {ChunkStatus.FEATURES, 1}, {ChunkStatus.TERRAIN, 1}, {ChunkStatus.STRUCTURE_STARTS, 0}};
        Random rnd = new Random(0xFE47L);
        int id = 0;
        for (int round = 0; round < 3; round++) {
            for (Object[] cfg : configs) {
                long seed = rnd.nextLong();
                int cx = rnd.nextInt(2000) - 1000, cz = rnd.nextInt(2000) - 1000;
                if (round == 0) { cx = rnd.nextInt(5) - 2; cz = rnd.nextInt(5) - 2; }
                ChunkStatus step = (ChunkStatus) cfg[0];
                int radius = (Integer) cfg[1];
                boolean wg = round < 2;
                out.println("SCENARIO " + id++ + " " + seed + " " + cx + " " + cz + " " + step.getName() + " " + radius + " " + (wg ? 1 : 0));
                FeatureOracleBase.Region region = FeatureOracleBase.newRegion(seed, cx, cz, step, radius, wg);
                FeatureOracleBase.Snapshot before = FeatureOracleBase.snapshot(region);
                for (String op : script(rnd, cx, cz, radius, 400)) {
                    out.println("> " + op); out.flush();
                    out.println("= " + exec(region, op));
                }
                for (String line : FeatureOracleBase.dump(region, before)) out.println("D " + line);
                out.println("END");
            }
        }
        out.close();
    }
}
