import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.Codec;
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
import net.minecraft.world.level.levelgen.feature.Feature;
import net.minecraft.world.level.levelgen.feature.featuresize.FeatureSize;
import net.minecraft.world.level.levelgen.feature.foliageplacers.FoliagePlacer;
import net.minecraft.world.level.levelgen.feature.rootplacers.RootPlacer;
import net.minecraft.world.level.levelgen.feature.treedecorators.TreeDecorator;
import net.minecraft.world.level.levelgen.feature.trunkplacers.TrunkPlacer;

/** Golden for the tree codecs: runs every case of fixtures/tree_codec_cases.in (kind TAB json) through the vanilla codec and
 *  writes `kind TAB json TAB ok|err` lines (vanilla redirects System.out into its logger, hence the output file).
 *  Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/tree_codec_cases.golden crates/mc-game/tests/oracle/TreeCodecOracle.java crates/mc-game/tests/fixtures/tree_codec_cases.in crates/mc-game/tests/fixtures/tree_codec_cases.golden
 *  Output: crates/mc-game/tests/fixtures/tree_codec_cases.golden */
public class TreeCodecOracle {
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

        Map<String, Codec<?>> codecs = Map.of("trunk", TrunkPlacer.CODEC, "foliage", FoliagePlacer.CODEC, "size", FeatureSize.CODEC,
            "root", RootPlacer.CODEC, "decorator", TreeDecorator.CODEC, "feature", Feature.DIRECT_CODEC);
        StringBuilder out = new StringBuilder();
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            if (line.isBlank()) continue;
            int tab = line.indexOf('\t');
            String kind = line.substring(0, tab), json = line.substring(tab + 1);
            boolean ok;
            try {
                ok = codecs.get(kind).parse(ops, JsonParser.parseString(json)).result().isPresent();
            } catch (RuntimeException e) {
                ok = false;
            }
            out.append(kind).append('\t').append(json).append('\t').append(ok ? "ok" : "err").append('\n');
        }
        Files.writeString(Path.of(args[1]), out.toString());
    }
}
