import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.RegistryLayer;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureSpawnOverride;
import net.minecraft.core.registries.BuiltInRegistries;

/** Golden for `Structure.StructureSettings.CODEC`: every line of the input (one JSON object) is decoded and dumped as
 *  `json TAB ok biomes=..;overrides=..;step=..;adapt=..` or `json TAB err`.
 *  Run: scripts/vanilla-java.sh crates/mc-game/tests/oracle/StructureSettingsOracle.java crates/mc-game/tests/fixtures/structure_settings_cases.in crates/mc-game/tests/fixtures/structure_settings_cases.golden */
public class StructureSettingsOracle {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        java.util.concurrent.Executor direct = Runnable::run;
        MultiPackResourceManager resources = new MultiPackResourceManager(PackType.SERVER_DATA, List.of(ServerPacksSource.createVanillaPackSource().fullResources()));
        LayeredRegistryAccess<RegistryLayer> layers = RegistryLayer.createRegistryAccess();
        List<Registry.PendingTags<?>> staticTags = TagLoader.loadTagsForExistingRegistries(resources, layers.getLayer(RegistryLayer.STATIC));
        staticTags.forEach(Registry.PendingTags::apply);
        var worldContext = TagLoader.buildUpdatedLookups(layers.getAccessForLoading(RegistryLayer.WORLD), staticTags);
        var world = RegistryDataLoader.load(resources, worldContext, RegistryDataLoader.WORLD_REGISTRIES, direct).join();
        List<HolderLookup.RegistryLookup<?>> dimContext = new ArrayList<>(worldContext);
        world.listRegistries().forEach(dimContext::add);
        var dims = RegistryDataLoader.load(resources, dimContext, RegistryDataLoader.DIMENSION_REGISTRIES, direct).join();
        RegistryAccess access = layers.replaceFrom(RegistryLayer.WORLD, world, dims).compositeAccess();
        RegistryOps<JsonElement> ops = RegistryOps.create(JsonOps.INSTANCE, access);
        var biomeRegistry = access.lookupOrThrow(Registries.BIOME);

        StringBuilder out = new StringBuilder();
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            if (line.isBlank()) continue;
            String result;
            try {
                var parsed = Structure.StructureSettings.CODEC.codec().parse(ops, JsonParser.parseString(line)).result();
                if (parsed.isEmpty()) {
                    result = "err";
                } else {
                    var s = parsed.get();
                    StringBuilder b = new StringBuilder("ok biomes=");
                    List<String> ids = new ArrayList<>();
                    s.biomes().stream().forEach(h -> ids.add(h.unwrapKey().get().identifier().toString()));
                    b.append(String.join(",", ids)).append(";overrides=");
                    List<String> overrides = new ArrayList<>();
                    for (var e : s.spawnOverrides().entrySet()) {
                        StringBuilder o = new StringBuilder(e.getKey().getSerializedName()).append(':').append(e.getValue().boundingBox().getSerializedName()).append('[');
                        List<String> spawns = new ArrayList<>();
                        for (var w : e.getValue().spawns().unwrap()) {
                            spawns.add(BuiltInRegistries.ENTITY_TYPE.getKey(w.value().type()) + "*" + w.value().count().minInclusive() + "-" + w.value().count().maxInclusive() + "@" + w.weight());
                        }
                        o.append(String.join(",", spawns)).append(']');
                        overrides.add(o.toString());
                    }
                    Collections.sort(overrides);
                    b.append(String.join("|", overrides)).append(";step=").append(s.step().getSerializedName()).append(";adapt=").append(s.terrainAdaptation().getSerializedName());
                    result = b.toString();
                }
            } catch (RuntimeException e) {
                result = "err";
            }
            out.append(line).append('\t').append(result).append('\n');
        }
        Files.writeString(Path.of(args[1]), out.toString());
    }
}
