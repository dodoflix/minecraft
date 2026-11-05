import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.stream.*;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Vec3i;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.Mirror;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;
import net.minecraft.world.phys.Vec3;

/**
 * Oracle of crates/mc-game/tests/structure_template_golden.rs: the real {@code StructureTemplate.placeInWorld} (with the
 * real processors) over the synthetic world of FeatureOracleBase for every template of the vanilla data pack, plus the
 * static transform math.
 * Usage (repo root): scripts/vanilla-java.sh crates/mc-game/tests/oracle/StructureTemplateOracle.java crates/mc-game/tests/fixtures/structure_template_place.txt [templateFilter] (then gzip -9 -f the fixture) [fullDumpScenarioId]
 *
 * Output lines:
 *   X <kind> ...                          static math (transform / zero position / bounding box)
 *   SC <id> <template> <seed> <cx> <cz> <rot> <mirror> <pivotX> <pivotZ> <x> <y> <z> <refX> <refY> <refZ> <ignoreEntities> <knownShape>
 *      <liquid> <bb|-> <settingsRandomSeed|-> <processors|-> <flags> <rngSeed> <paletteCount>
 *   R <true|false|EXC:class>              placeInWorld result
 *   Q <size> <bb> <jigsawDigest> <filterDigest>    template queries (getSize(rotation), getBoundingBox, getJigsaws, filterBlocks)
 *   RNG <next long of the random argument>
 *   DG <category> <fnv hex> <count>       digests of the B/E/H/P/K/G lines of FeatureOracleBase.dump
 *   END
 * Entities: with the null level of the harness vanilla cannot create entities (EntityType.create throws inside
 * createEntityIgnoreException and is swallowed), so entity placement is not part of the digests.
 */
public class StructureTemplateOracle {
    static final String SCENARIO_KEY_SEP = ";";

    static RegistryOps<com.google.gson.JsonElement> ops;

    static StructureProcessor processorFromJson(String json) {
        return StructureProcessorType.SINGLE_CODEC.parse(ops, JsonParser.parseString(json)).getOrThrow();
    }

    static List<StructureProcessor> processors(String spec) {
        List<StructureProcessor> out = new ArrayList<>();
        if (spec.equals("-")) return out;
        for (String s : spec.split(SCENARIO_KEY_SEP)) {
            if (s.startsWith("list:")) {
                var key = ResourceKey.create(Registries.PROCESSOR_LIST, Identifier.parse(s.substring(5)));
                out.addAll(FeatureOracleBase.ACCESS.lookupOrThrow(Registries.PROCESSOR_LIST).getOrThrow(key).value().list());
            } else if (s.startsWith("json:")) {
                out.add(processorFromJson(s.substring(5)));
            } else throw new IllegalArgumentException(s);
        }
        return out;
    }

    static final String[] JSON_PROCESSORS = {
        "{\"processor_type\":\"minecraft:block_ignore\",\"blocks\":[{\"id\":\"minecraft:air\"},{\"id\":\"minecraft:structure_block\"}]}",
        "{\"processor_type\":\"minecraft:jigsaw_replacement\"}",
        "{\"processor_type\":\"minecraft:gravity\",\"heightmap\":\"WORLD_SURFACE_WG\",\"offset\":-1}",
        "{\"processor_type\":\"minecraft:gravity\",\"heightmap\":\"OCEAN_FLOOR\",\"offset\":2}",
        "{\"processor_type\":\"minecraft:block_rot\",\"integrity\":0.6}",
        "{\"processor_type\":\"minecraft:block_rot\",\"integrity\":0.8,\"rottable_blocks\":\"#minecraft:planks\"}",
        "{\"processor_type\":\"minecraft:block_age\",\"mossiness\":0.5}",
        "{\"processor_type\":\"minecraft:blackstone_replace\"}",
        "{\"processor_type\":\"minecraft:lava_submerged_block\"}",
        "{\"processor_type\":\"minecraft:protected_blocks\",\"value\":\"#minecraft:features_cannot_replace\"}",
        "{\"processor_type\":\"minecraft:nop\"}",
        "{\"processor_type\":\"minecraft:capped\",\"limit\":{\"type\":\"minecraft:uniform\",\"min_inclusive\":1,\"max_inclusive\":6},\"delegate\":{\"processor_type\":\"minecraft:block_rot\",\"integrity\":0.0}}",
        "{\"processor_type\":\"minecraft:capped\",\"limit\":5,\"delegate\":{\"processor_type\":\"minecraft:blackstone_replace\"}}",
        "{\"processor_type\":\"minecraft:rule\",\"rules\":[{\"input_predicate\":{\"predicate_type\":\"minecraft:random_block_match\",\"block\":\"minecraft:stone_bricks\",\"probability\":0.5},\"location_predicate\":{\"predicate_type\":\"minecraft:always_true\"},\"position_predicate\":{\"predicate_type\":\"minecraft:linear_pos\",\"min_chance\":0.1,\"max_chance\":0.9,\"min_dist\":0,\"max_dist\":12},\"output_state\":{\"id\":\"minecraft:mossy_stone_bricks\"}},{\"input_predicate\":{\"predicate_type\":\"minecraft:tag_match\",\"tag\":\"minecraft:logs\"},\"location_predicate\":{\"predicate_type\":\"minecraft:not\",\"rule\":{\"predicate_type\":\"minecraft:block_match\",\"block\":\"minecraft:stone\"}},\"position_predicate\":{\"predicate_type\":\"minecraft:axis_aligned_linear_pos\",\"axis\":\"y\",\"min_chance\":0.0,\"max_chance\":1.0,\"min_dist\":2,\"max_dist\":9},\"output_state\":{\"id\":\"minecraft:oak_planks\"},\"block_entity_modifier\":{\"type\":\"minecraft:clear\"}},{\"input_predicate\":{\"predicate_type\":\"minecraft:any_of\",\"rules\":[{\"predicate_type\":\"minecraft:blockstate_match\",\"block_state\":{\"id\":\"minecraft:chest\",\"properties\":{\"facing\":\"north\",\"type\":\"single\",\"waterlogged\":\"false\"}}},{\"predicate_type\":\"minecraft:block_match\",\"block\":\"minecraft:barrel\"}]},\"location_predicate\":{\"predicate_type\":\"minecraft:height_match\",\"min_inclusive\":0,\"max_inclusive\":200},\"output_state\":{\"id\":\"minecraft:barrel\",\"properties\":{\"facing\":\"up\",\"open\":\"false\"}},\"block_entity_modifier\":{\"type\":\"minecraft:append_loot\",\"loot_table\":\"minecraft:chests/simple_dungeon\"}}]}",
    };


    /** Stub for {@code getLevel()}: only {@code getSeed()} is used by the processors (CappedProcessor). */
    static class FakeServerLevel extends net.minecraft.server.level.ServerLevel {
        long fakeSeed;

        FakeServerLevel() {
            super(null, null, null, null, null, null, false, 0L, List.of(), false);
        }

        @Override
        public long getSeed() {
            return fakeSeed;
        }
    }

    /** One region per seed (building the synthetic chunks dominates the run time): reset to the snapshot between scenarios. */
    static final Map<Long, FeatureOracleBase.Region> REGIONS = new HashMap<>();
    static final Map<Long, FeatureOracleBase.Snapshot> SNAPSHOTS = new HashMap<>();

    static FeatureOracleBase.Region regionFor(long seed, int cx, int cz) throws Exception {
        FeatureOracleBase.Region region = REGIONS.get(seed);
        if (region == null) {
            region = FeatureOracleBase.newRegion(seed, cx, cz, ChunkStatus.FEATURES, 2);
            java.lang.reflect.Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
            uf.setAccessible(true);
            FakeServerLevel level = (FakeServerLevel) ((sun.misc.Unsafe) uf.get(null)).allocateInstance(FakeServerLevel.class);
            level.fakeSeed = seed;
            region.serverLevel = level;
            // the static environment attribute layers (dimension + biomes) of the overworld: LiquidBlock.updateShape reads fast_lava
            FeatureOracleBase.setField(region, "dimensionType", FeatureOracleBase.ACCESS.lookupOrThrow(Registries.DIMENSION_TYPE)
                .getOrThrow(net.minecraft.world.level.dimension.BuiltinDimensionTypes.OVERWORLD).value());
            FeatureOracleBase.setField(region, "environmentAttributes", net.minecraft.world.attribute.EnvironmentAttributeSystem.builder().addStaticLayers(region).build());
            REGIONS.put(seed, region);
            SNAPSHOTS.put(seed, FeatureOracleBase.snapshot(region));
            return region;
        }
        FeatureOracleBase.Snapshot before = SNAPSHOTS.get(seed);
        for (net.minecraft.world.level.chunk.ProtoChunk c : FeatureOracleBase.sortedChunks(region)) {
            net.minecraft.world.level.block.state.BlockState[] was = before.blocks.get(c.getPos().pack());
            net.minecraft.world.level.block.state.BlockState[] now = FeatureOracleBase.blocksOf(c);
            BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
            for (int i = 0; i < now.length; i++)
                if (now[i] != was[i]) c.setBlockState(p.set(c.getPos().getMinBlockX() + (i & 15), FeatureOracleBase.MIN_Y + (i >> 8), c.getPos().getMinBlockZ() + ((i >> 4) & 15)), was[i], 0);
            for (BlockPos bp : new ArrayList<>(c.getBlockEntitiesPos())) c.removeBlockEntity(bp);
            for (var sl : c.getPostProcessing()) if (sl != null) sl.clear();
            c.getEntities().clear();
            for (Object ticks : new Object[] {c.getBlockTicks(), c.getFluidTicks()}) {
                for (String field : new String[] {"ticks", "ticksPerPosition"}) {
                    java.lang.reflect.Field f = net.minecraft.world.ticks.ProtoChunkTicks.class.getDeclaredField(field);
                    f.setAccessible(true);
                    ((java.util.Collection<?>) f.get(ticks)).clear();
                }
            }
        }
        FeatureOracleBase.setField(region, "random", FeatureOracleBase.randomState(seed).getOrCreateRandomFactory(Identifier.withDefaultNamespace("worldgen_region_random")).at(new ChunkPos(cx, cz).getWorldPosition()));
        FeatureOracleBase.setField(region, "subTickCount", new java.util.concurrent.atomic.AtomicLong());
        return region;
    }

    static String fnvLines(List<String> lines, char kind) {
        FeatureOracleBase.Fnv f = new FeatureOracleBase.Fnv();
        int n = 0;
        for (String l : lines) {
            if (l.charAt(0) == kind && l.charAt(1) == ' ') {
                f.s(l);
                n++;
            }
        }
        return f.hex() + " " + n;
    }

    static String bbString(BoundingBox bb) {
        return bb == null ? "-" : bb.minX() + "," + bb.minY() + "," + bb.minZ() + "," + bb.maxX() + "," + bb.maxY() + "," + bb.maxZ();
    }

    static String blockInfoDigest(List<StructureTemplate.StructureBlockInfo> infos) {
        FeatureOracleBase.Fnv f = new FeatureOracleBase.Fnv();
        for (var i : infos) f.s(i.pos().getX() + " " + i.pos().getY() + " " + i.pos().getZ() + " " + BlockStateParser.serialize(i.state()) + " " + i.nbt());
        return f.hex() + "/" + f.count;
    }

    static String jigsawDigest(List<StructureTemplate.JigsawBlockInfo> infos) {
        FeatureOracleBase.Fnv f = new FeatureOracleBase.Fnv();
        for (var i : infos)
            f.s(i.pos().getX() + " " + i.pos().getY() + " " + i.pos().getZ() + " " + BlockStateParser.serialize(i.state()) + " " + i.jointType().getSerializedName() + " " + i.name() + " "
                + i.pool().identifier() + " " + i.target() + " " + i.placementPriority() + " " + i.selectionPriority());
        return f.hex() + "/" + f.count;
    }

    static StructureTemplate load(Path file) throws IOException {
        CompoundTag tag = NbtIo.readCompressed(file, NbtAccounter.unlimitedHeap());
        StructureTemplate t = new StructureTemplate();
        t.load(BuiltInRegistries.BLOCK, tag);
        return t;
    }

    static void staticMath(PrintWriter out) {
        Random r = new Random(0x5717L);
        Rotation[] rots = Rotation.values();
        Mirror[] mirrors = Mirror.values();
        for (int n = 0; n < 600; n++) {
            Rotation rot = rots[r.nextInt(4)];
            Mirror mir = mirrors[r.nextInt(3)];
            BlockPos pivot = new BlockPos(r.nextInt(41) - 20, 0, r.nextInt(41) - 20);
            BlockPos p = new BlockPos(r.nextInt(201) - 100, r.nextInt(300) - 64, r.nextInt(201) - 100);
            BlockPos t = StructureTemplate.transform(p, mir, rot, pivot);
            out.println("X pos " + p.getX() + " " + p.getY() + " " + p.getZ() + " " + mir.name() + " " + rot.name() + " " + pivot.getX() + " " + pivot.getZ() + " " + t.getX() + " " + t.getY() + " " + t.getZ());
            Vec3 v = new Vec3(r.nextInt(201) - 100 + r.nextDouble(), r.nextInt(300) - 64 + r.nextDouble(), r.nextInt(201) - 100 + r.nextDouble());
            Vec3 tv = StructureTemplate.transform(v, mir, rot, pivot);
            out.println("X vec " + Long.toHexString(Double.doubleToRawLongBits(v.x)) + " " + Long.toHexString(Double.doubleToRawLongBits(v.y)) + " " + Long.toHexString(Double.doubleToRawLongBits(v.z)) + " " + mir.name() + " " + rot.name()
                + " " + pivot.getX() + " " + pivot.getZ() + " " + Long.toHexString(Double.doubleToRawLongBits(tv.x)) + " " + Long.toHexString(Double.doubleToRawLongBits(tv.y)) + " " + Long.toHexString(Double.doubleToRawLongBits(tv.z)));
            int sx = 1 + r.nextInt(60), sz = 1 + r.nextInt(60), sy = 1 + r.nextInt(40);
            BlockPos z = StructureTemplate.getZeroPositionWithTransform(p, mir, rot, sx, sz);
            out.println("X zero " + p.getX() + " " + p.getY() + " " + p.getZ() + " " + mir.name() + " " + rot.name() + " " + sx + " " + sz + " " + z.getX() + " " + z.getY() + " " + z.getZ());
        }
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        // what the server does after loading the data packs: bind the item/block default components (block entity items, vault)
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(FeatureOracleBase.ACCESS).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        ops = RegistryOps.create(JsonOps.INSTANCE, FeatureOracleBase.ACCESS);
        String filter = args.length > 1 ? args[1] : null;
        int fullDumpId = args.length > 2 ? Integer.parseInt(args[2]) : -1;
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        if (filter == null) staticMath(out);
        Path root = Paths.get("ref/src/data/minecraft/structure");
        List<Path> files;
        try (Stream<Path> s = Files.walk(root)) {
            files = s.filter(p -> p.toString().endsWith(".nbt")).sorted().collect(Collectors.toList());
        }
        Rotation[] rots = Rotation.values();
        Mirror[] mirrors = Mirror.values();
        String[] liquids = {"APPLY_WATERLOGGING", "IGNORE_WATERLOGGING"};
        int[] flagChoices = {2, 18, 3, 19, 2, 4};
        List<String> listNames = FeatureOracleBase.ACCESS.lookupOrThrow(Registries.PROCESSOR_LIST).keySet().stream().map(Identifier::toString).sorted().collect(Collectors.toList());
        int id = 0;
        for (int ti = 0; ti < files.size(); ti++) {
            String name = root.relativize(files.get(ti)).toString().replace(File.separatorChar, '/');
            name = "minecraft:" + name.substring(0, name.length() - 4);
            StructureTemplate template = load(files.get(ti));
            boolean full = ti % 25 == 0;
            int variants = full ? 12 : 3;
            for (int v = 0; v < variants; v++) {
                int scenario = id++;
                if (filter != null && !name.contains(filter)) continue;
                Random r = new Random(ti * 1000003L + v * 7919L + 17);
                Rotation rot = full ? rots[v % 4] : rots[(ti + v * 3) % 4];
                Mirror mir = full ? mirrors[v / 4] : mirrors[(ti / 3 + v * 2) % 3];
                long seed = 1000 + r.nextInt(6);
                int cx = (int) (seed % 7) - 3, cz = (int) (seed % 5) - 2;
                Vec3i size = template.getSize();
                int span = Math.max(size.getX(), size.getZ());
                int px = r.nextInt(4) == 0 ? r.nextInt(Math.max(1, size.getX())) : 0, pz = r.nextInt(4) == 0 ? r.nextInt(Math.max(1, size.getZ())) : 0;
                // the writable zone of the FEATURES step is the 3x3 chunks around the center: x in [cx*16-16, cx*16+31]; keep a one
                // block margin so that neighbour reads stay inside the readable zone (larger templates stick out on purpose)
                int x = cx * 16 - 15 + (span < 46 ? r.nextInt(46 - span) : -r.nextInt(8));
                int y = 50 + r.nextInt(26);
                int z = cz * 16 - 15 + (span < 46 ? r.nextInt(46 - span) : -r.nextInt(8));
                boolean ignoreEntities = r.nextInt(3) == 0;
                boolean knownShape = r.nextInt(4) == 0;
                String liquid = liquids[r.nextInt(5) == 0 ? 1 : 0];
                String bb = "-";
                if (r.nextInt(4) == 0) {
                    int x0 = cx * 16 - 16 + r.nextInt(32), z0 = cz * 16 - 16 + r.nextInt(32);
                    bb = x0 + "," + (40 + r.nextInt(20)) + "," + z0 + "," + (x0 + 8 + r.nextInt(30)) + "," + (80 + r.nextInt(30)) + "," + (z0 + 8 + r.nextInt(30));
                }
                String settingsRandom = r.nextInt(4) == 0 ? "" + r.nextInt(100000) : "-";
                String procSpec = "-";
                int pk = r.nextInt(10);
                if (pk < 2) procSpec = "list:" + listNames.get(r.nextInt(listNames.size()));
                else if (pk < 6) {
                    int n = 1 + r.nextInt(3);
                    List<String> specs = new ArrayList<>();
                    for (int k = 0; k < n; k++) specs.add("json:" + JSON_PROCESSORS[r.nextInt(JSON_PROCESSORS.length)]);
                    procSpec = String.join(SCENARIO_KEY_SEP, specs);
                }
                int flags = flagChoices[r.nextInt(flagChoices.length)];
                long rngSeed = r.nextInt(1000000);
                out.println("SC " + scenario + " " + name + " " + seed + " " + cx + " " + cz + " " + rot.name() + " " + mir.name() + " " + px + " " + pz + " " + x + " " + y + " " + z + " " + x + " " + y + " " + z + " "
                    + ignoreEntities + " " + knownShape + " " + liquid + " " + bb + " " + settingsRandom + " " + procSpec + " " + flags + " " + rngSeed + " " + 1);
                out.flush();
                FeatureOracleBase.Region region = regionFor(seed, cx, cz);
                FeatureOracleBase.Snapshot before = SNAPSHOTS.get(seed);
                StructurePlaceSettings settings = new StructurePlaceSettings().setRotation(rot).setMirror(mir).setRotationPivot(new BlockPos(px, 0, pz))
                    .setIgnoreEntities(ignoreEntities).setKnownShape(knownShape).setLiquidSettings(LiquidSettings.valueOf(liquid));
                if (!bb.equals("-")) {
                    String[] b = bb.split(",");
                    settings.setBoundingBox(new BoundingBox(Integer.parseInt(b[0]), Integer.parseInt(b[1]), Integer.parseInt(b[2]), Integer.parseInt(b[3]), Integer.parseInt(b[4]), Integer.parseInt(b[5])));
                }
                if (!settingsRandom.equals("-")) settings.setRandom(RandomSource.create(Long.parseLong(settingsRandom)));
                for (StructureProcessor p : processors(procSpec)) settings.addProcessor(p);
                BlockPos pos = new BlockPos(x, y, z);
                RandomSource random = RandomSource.create(rngSeed);
                String result;
                try {
                    result = "" + template.placeInWorld(region, pos, pos, settings, random, flags);
                } catch (Throwable e) {
                    result = "EXC:" + e.getClass().getSimpleName();
                    if (filter != null) e.printStackTrace(); // with a template filter: show why vanilla throws
                }
                out.println("R " + result);
                StructurePlaceSettings q = new StructurePlaceSettings().setRotation(rot).setMirror(mir).setRotationPivot(new BlockPos(px, 0, pz));
                if (!bb.equals("-")) q.setBoundingBox(settings.getBoundingBox());
                Vec3i rs = template.getSize(rot);
                out.println("Q " + rs.getX() + "," + rs.getY() + "," + rs.getZ() + " " + bbString(template.getBoundingBox(q, pos)) + " " + jigsawDigest(template.getJigsaws(pos, rot)) + " "
                    + blockInfoDigest(template.filterBlocks(pos, q, Blocks.CHEST)) + " " + blockInfoDigest(template.filterBlocks(pos, q, Blocks.JIGSAW, false)));
                out.println("RNG " + random.nextLong());
                List<String> dump = FeatureOracleBase.dump(region, before);
                for (char k : new char[] {'B', 'E', 'H', 'P', 'K', 'G'}) out.println("DG " + k + " " + fnvLines(dump, k));
                if (scenario == fullDumpId) for (String l : dump) System.err.println("D " + l);
                out.println("END");
                out.flush();
            }
        }
        out.close();
    }
}
