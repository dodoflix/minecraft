// Golden generator for crates/mc-world/tests/golden/level_summary_golden.rs
// Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/level_storage/level_summary_golden.txt crates/mc-world/tests/fixtures/level_storage/LevelSummaryGolden.java crates/mc-world/tests/fixtures/level_storage/level_summary_golden.txt
// Output: crates/mc-world/tests/fixtures/level_storage/level_summary_golden.txt
// For every input `Data` compound the REAL vanilla LevelStorageAccess produces the summary of `fixAndGetSummaryFromTag`
// (or the exception it throws), the outcome of `collectIssues`, and the level data `FileFixerUpper.fix` hands to the
// world load (LastPlayed forced to 0).
import com.mojang.serialization.Dynamic;
import java.io.PrintStream;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.*;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.util.worldupdate.UpgradeProgress;
import net.minecraft.world.level.storage.*;

public class LevelSummaryGolden {
  static PrintStream out;

  static void run(String name, CompoundTag data) throws Exception {
    Path saves = Files.createTempDirectory("saves");
    Path world = saves.resolve("w");
    Files.createDirectories(world);
    CompoundTag root = new CompoundTag();
    root.put("Data", data);
    NbtIo.writeCompressed(root, world.resolve("level.dat"));
    LevelStorageSource source = LevelStorageSource.createDefault(saves);
    out.println("@@ " + name);
    out.println("IN " + data);
    try (LevelStorageSource.LevelStorageAccess access = source.validateAndCreateAccess("w")) {
      Dynamic<?> unfixed = access.getUnfixedDataTagWithFallback();
      try {
        LevelSummary s = access.fixAndGetSummaryFromTag(unfixed);
        out.println("SUMMARY name=" + s.getLevelName() + " mode=" + s.getGameMode() + " hardcore=" + s.isHardcore()
            + " commands=" + s.hasCommands() + " experimental=" + s.isExperimental() + " lastPlayed=" + s.getLastPlayed()
            + " manual=" + s.requiresManualConversion() + " fileFix=" + s.requiresFileFixing() + " compatible=" + s.isCompatible()
            + " backup=" + s.backupStatus().getTranslationKey() + " shouldBackup=" + s.shouldBackup() + " downgrade=" + s.isDowngrade()
            + " disabled=" + s.isDisabled() + " upload=" + s.canUpload() + " edit=" + s.canEdit() + " recreate=" + s.canRecreate()
            + " version=" + s.levelVersion().minecraftVersionName() + "," + s.levelVersion().minecraftVersion().version() + "," + s.levelVersion().minecraftVersion().series() + "," + s.levelVersion().snapshot());
      } catch (Throwable t) {
        out.println("SUMMARY ERROR " + t.getClass().getName() + ": " + t.getMessage());
      }
      try {
        access.collectIssues(false);
        out.println("ISSUES ok");
      } catch (Throwable t) {
        out.println("ISSUES ERROR " + t.getClass().getName() + ": " + t.getMessage());
      }
      int version = NbtUtils.getDataVersion(unfixed);
      if (DataFixers.getFileFixer().requiresFileFixing(version)) {
        out.println("FIXED file fixing required");
      } else {
        Dynamic<?> fixed = DataFixers.getFileFixer().fix(access, unfixed, new UpgradeProgress.Noop());
        CompoundTag tag = (CompoundTag) fixed.convert(NbtOps.INSTANCE).getValue();
        tag.putLong("LastPlayed", 0L);
        out.println("FIXED " + tag);
      }
    }
  }

  static String vanillaLike(int version) {
    return "{DataPacks:{Disabled:[\"minecart_improvements\"],Enabled:[\"vanilla\"]},DataVersion:" + version
        + ",GameType:1,LastPlayed:1793788769420L,LevelName:\"world\",ServerBrands:[\"vanilla\"],Time:1199L,Version:{Id:" + version
        + ",Name:\"v" + version + "\",Series:\"main\",Snapshot:0b},WasModded:0b,allowCommands:0b,difficulty_settings:{difficulty:\"normal\",hardcore:0b,locked:0b},initialized:1b,spawn:{dimension:\"minecraft:overworld\",pitch:0.0f,pos:[I;96,145,-32],yaw:0.0f},version:19133,version_history:[" + version + "]}";
  }

  static String rich(int version) {
    return "{DataPacks:{Disabled:[\"a\",\"b\"],Enabled:[\"vanilla\",\"file/x\"]},enabled_features:[\"minecraft:vanilla\",\"minecraft:trade_rebalance\",\"minecraft:nope\",5],DataVersion:" + version
        + ",GameType:3,LastPlayed:5L,LevelName:\"  Rich \\u2603 World \",ServerBrands:[\"vanilla\",\"fabric\"],Time:-77L,Version:{Id:" + version
        + ",Name:\"1.2.3\",Series:\"main\",Snapshot:1b},WasModded:1b,allowCommands:1b,difficulty_settings:{difficulty:\"hard\",hardcore:1b,locked:1b},initialized:0b,spawn:{dimension:\"minecraft:the_nether\",pitch:-90.0f,pos:[I;-1,2,3],yaw:-180.0f},version:19133,version_history:[3000," + version + "],singleplayer_uuid:[I;1,-2,3,-4],Player:{Health:5.0f},WorldGenSettings:{seed:1L}}";
  }

  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();
    net.minecraft.server.Bootstrap.bootStrap();
    out = new PrintStream(args[0]);
    for (int v = 4899; v <= 5025; v++) run("version_" + v, TagParser.parseCompoundFully(vanillaLike(v)));
    for (int v : new int[] {4899, 4950, 5016, 5022, 5023, 5024}) run("rich_" + v, TagParser.parseCompoundFully(rich(v)));
    String base = vanillaLike(5000);
    run("mcregion", TagParser.parseCompoundFully(base.replace("version:19133", "version:19132")));
    run("unknown_version_tag", TagParser.parseCompoundFully(base.replace("version:19133", "version:12345")));
    run("absent_version_tag", TagParser.parseCompoundFully(base.replace("version:19133,", "")));
    run("other_series", TagParser.parseCompoundFully(base.replace("Series:\"main\"", "Series:\"other\"")));
    run("no_version_compound", TagParser.parseCompoundFully(base.replaceAll("Version:\\{[^}]*\\},", "")));
    run("future_data_version", TagParser.parseCompoundFully(vanillaLike(7000)));
    // data older than 4771 goes through the fixes of the level data types (the difficulty, the feature toggles)
    String older = "version:19133,LevelName:\"d\",GameType:0,Version:{Id:4000,Name:\"x\",Series:\"main\",Snapshot:0b}";
    run("difficulty_4770", TagParser.parseCompoundFully("{DataVersion:4770,Difficulty:3,hardcore:1b,DifficultyLocked:1b," + older + "}"));
    run("difficulty_default_4700", TagParser.parseCompoundFully("{DataVersion:4700," + older + "}"));
    run("locator_bar_4423", TagParser.parseCompoundFully("{DataVersion:4423,enabled_features:[\"minecraft:vanilla\",\"minecraft:locator_bar\",5],removed_features:[\"minecraft:old\"]," + older + "}"));
    run("toggles_4000", TagParser.parseCompoundFully("{DataVersion:4000,Difficulty:1,enabled_features:[\"minecraft:vanilla\",\"minecraft:bundle\",\"minecraft:winter_drop\",\"minecraft:trade_rebalance\"]," + older + "}"));
    run("bad_types", TagParser.parseCompoundFully("{DataVersion:5000,version:19133,GameType:7,LevelName:5,allowCommands:2.5d,difficulty_settings:{difficulty:\"insane\",hardcore:1b,locked:0b},Version:\"str\",enabled_features:\"x\"}"));
    out.close();
  }
}
