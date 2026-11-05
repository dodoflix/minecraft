import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.Codec;
import com.mojang.serialization.JsonOps;
import com.mojang.serialization.DataResult;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.*;
import net.minecraft.resources.RegistryOps;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.pools.*;
import net.minecraft.world.level.levelgen.structure.pools.alias.PoolAliasBinding;
import net.minecraft.world.level.levelgen.structure.structures.JigsawStructure;

/**
 * Golden of the jigsaw codecs: every case of fixtures/jigsaw_codec_cases.in (kind TAB json) through the vanilla codec ->
 * {@code kind TAB json TAB ok|err[ TAB canonical NbtOps encoding]} (pool elements are re-encoded to NBT, the form saved in
 * jigsaw pieces), plus one {@code POOL} line per vanilla template pool: id, fallback, element count and a digest of the
 * weighted NBT encodings of its elements.
 * Run (repo root): scripts/vanilla-java.sh crates/mc-game/tests/oracle/JigsawCodecOracle.java crates/mc-game/tests/fixtures/jigsaw_codec_cases.in crates/mc-game/tests/fixtures/jigsaw_codec_cases.golden
 */
public class JigsawCodecOracle {
    /** Sorted-key canonical text of a tag (mirrored by the Rust test). */
    public static String canon(Tag t) {
        if (t instanceof CompoundTag c) {
            StringBuilder sb = new StringBuilder("{");
            for (String k : new TreeSet<>(c.keySet())) sb.append(sb.length() > 1 ? "," : "").append(k).append(':').append(canon(c.get(k)));
            return sb.append('}').toString();
        } else if (t instanceof ListTag l) {
            StringBuilder sb = new StringBuilder("[");
            for (int i = 0; i < l.size(); i++) sb.append(i > 0 ? "," : "").append(canon(l.get(i)));
            return sb.append(']').toString();
        } else if (t instanceof StringTag s) {
            return "\"" + s.value() + "\"";
        } else if (t instanceof ByteTag b) { return b.value() + "b";
        } else if (t instanceof ShortTag b) { return b.value() + "s";
        } else if (t instanceof IntTag b) { return b.value() + "i";
        } else if (t instanceof LongTag b) { return b.value() + "l";
        } else if (t instanceof FloatTag b) { return b.value() + "f";
        } else if (t instanceof DoubleTag b) { return b.value() + "d";
        }
        return t.toString();
    }

    public static void main(String[] args) throws Exception {
        FeatureOracleBase.setup();
        RegistryAccess access = FeatureOracleBase.ACCESS;
        RegistryOps<JsonElement> ops = RegistryOps.create(JsonOps.INSTANCE, access);
        RegistryOps<Tag> nbt = RegistryOps.create(NbtOps.INSTANCE, access);
        Map<String, Codec<?>> codecs = Map.of(
            "alias", PoolAliasBinding.CODEC,
            "padding", DimensionPadding.CODEC,
            "maxdist", JigsawStructure.MaxDistance.CODEC,
            "element", StructurePoolElement.CODEC,
            "pool", StructureTemplatePool.DIRECT_CODEC,
            "structure", Structure.DIRECT_CODEC);
        StringBuilder out = new StringBuilder();
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            if (line.isBlank()) continue;
            int tab = line.indexOf('\t');
            String kind = line.substring(0, tab), json = line.substring(tab + 1);
            String result;
            try {
                @SuppressWarnings("unchecked")
                Codec<Object> codec = (Codec<Object>) codecs.get(kind);
                DataResult<Object> parsed = codec.parse(ops, JsonParser.parseString(json));
                if (parsed.result().isEmpty()) {
                    result = "err";
                } else if (kind.equals("element")) {
                    result = "ok\t" + canon(codec.encodeStart(nbt, parsed.result().get()).getOrThrow());
                } else {
                    result = "ok";
                }
            } catch (RuntimeException e) {
                result = "err";
            }
            out.append(kind).append('\t').append(json).append('\t').append(result).append('\n');
        }
        Registry<StructureTemplatePool> pools = access.lookupOrThrow(Registries.TEMPLATE_POOL);
        List<String> ids = new ArrayList<>();
        pools.keySet().forEach(k -> ids.add(k.toString()));
        Collections.sort(ids);
        for (String id : ids) {
            StructureTemplatePool pool = pools.getValue(net.minecraft.resources.Identifier.parse(id));
            FeatureOracleBase.Fnv f = new FeatureOracleBase.Fnv();
            for (var pair : pool.getTemplates()) {
                f.s(pair.getSecond() + "|" + canon(StructurePoolElement.CODEC.encodeStart(nbt, pair.getFirst()).getOrThrow()));
            }
            out.append("POOL\t").append(id).append('\t').append(pool.getFallback().unwrapKey().orElseThrow().identifier()).append('\t')
                .append(pool.getTemplates().size()).append('\t').append(f.hex()).append('\n');
        }
        Files.writeString(Path.of(args[1]), out.toString());
    }
}
