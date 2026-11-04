import java.io.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;

/**
 * Drives the real vanilla Heightmap on a real ProtoChunk with random setBlockState sequences.
 * Usage: scripts/vanilla-java.sh HeightmapGolden.java out.txt
 * Output lines:
 *   CFG <minY> <height> <status>
 *   OP <x> <y> <z> <stateId>
 *   SNAP <type> <hsp> <long,long,...>     incremental heightmaps after the preceding ops
 *   PRIME <type> <long,...>               Heightmap.primeHeightmaps over a fresh chunk holding identical blocks
 *   END
 */
public class HeightmapGolden {
    static PalettedContainerFactory factory() {
        var blocks = Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY);
        IdMapper<Holder<Biome>> ids = new IdMapper<>();
        Holder<Biome> dummy = Holder.direct(null);
        ids.add(dummy);
        var biomes = Strategy.createForBiomes(ids);
        return new PalettedContainerFactory(blocks, Blocks.AIR.defaultBlockState(), null, biomes, dummy, null);
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        // tags come from the data-generator output (run from the repo root)
        var info = new net.minecraft.server.packs.PackLocationInfo("gen", net.minecraft.network.chat.Component.literal("gen"), net.minecraft.server.packs.repository.PackSource.BUILT_IN, Optional.empty());
        var rm = new net.minecraft.server.packs.resources.MultiPackResourceManager(net.minecraft.server.packs.PackType.SERVER_DATA,
            List.<net.minecraft.server.packs.PackResources>of(new net.minecraft.server.packs.PathPackResources(info, java.nio.file.Path.of("ref/generated"))));
        net.minecraft.tags.TagLoader.loadTagsForExistingRegistries(rm, net.minecraft.core.RegistryAccess.fromRegistryOfRegistries(net.minecraft.core.registries.BuiltInRegistries.REGISTRY)).forEach(net.minecraft.core.Registry.PendingTags::apply);
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        PalettedContainerFactory f = factory();
        Block[] pool = {
            Blocks.AIR, Blocks.AIR, Blocks.AIR, Blocks.CAVE_AIR, Blocks.VOID_AIR, Blocks.STONE, Blocks.STONE, Blocks.DIRT, Blocks.WATER, Blocks.LAVA,
            Blocks.OAK_LEAVES, Blocks.GLASS, Blocks.SHORT_GRASS, Blocks.KELP, Blocks.SEAGRASS, Blocks.SNOW, Blocks.ICE, Blocks.COBWEB,
            Blocks.BUBBLE_COLUMN, Blocks.OAK_LOG, Blocks.TORCH, Blocks.OAK_FENCE, Blocks.OAK_SLAB, Blocks.TALL_SEAGRASS, Blocks.BARRIER,
            Blocks.SPRUCE_LEAVES, Blocks.BEDROCK, Blocks.OAK_SAPLING, Blocks.POWDER_SNOW, Blocks.LIGHT, Blocks.OAK_SIGN
        };
        List<BlockState> states = new ArrayList<>();
        for (Block b : pool) {
            List<BlockState> all = b.getStateDefinition().getPossibleStates();
            states.add(all.get(0));
            states.add(all.get(all.size() - 1));
            states.add(all.get(all.size() / 2));
        }
        // waterlogged states
        for (BlockState s : Blocks.OAK_SLAB.getStateDefinition().getPossibleStates()) states.add(s);

        int[][] cfgs = {{-64, 384, 5}, {0, 256, 5}, {-64, 384, 3}, {0, 256, 4}, {-48, 160, 5}, {16, 320, 3}};
        Random rnd = new Random(0x5EED5L);
        for (int[] cfg : cfgs) {
            int minY = cfg[0], height = cfg[1];
            ChunkStatus status = ChunkStatus.getStatusList().get(cfg[2]);
            out.println("CFG " + minY + " " + height + " " + BuiltInRegistries_name(status));
            LevelHeightAccessor acc = LevelHeightAccessor.create(minY, height);
            ProtoChunk chunk = new ProtoChunk(new ChunkPos(3, -5), UpgradeData.EMPTY, acc, f, null);
            chunk.setPersistedStatus(status);
            Map<Long, BlockState> placed = new LinkedHashMap<>();
            int nOps = 1000;
            for (int op = 0; op < nOps; op++) {
                int x = rnd.nextInt(16), z = rnd.nextInt(16);
                int y;
                int mode = rnd.nextInt(10);
                if (mode < 4) {
                    y = minY + rnd.nextInt(height);
                } else if (mode < 5) {
                    y = minY + rnd.nextInt(Math.min(height, 24)); // low band, many collisions
                } else {
                    // near current top of a random heightmap
                    Heightmap.Types t = status.heightmapsAfter().iterator().next();
                    int top = chunk.getHeight(t, x, z);
                    y = Math.max(minY, Math.min(minY + height - 1, top + rnd.nextInt(7) - 3));
                }
                BlockState s = states.get(rnd.nextInt(states.size()));
                chunk.setBlockState(new BlockPos(x, y, z), s, 0);
                placed.put(((long) x << 40) | ((long) z << 32) | (y - minY), s);
                out.println("OP " + x + " " + y + " " + z + " " + Block.BLOCK_STATE_REGISTRY.getId(s));
                if (op % 250 == 249 || op == nOps - 1) {
                    for (Map.Entry<Heightmap.Types, Heightmap> e : chunk.getHeightmaps()) {
                        out.println("SNAP " + e.getKey().getSerializationKey() + " " + chunk.getHighestSectionPosition() + " " + longs(e.getValue().getRawData()));
                    }
                }
            }
            // fresh chunk with identical blocks, set at section level so no heightmap is touched
            ProtoChunk fresh = new ProtoChunk(new ChunkPos(3, -5), UpgradeData.EMPTY, acc, f, null);
            for (Map.Entry<Long, BlockState> e : placed.entrySet()) {
                long k = e.getKey();
                int x = (int) (k >> 40) & 15, z = (int) (k >> 32) & 15, y = (int) (k & 0xFFFFFFFFL) + minY;
                fresh.getSection(fresh.getSectionIndex(y)).setBlockState(x, y & 15, z, e.getValue());
            }
            if (fresh.getHighestSectionPosition() != chunk.getHighestSectionPosition()) throw new IllegalStateException("hsp mismatch");
            Heightmap.primeHeightmaps(fresh, EnumSet.allOf(Heightmap.Types.class));
            for (Map.Entry<Heightmap.Types, Heightmap> e : fresh.getHeightmaps()) {
                out.println("PRIME " + e.getKey().getSerializationKey() + " " + fresh.getHighestSectionPosition() + " " + longs(e.getValue().getRawData()));
            }
        }
        out.println("END");
        out.close();
    }

    static String BuiltInRegistries_name(ChunkStatus s) { return s.getName(); }

    static String longs(long[] a) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < a.length; i++) { if (i > 0) sb.append(','); sb.append(a[i]); }
        return sb.toString();
    }
}
