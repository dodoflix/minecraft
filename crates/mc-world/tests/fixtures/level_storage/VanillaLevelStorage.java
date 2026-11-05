// Oracle for crates/mc-world/tests/oracle/level_storage_vanilla.rs (real vanilla LevelStorageSource / PlayerDataStorage /
// DirectoryLock):
//   summary <savesDir> <levelId> <playerUuid>   print level data, summary, resource paths, player data
//   lock <dir>                                  DirectoryLock.isLocked + try DirectoryLock.create
//   hold <dir> <millis>                         DirectoryLock.create, print HELD, sleep
import java.nio.file.*;
import java.util.UUID;
import com.mojang.serialization.Dynamic;
import net.minecraft.nbt.*;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.players.NameAndId;
import net.minecraft.util.DirectoryLock;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.*;

public class VanillaLevelStorage {
  static ResourceKey<Level> dim(String id) { return ResourceKey.create(Registries.DIMENSION, Identifier.parse(id)); }

  public static void main(String[] args) throws Exception {
    switch (args[0]) {
      case "lock" -> {
        Path dir = Path.of(args[1]);
        System.out.println("isLocked=" + DirectoryLock.isLocked(dir));
        try { DirectoryLock l = DirectoryLock.create(dir); System.out.println("CREATED"); l.close(); }
        catch (Exception e) { System.out.println("FAILED " + e.getClass().getSimpleName() + ": " + e.getMessage()); }
      }
      case "hold" -> {
        DirectoryLock l = DirectoryLock.create(Path.of(args[1]));
        System.out.println("HELD");
        System.out.flush();
        Thread.sleep(Long.parseLong(args[2]));
        l.close();
      }
      case "summary" -> {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        Path saves = Path.of(args[1]);
        LevelStorageSource source = LevelStorageSource.createDefault(saves);
        try (LevelStorageSource.LevelStorageAccess access = source.validateAndCreateAccess(args[2])) {
          System.out.println("hasWorldData=" + access.hasWorldData());
          Dynamic<?> data = access.getUnfixedDataTagWithFallback();
          System.out.println("DATA " + data.getValue());
          LevelSummary s = access.fixAndGetSummaryFromTag(data);
          System.out.println("summary id=" + s.getLevelId() + " name=" + s.getLevelName() + " mode=" + s.getGameMode() + " hardcore=" + s.isHardcore()
              + " commands=" + s.hasCommands() + " experimental=" + s.isExperimental() + " lastPlayed=" + s.getLastPlayed()
              + " manual=" + s.requiresManualConversion() + " fileFix=" + s.requiresFileFixing() + " compatible=" + s.isCompatible()
              + " backup=" + s.backupStatus() + " version=" + s.levelVersion().minecraftVersionName() + " disabled=" + s.isDisabled());
          Path root = saves.resolve(args[2]);
          for (LevelResource r : new LevelResource[] {LevelResource.PLAYER_DATA_DIR, LevelResource.PLAYER_STATS_DIR, LevelResource.PLAYER_ADVANCEMENTS_DIR, LevelResource.DATA, LevelResource.LEVEL_DATA_FILE, LevelResource.OLD_LEVEL_DATA_FILE, LevelResource.ICON_FILE, LevelResource.LOCK_FILE, LevelResource.DATAPACK_DIR, LevelResource.GENERATED_DIR})
            System.out.println("path " + r.id() + " " + root.relativize(access.getLevelPath(r)));
          for (String d : new String[] {"minecraft:overworld", "minecraft:the_nether", "minecraft:the_end", "mymod:foo/bar"})
            System.out.println("dim " + d + " " + root.relativize(access.getDimensionPath(dim(d))));
          PlayerDataStorage players = access.createPlayerStorage();
          System.out.println("PLAYER " + players.load(new NameAndId(UUID.fromString(args[3]), "tester")).map(Object::toString).orElse("none"));
        }
      }
    }
  }
}
