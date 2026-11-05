import java.nio.file.*;
import java.util.List;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager;
import net.minecraft.world.level.storage.LevelStorageSource;

/** The real {@code StructureTemplateManager} over the vanilla data pack (structure templates), for structure oracles. */
public class TemplateSupport {
    public static StructureTemplateManager manager() throws Exception {
        MultiPackResourceManager resources = new MultiPackResourceManager(PackType.SERVER_DATA, List.of(ServerPacksSource.createVanillaPackSource().fullResources()));
        Path tmp = Files.createTempDirectory("mc-oracle-level");
        var storage = LevelStorageSource.createDefault(tmp).createAccess("oracle");
        return new StructureTemplateManager(resources, storage, DataFixers.getDataFixer(), BuiltInRegistries.BLOCK);
    }
}
