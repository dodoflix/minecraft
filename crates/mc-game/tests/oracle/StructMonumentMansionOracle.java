import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.function.Predicate;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.WorldGenRegion;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.Climate;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext;
import net.minecraft.world.level.levelgen.structure.placement.RandomSpreadStructurePlacement;
import net.minecraft.world.level.levelgen.structure.structures.OceanMonumentStructure;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager;
import net.minecraft.world.level.levelgen.structure.StructureSet;

/**
 * Oracle of crates/mc-game/tests/struct_monument_mansion_golden.rs: the real {@code Structure.generate} of the ocean monument
 * ({@code minecraft:monument}) or the woodland mansion ({@code minecraft:mansion}) on the real overworld generator for the
 * potential start chunks of the structure set, and {@code StructureStart.placeInChunk} of every chunk of the start over the
 * synthetic world of FeatureOracleBase (one region per chunk, as the decoration step places).
 * Usage (repo root): scripts/vanilla-java.sh crates/mc-game/tests/oracle/StructMonumentMansionOracle.java <out> monument|mansion <seed,seed,..> <regionRadius> <maxStartsPerSeed>
 *
 * Output: {@code SEED s}, {@code START cx cz <createTag snbt>} per valid start (the potential chunk list is implicit: the Rust test walks the
 * same regions), then per chunk of the start's box {@code PLACE cx cz <bb>}, then the digest lines {@code DG <category> <fnv> <count>} of the dump
 * ({@code B}/{@code K} digested, the other categories verbatim as {@code D <line>}, entity UUIDs masked), then {@code END}.
 */
public class StructMonumentMansionOracle {
    static PrintWriter out;

    public static void main(String[] a) throws Exception {
        FeatureOracleBase.setup();
        out = new PrintWriter(new BufferedWriter(new FileWriter(a[0])));
        String kind = a[1];
        String structureId = kind.equals("monument") ? "monument" : "mansion";
        String setId = kind.equals("monument") ? "ocean_monuments" : "woodland_mansions";
        int radius = Integer.parseInt(a[3]);
        int maxStarts = Integer.parseInt(a[4]);
        var access = FeatureOracleBase.ACCESS;
        Holder<Structure> holder = access.lookupOrThrow(Registries.STRUCTURE).getOrThrow(ResourceKey.create(Registries.STRUCTURE, Identifier.withDefaultNamespace(structureId)));
        StructureSet set = access.lookupOrThrow(Registries.STRUCTURE_SET).getOrThrow(ResourceKey.create(Registries.STRUCTURE_SET, Identifier.withDefaultNamespace(setId))).value();
        RandomSpreadStructurePlacement placement = (RandomSpreadStructurePlacement) set.placement();
        NoiseBasedChunkGenerator generator = FeatureOracleBase.generator();
        StructureTemplateManager templates = kind.equals("mansion") ? TemplateSupport.manager() : null;
        LevelHeightAccessor height = LevelHeightAccessor.create(FeatureOracleBase.MIN_Y, FeatureOracleBase.HEIGHT);
        Predicate<Holder<Biome>> valid = b -> holder.value().biomes().contains(b);
        for (String seedText : a[2].split(",")) {
            long seed = Long.parseLong(seedText);
            RandomState randomState = FeatureOracleBase.randomState(seed);
            out.println("SEED " + seed);
            int found = 0;
            for (int rx = -radius; rx <= radius && found < maxStarts; rx++) {
                for (int rz = -radius; rz <= radius && found < maxStarts; rz++) {
                    ChunkPos chunkPos = placement.getPotentialStructureChunk(seed, rx * placement.spacing(), rz * placement.spacing());
                    StructureStart start = holder.value().generate(holder, net.minecraft.world.level.Level.OVERWORLD, access, generator, generator.getBiomeSource(), randomState.createClimateSampler(net.minecraft.world.level.levelgen.densityfunction.SamplerContext.builder().enableCaches().build()), randomState,
                        templates, seed, chunkPos, 0, height, valid);
                    if (!start.isValid()) continue;
                    found++;
                    StructurePieceSerializationContext context = new StructurePieceSerializationContext(null, access, templates);
                    out.println("START " + chunkPos.x() + " " + chunkPos.z() + " " + start.createTag(context, chunkPos));
                    BoundingBox box = start.getBoundingBox();
                    for (int cx = box.minX() >> 4; cx <= box.maxX() >> 4; cx++) {
                        for (int cz = box.minZ() >> 4; cz <= box.maxZ() >> 4; cz++) place(start, holder.value(), generator, seed, cx, cz);
                    }
                }
            }
        }
        out.println("END");
        out.close();
    }

    static void place(StructureStart start, Structure structure, NoiseBasedChunkGenerator generator, long seed, int cx, int cz) throws Exception {
        FeatureOracleBase.Region region = FeatureOracleBase.newRegion(seed, cx, cz, ChunkStatus.FEATURES, 2);
        FeatureOracleBase.Snapshot before = FeatureOracleBase.snapshot(region);
        ChunkPos chunkPos = new ChunkPos(cx, cz);
        WorldgenRandom random = new WorldgenRandom(new XoroshiroRandomSource(0L));
        long decorationSeed = random.setDecorationSeed(seed, chunkPos.getMinBlockX(), chunkPos.getMinBlockZ());
        random.setFeatureSeed(decorationSeed, 7, 4);
        BoundingBox chunkBB = new BoundingBox(chunkPos.getMinBlockX(), FeatureOracleBase.MIN_Y + 1, chunkPos.getMinBlockZ(), chunkPos.getMinBlockX() + 15, FeatureOracleBase.MIN_Y + FeatureOracleBase.HEIGHT - 1, chunkPos.getMinBlockZ() + 15);
        start.placeInChunk(region, null, generator, random, chunkBB, chunkPos);
        out.println("PLACE " + cx + " " + cz);
        List<String> lines = FeatureOracleBase.dump(region, before);
        Map<Character, FeatureOracleBase.Fnv> digests = new TreeMap<>();
        for (String line : lines) {
            char c = line.charAt(0);
            if (c == 'B' || c == 'K') {
                digests.computeIfAbsent(c, k -> new FeatureOracleBase.Fnv()).s(line);
            } else if (c == 'N') {
                out.println("D " + line.replaceAll("UUID:\\[I;[-0-9,]*\\]", "UUID:[I;0,0,0,0]"));
            } else {
                out.println("D " + line);
            }
        }
        for (var e : digests.entrySet()) out.println("DG " + e.getKey() + " " + e.getValue().hex() + " " + e.getValue().count);
    }
}
