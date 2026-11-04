package extractor;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.Executor;
import net.minecraft.SharedConstants;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.LayeredRegistryAccess;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.RegistryLayer;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.tags.TagLoader;

/**
 * Boots the vanilla server classes the way a dedicated server does: registries, vanilla data pack tags bound on the
 * static registries, dynamic registries loaded from the data pack, default item components applied.
 * Also used by golden-value generators in other crates' tests (classpath: tools/extractor/build).
 */
public final class Setup {
   private Setup() {
   }

   /** @return all registries (static + data-pack-loaded) */
   public static RegistryAccess.Frozen bootstrap() throws Exception {
      SharedConstants.tryDetectVersion();
      Bootstrap.bootStrap();
      Executor direct = Runnable::run;
      MultiPackResourceManager resources = new MultiPackResourceManager(
         PackType.SERVER_DATA, List.of(ServerPacksSource.createVanillaPackSource().fullResources())
      );
      LayeredRegistryAccess<RegistryLayer> layers = RegistryLayer.createRegistryAccess();
      List<Registry.PendingTags<?>> staticTags = TagLoader.loadTagsForExistingRegistries(resources, layers.getLayer(RegistryLayer.STATIC));
      staticTags.forEach(Registry.PendingTags::apply);
      var worldContext = TagLoader.buildUpdatedLookups(layers.getAccessForLoading(RegistryLayer.WORLD), staticTags);
      var world = RegistryDataLoader.load(resources, worldContext, RegistryDataLoader.WORLD_REGISTRIES, direct).join();
      List<HolderLookup.RegistryLookup<?>> dimContext = new ArrayList<>(worldContext);
      world.listRegistries().forEach(dimContext::add);
      var dims = RegistryDataLoader.load(resources, dimContext, RegistryDataLoader.DIMENSION_REGISTRIES, direct).join();
      RegistryAccess.Frozen full = layers.replaceFrom(RegistryLayer.WORLD, world, dims).compositeAccess();
      List<DataComponentInitializers.PendingComponents<?>> comps = BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(full);
      comps.forEach(DataComponentInitializers.PendingComponents::apply);
      return full;
   }
}
