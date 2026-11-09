import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.stream.Stream;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.core.component.DataComponentMap;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.TypedDataComponent;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.nbt.TagParser;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;

/** Golden for every registered block entity type: a fresh entity's `saveWithFullMetadata`, `loadStatic` (id/x/y/z + the case's
 *  NBT) followed by `saveWithFullMetadata`, `getUpdateTag`, the presence of an update packet, `collectComponents`, the
 *  place-from-item path (`applyComponentsFromItemStack` of a stack with the case's component patch on a fresh entity) and the
 *  component round trip (collected components applied to a fresh entity). Cases come from `*.txt` files in the cases directory:
 *  name TAB block TAB props TAB input-snbt TAB component-patch-snbt, `-` for nothing, `#` starts a comment line.
 *  Output line: the five input columns, then saved-hex, saved-snbt, update-hex, has-update-packet, collected-hex,
 *  collected-snbt, applied-hex, applied-snbt, applied-collected-hex, roundtrip-hex, roundtrip-collected-hex, pick-hex (`saveCustomOnly` minus `removeComponentsFromTag`) (the applied
 *  columns are `-` without a patch). The update packet of every vanilla type that has one is `create(this)`: its tag is the update tag.
 *  Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/container_block_entities.txt.gz crates/mc-game/tests/oracle/ContainerBlockEntitiesOracle.java crates/mc-game/tests/fixtures/container_block_entities.txt.gz crates/mc-game/tests/oracle/container_block_entities_cases
 *  Output: crates/mc-game/tests/fixtures/container_block_entities.txt.gz */
public class ContainerBlockEntitiesOracle {
    static final BlockPos POS = new BlockPos(10, 64, -20);
    static HolderLookup.Provider REG; // built-in and the datagen bootstrap of the dynamic registries (banner patterns, ...)
    static RegistryOps<Tag> OPS;
    static final StringBuilder OUT = new StringBuilder();
    static final Set<String> TYPES_SEEN = new TreeSet<>();
    static final List<String> ERRORS = new ArrayList<>(); // every failing case, reported together

    static BlockState state(String block, String props) {
        Block b = BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(block));
        if (BuiltInRegistries.BLOCK.getKey(b).getPath().equals("air") && !block.equals("air")) throw new IllegalStateException("unknown block " + block);
        BlockState s = b.defaultBlockState();
        if (!props.equals("-")) {
            for (String kv : props.split(",")) {
                String[] p = kv.split("=");
                Property<?> prop = b.getStateDefinition().getProperty(p[0]);
                if (prop == null) throw new IllegalStateException("unknown property " + kv + " of " + block);
                s = set(s, prop, p[1]);
            }
        }
        return s;
    }

    static <T extends Comparable<T>> BlockState set(BlockState s, Property<T> prop, String v) {
        return s.setValue(prop, prop.getValue(v).orElseThrow(() -> new IllegalStateException("bad value " + v + " of " + prop.getName())));
    }

    static BlockEntityType<?> typeOf(BlockState s) {
        for (BlockEntityType<?> t : BuiltInRegistries.BLOCK_ENTITY_TYPE) if (t.isValid(s)) return t;
        throw new IllegalStateException(s.toString());
    }

    static String hex(CompoundTag tag) throws Exception {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        NbtIo.write(tag, new DataOutputStream(bytes));
        StringBuilder sb = new StringBuilder();
        for (byte b : bytes.toByteArray()) sb.append(String.format(java.util.Locale.ROOT, "%02x", b & 0xFF));
        return sb.toString();
    }

    static CompoundTag encode(DataComponentMap map) {
        return (CompoundTag) DataComponentMap.CODEC.encodeStart(OPS, map).getOrThrow(IllegalStateException::new);
    }

    static DataComponentPatch patchOf(String snbt) throws Exception {
        CompoundTag tag = TagParser.parseCompoundFully(snbt);
        return DataComponentPatch.CODEC.parse(OPS, tag).getOrThrow(IllegalStateException::new);
    }

    /** `BlockItem.updateBlockEntityComponents`: the stack of the block's item with the patch, or the bare patch for blocks without an item. */
    static void apply(BlockEntity be, Block block, DataComponentPatch patch) {
        Item item = block.asItem();
        if (item == Items.AIR) {
            be.applyComponents(DataComponentMap.EMPTY, patch);
        } else {
            ItemStack stack = new ItemStack(item);
            stack.applyComponents(patch);
            be.applyComponentsFromItemStack(stack);
        }
    }

    static void applyMap(BlockEntity be, Block block, DataComponentMap map) {
        DataComponentPatch.Builder b = DataComponentPatch.builder();
        for (TypedDataComponent<?> c : map) b.set(c);
        apply(be, block, b.build());
    }

    /** The entity a placed block gets (`EntityBlock.newBlockEntity`: a command block copies its `automatic` flag), `type.create` for
     *  blocks that do not make one themselves. A moving piston has no direction until it is loaded (saving it fresh is a
     *  NullPointerException), so it loads `{}`. */
    static BlockEntity fresh(BlockEntityType<?> type, BlockState s) {
        BlockEntity be = s.getBlock() instanceof net.minecraft.world.level.block.EntityBlock eb ? eb.newBlockEntity(POS, s) : null;
        if (be == null) be = type.create(POS, s);
        if (type == net.minecraft.world.level.block.entity.BlockEntityTypes.PISTON) be.loadWithComponents(net.minecraft.world.level.storage.TagValueInput.create(net.minecraft.util.ProblemReporter.DISCARDING, REG, new CompoundTag()));
        return be;
    }

    /** `ServerGamePacketListenerImpl.addBlockDataToItem` before `setBlockEntityData`: `saveCustomOnly` minus the implicit components. */
    static CompoundTag pickTag(BlockEntity be) {
        net.minecraft.world.level.storage.TagValueOutput output = net.minecraft.world.level.storage.TagValueOutput.createWithContext(net.minecraft.util.ProblemReporter.DISCARDING, REG);
        be.saveCustomOnly(output);
        be.removeComponentsFromTag(output);
        return output.buildResult();
    }

    static void line(String name, String block, String props, String input, String patch) throws Exception {
        BlockState s = state(block, props);
        BlockEntityType<?> type = typeOf(s);
        TYPES_SEEN.add(BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(type).getPath());
        BlockEntity be;
        if (input.equals("-")) {
            be = fresh(type, s);
        } else {
            CompoundTag tag = TagParser.parseCompoundFully(input).copy(); // copy: the parser may hand out one shared instance for `{}`
            tag.putString("id", BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(type).toString());
            tag.putInt("x", POS.getX());
            tag.putInt("y", POS.getY());
            tag.putInt("z", POS.getZ());
            be = BlockEntity.loadStatic(POS, s, tag, REG);
            if (be == null) throw new IllegalStateException(name + ": loadStatic returned null");
        }
        CompoundTag saved = be.saveWithFullMetadata(REG);
        CompoundTag update = be.getUpdateTag(REG);
        CompoundTag collected = encode(be.collectComponents());
        String applied = "-", appliedSnbt = "-", appliedCollected = "-";
        if (!patch.equals("-")) {
            BlockEntity a = fresh(type, s);
            apply(a, s.getBlock(), patchOf(patch));
            applied = hex(a.saveWithFullMetadata(REG));
            appliedSnbt = a.saveWithFullMetadata(REG).toString();
            appliedCollected = hex(encode(a.collectComponents()));
        }
        BlockEntity rt = fresh(type, s);
        applyMap(rt, s.getBlock(), be.collectComponents());
        OUT.append(name).append('\t').append(block).append('\t').append(props).append('\t').append(input).append('\t').append(patch).append('\t')
            .append(hex(saved)).append('\t').append(saved).append('\t').append(hex(update)).append('\t')
            .append(be.getClass().getMethod("getUpdatePacket").getDeclaringClass() != BlockEntity.class).append('\t')
            .append(hex(collected)).append('\t').append(collected).append('\t')
            .append(applied).append('\t').append(appliedSnbt).append('\t').append(appliedCollected).append('\t')
            .append(hex(rt.saveWithFullMetadata(REG))).append('\t').append(hex(encode(rt.collectComponents()))).append('\t').append(hex(pickTag(be))).append('\n');
    }

    public static void main(String[] args) throws Exception {
        try { run(args[0], args[1]); } catch (Throwable t) { t.printStackTrace(); System.exit(1); }
    }

    static void run(String out, String casesDir) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        REG = VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(REG).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply); // binds the default item components
        OPS = REG.createSerializationContext(NbtOps.INSTANCE);

        List<Path> files;
        try (Stream<Path> s = Files.list(Path.of(casesDir))) { files = s.filter(p -> p.toString().endsWith(".txt")).sorted().toList(); }
        Set<String> names = new HashSet<>();
        for (Path f : files) {
            for (String l : Files.readAllLines(f)) {
                if (l.isBlank() || l.startsWith("#")) continue;
                String[] c = l.split("\t", -1);
                if (c.length != 5) throw new IllegalStateException(f + ": expected 5 columns: " + l);
                if (!names.add(c[0])) throw new IllegalStateException("duplicate case " + c[0]);
                try {
                    line(c[0], c[1], c[2], c[3], c[4]);
                } catch (Throwable t) {
                    ERRORS.add(f.getFileName() + ": case " + c[0] + ": " + t);
                }
            }
        }
        if (!ERRORS.isEmpty()) throw new IllegalStateException(ERRORS.size() + " cases failed:\n" + String.join("\n", ERRORS));
        for (BlockEntityType<?> t : BuiltInRegistries.BLOCK_ENTITY_TYPE) {
            String id = BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(t).getPath();
            if (!TYPES_SEEN.contains(id)) throw new IllegalStateException("no case for block entity type " + id);
        }
        Files.writeString(Path.of(out), OUT.toString());
    }
}
