// Golden generator for ticks::SavedTick list codec (SerializableChunkData codec) and UpgradeData NBT.
// Run: scripts/vanilla-java.sh crates/mc-world/tests/fixtures/ticks/SavedTickGolden.java crates/mc-world/tests/fixtures/ticks/saved_tick_golden.txt
import com.mojang.serialization.Codec;
import java.io.PrintWriter;
import java.util.List;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.nbt.TagParser;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.chunk.UpgradeData;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.ticks.SavedTick;

public class SavedTickGolden {
    static Codec<List<SavedTick<Block>>> BLOCK;
    static Codec<List<SavedTick<Fluid>>> FLUID;

    static String el(String body) { return "{" + body + "}"; }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        BLOCK = SavedTick.codec(BuiltInRegistries.BLOCK.byNameCodec()).listOf();
        FLUID = SavedTick.codec(BuiltInRegistries.FLUID.byNameCodec()).listOf();
        try (PrintWriter out = new PrintWriter(args[0])) {
            String[] blockLists = {
                "[]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0}]",
                "[{i:\"minecraft:stone\",x:-17,y:-64,z:300,t:-4,p:-3},{i:\"minecraft:dirt\",x:5,y:6,z:7,t:0,p:3},{i:\"minecraft:air\",x:5,y:6,z:7,t:0,p:1}]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:5}]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:-9}]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:2147483647},{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:-2147483648}]",
                "[{i:\"minecraft:nope\",x:1,y:2,z:3,t:4,p:0}]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0},{i:\"minecraft:nope\",x:1,y:2,z:3,t:4,p:0},{i:\"minecraft:dirt\",x:9,y:9,z:9,t:9,p:1}]",
                "[{i:\"Bad Name!\",x:1,y:2,z:3,t:4,p:0}]",
                "[{i:\"stone\",x:1,y:2,z:3,t:4,p:0}]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4}]",
                "[{x:1,y:2,z:3,t:4,p:0}]",
                "[{i:\"minecraft:stone\",y:2,z:3,t:4,p:0}]",
                "[{i:\"minecraft:stone\",x:1,z:3,t:4,p:0}]",
                "[{i:\"minecraft:stone\",x:1,y:2,t:4,p:0}]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,p:0}]",
                "[{i:5,x:1,y:2,z:3,t:4,p:0}]",
                "[{i:\"minecraft:stone\",x:1b,y:2s,z:3L,t:4.9d,p:0.5f}]",
                "[{i:\"minecraft:stone\",x:-1.5f,y:-2.5d,z:1e20d,t:-7.5f,p:-0.5d}]",
                "[{i:\"minecraft:stone\",x:\"1\",y:2,z:3,t:4,p:0}]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0,extra:7}]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:\"x\"}]",
                "[{i:\"minecraft:stone\",x:[1],y:2,z:3,t:4,p:0}]",
                "[1,2]",
                "[\"a\"]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0}, 5]",
                "[5, {i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0}]",
                "[[I;1,2]]",
                "[I;]",
                "[B;]",
                "[L;1L]",
                "{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0}",
                "\"abc\"",
                "7",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0},{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0}]",
                "[{i:\"minecraft:water\",x:1,y:2,z:3,t:4,p:0}]",
                "[{i:\"minecraft:oak_sign\",x:0,y:0,z:0,t:0,p:0},{i:\"minecraft:redstone_wire\",x:15,y:15,z:15,t:1,p:-1}]",
            };
            for (String l : blockLists) {
                String in = "{block_ticks:" + l + "}";
                CompoundTag tag = TagParser.parseCompoundFully(in);
                List<SavedTick<Block>> res = tag.read("block_ticks", BLOCK).orElse(List.of());
                Tag enc = BLOCK.encodeStart(NbtOps.INSTANCE, res).getOrThrow();
                out.println("BLOCK\t" + l + "\t" + enc);
            }
            String[] fluidLists = {
                "[{i:\"minecraft:water\",x:1,y:2,z:3,t:4,p:0},{i:\"minecraft:flowing_lava\",x:1,y:2,z:3,t:4,p:0},{i:\"minecraft:empty\",x:1,y:2,z:3,t:4,p:0}]",
                "[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0},{i:\"minecraft:lava\",x:1,y:2,z:3,t:4,p:2}]",
                "[]",
            };
            for (String l : fluidLists) {
                String in = "{fluid_ticks:" + l + "}";
                CompoundTag tag = TagParser.parseCompoundFully(in);
                List<SavedTick<Fluid>> res = tag.read("fluid_ticks", FLUID).orElse(List.of());
                Tag enc = FLUID.encodeStart(NbtOps.INSTANCE, res).getOrThrow();
                out.println("FLUID\t" + l + "\t" + enc);
            }

            // UpgradeData: (minY, height) then tag -> write() + isEmpty
            int[][] heights = {{-64, 384}, {0, 256}, {0, 16}};
            String[] ups = {
                "{}",
                "{Sides:0b}",
                "{Sides:255}",
                "{Sides:511}",
                "{Sides:-1}",
                "{Sides:5s}",
                "{Sides:3.7d}",
                "{Sides:\"3\"}",
                "{Sides:-127b,Indices:{\"0\":[I;1,2,3],\"3\":[I;],\"5\":[I;4096]}}",
                "{Indices:{\"0\":[I;1],\"1\":[B;1b],\"2\":5,\"23\":[I;7],\"24\":[I;8],\"-1\":[I;9],\"x\":[I;1]}}",
                "{Indices:[I;1,2]}",
                "{Indices:{}}",
                "{Indices:{\"0\":[I;]}}",
                "{Sides:1b,neighbor_block_ticks:[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0}],neighbor_fluid_ticks:[{i:\"minecraft:water\",x:1,y:2,z:3,t:4,p:1}]}",
                "{neighbor_block_ticks:[{i:\"minecraft:nope\",x:1,y:2,z:3,t:4,p:9},{i:\"minecraft:dirt\",x:1,y:2,z:3,t:4,p:0}]}",
                "{neighbor_block_ticks:[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4},{i:\"minecraft:dirt\",x:1,y:2,z:3,t:4,p:0}]}",
                "{neighbor_block_ticks:[{i:5,x:1,y:2,z:3,t:4,p:0},{x:1,y:2,z:3,t:4,p:0},{i:\"minecraft:dirt\",x:1,y:2,z:3,t:4,p:0}]}",
                "{neighbor_fluid_ticks:[{i:\"minecraft:stone\",x:1,y:2,z:3,t:4,p:0}]}",
                "{neighbor_block_ticks:[]}",
                "{neighbor_block_ticks:7,neighbor_fluid_ticks:\"x\"}",
                "{neighbor_block_ticks:[{i:\"Bad Name\",x:1,y:2,z:3,t:4,p:0}]}",
                "{Sides:2b,Indices:{\"0\":[I;1,2,3]},neighbor_block_ticks:[{i:\"minecraft:air\",x:1,y:2,z:3,t:4,p:0}],neighbor_fluid_ticks:[{i:\"minecraft:empty\",x:1,y:2,z:3,t:4,p:0}]}",
            };
            for (int[] h : heights) {
                for (String u : ups) {
                    CompoundTag tag = TagParser.parseCompoundFully(u);
                    UpgradeData data = new UpgradeData(tag, LevelHeightAccessor.create(h[0], h[1]));
                    CompoundTag w = data.write();
                    UpgradeData cp = data.copy();
                    out.println("UPGRADE\t" + h[0] + "\t" + h[1] + "\t" + u + "\t" + w + "\t" + data.isEmpty() + "\t" + cp.write());
                }
            }
        }
    }
}
