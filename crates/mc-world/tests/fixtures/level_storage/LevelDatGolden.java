// Golden generator for crates/mc-world/tests/golden/level_storage_golden.rs
// Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/level_storage/level_dat_golden.txt crates/mc-world/tests/fixtures/level_storage/LevelDatGolden.java crates/mc-world/tests/fixtures/level_storage/level_dat_golden.txt
// Output: crates/mc-world/tests/fixtures/level_storage/level_dat_golden.txt
// For every input `Data` compound the REAL vanilla classes (LevelStorageSource.readDataConfig, LevelSettings.parse,
// LevelVersion.parse, PrimaryLevelData.parse/createTag, FeatureFlags.CODEC) produce the expected field values and the
// re-serialized compound (LastPlayed forced to 0).
import com.mojang.serialization.Dynamic;
import com.mojang.serialization.Lifecycle;
import java.io.PrintStream;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.storage.*;

public class LevelDatGolden {
  static PrintStream out;

  static String f(float v) { return Integer.toHexString(Float.floatToIntBits(v)); }

  static void run(String name, CompoundTag data) throws Exception {
    Dynamic<Tag> dyn = new Dynamic<>(NbtOps.INSTANCE, data);
    WorldDataConfiguration cfg = LevelStorageSource.readDataConfig(dyn);
    LevelSettings settings = LevelSettings.parse(dyn, cfg);
    LevelVersion lv = LevelVersion.parse(dyn);
    PrimaryLevelData pld = PrimaryLevelData.parse(dyn, settings, PrimaryLevelData.SpecialWorldProperty.NONE, Lifecycle.stable());
    out.println("@@ " + name);
    out.println("IN " + data);
    out.println("levelName=" + settings.levelName());
    out.println("gameType=" + settings.gameType().getId());
    out.println("difficulty=" + settings.difficultySettings().difficulty().getSerializedName() + "," + settings.difficultySettings().hardcore() + "," + settings.difficultySettings().locked());
    out.println("allowCommands=" + settings.allowCommands());
    out.println("dataPacks=" + cfg.dataPacks().getEnabled() + "|" + cfg.dataPacks().getDisabled());
    List<String> feats = new ArrayList<>();
    FeatureFlags.REGISTRY.toNames(cfg.enabledFeatures()).forEach(i -> feats.add(i.toString()));
    Collections.sort(feats);
    out.println("features=" + feats + " experimental=" + FeatureFlags.isExperimental(cfg.enabledFeatures()));
    var sp = pld.getRespawnData();
    out.println("spawn=" + sp.dimension().identifier() + "," + sp.pos().getX() + "," + sp.pos().getY() + "," + sp.pos().getZ() + "," + f(sp.yaw()) + "," + f(sp.pitch()));
    out.println("time=" + pld.getGameTime());
    out.println("singleplayer=" + pld.getSinglePlayerUUID());
    out.println("version=" + pld.getVersion() + " initialized=" + pld.isInitialized() + " wasModded=" + pld.wasModded());
    out.println("brands=" + new ArrayList<>(pld.getKnownServerBrands()));
    List<String> removed = new ArrayList<>(pld.getRemovedFeatureFlags());
    Collections.sort(removed);
    out.println("removed=" + removed);
    out.println("levelVersion=" + lv.levelDataVersion() + "," + lv.lastPlayed() + "," + lv.minecraftVersionName() + "," + lv.minecraftVersion().version() + "," + lv.minecraftVersion().series() + "," + lv.snapshot());
    CompoundTag tag = pld.createTag(null);
    tag.putLong("LastPlayed", 0L);
    out.println("OUT " + tag);
  }

  public static void main(String[] args) throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion();
    net.minecraft.server.Bootstrap.bootStrap();
    out = new PrintStream(args[0]);
    String uuid = "[I;1,-2,3,-4]";
    run("empty", new CompoundTag());
    run("vanilla_like", TagParser.parseCompoundFully("{DataPacks:{Disabled:[\"minecart_improvements\"],Enabled:[\"vanilla\"]},DataVersion:5023,GameType:1,LastPlayed:1793788769420L,LevelName:\"world\",ServerBrands:[\"vanilla\"],Time:1199L,Version:{Id:5023,Name:\"26.3\",Series:\"main\",Snapshot:0b},WasModded:0b,allowCommands:0b,difficulty_settings:{difficulty:\"normal\",hardcore:0b,locked:0b},initialized:1b,spawn:{dimension:\"minecraft:overworld\",pitch:0.0f,pos:[I;96,145,-32],yaw:0.0f},version:19133,version_history:[5023]}"));
    run("rich", TagParser.parseCompoundFully("{DataPacks:{Disabled:[\"a\",\"b\"],Enabled:[\"vanilla\",\"file/x\"]},enabled_features:[\"minecraft:vanilla\",\"minecraft:trade_rebalance\",\"minecraft:redstone_experiments\",\"minecraft:minecart_improvements\"],GameType:3,LastPlayed:5L,LevelName:\"  Rich \\u2603 World \",ServerBrands:[\"vanilla\",\"fabric\",\"vanilla\",5,\"x\"],Time:-77L,Version:{Id:4000,Name:\"1.21.9\",Series:\"main\",Snapshot:1b},WasModded:1b,allowCommands:1b,difficulty_settings:{difficulty:\"hard\",hardcore:1b,locked:1b},initialized:0b,singleplayer_uuid:" + uuid + ",spawn:{dimension:\"minecraft:the_nether\",pitch:-90.0f,pos:[I;-1,2,3],yaw:-180.0f},version:19133,version_history:[3000,4000],removed_features:[\"minecraft:a\",\"b\",\"minecraft:zz\",\"q\",\"abc\",\"minecraft:redstone_experiments\",\"u\",\"minecraft:v\",\"w\",\"x\",\"y\",\"z\",\"minecraft:aa\",\"minecraft:bb\",\"minecraft:cc\"],Player:{Health:20.0f},WorldGenSettings:{seed:1L},custom_unknown:[1,2,3]}"));
    run("rich_features_pair", TagParser.parseCompoundFully("{enabled_features:[\"minecraft:minecart_improvements\",\"minecraft:vanilla\"],DataPacks:{Enabled:[\"vanilla\"],Disabled:[]}}"));
    run("bad_types", TagParser.parseCompoundFully("{GameType:7,LevelName:5,allowCommands:2.5d,difficulty_settings:{difficulty:\"insane\",hardcore:1b,locked:0b},spawn:{dimension:\"minecraft:overworld\",pitch:0.0f,pos:[I;1,2,3],yaw:200.0f},singleplayer_uuid:[I;1,2,3],DataPacks:{Enabled:[\"x\"]},enabled_features:[\"minecraft:nope\"],version_history:[5,\"x\",2.7d,9L],Version:\"str\",ServerBrands:\"notalist\",Time:\"x\",version:\"v\",initialized:\"x\",WasModded:3}"));
    run("bad_types2", TagParser.parseCompoundFully("{GameType:-1,allowCommands:0.0d,difficulty_settings:{difficulty:\"easy\",hardcore:2b,locked:0.5d},spawn:{dimension:\"Bad Dim\",pitch:0.0f,pos:[I;1,2,3],yaw:1.0f},singleplayer_uuid:[1,2,3,4],DataPacks:{Enabled:[\"x\",1],Disabled:[]},enabled_features:[\"minecraft:trade_rebalance\"],version_history:[I;7,8],Version:{Name:3,Id:\"x\"},ServerBrands:[I;1],removed_features:[],version:19132}"));
    run("spawn_list_pos", TagParser.parseCompoundFully("{GameType:2,spawn:{dimension:\"minecraft:overworld\",pitch:45.5f,pos:[1,2.9d,3L],yaw:-12.25f},Version:{Id:5023},DataPacks:{Enabled:[],Disabled:[]},removed_features:[\"only\"]}"));

    // FeatureFlags.CODEC encode order for every subset
    for (int mask = 0; mask < 16; mask++) {
      List<net.minecraft.resources.Identifier> ids = new ArrayList<>();
      String[] n = {"vanilla", "trade_rebalance", "redstone_experiments", "minecart_improvements"};
      for (int i = 0; i < 4; i++) if ((mask >> i & 1) != 0) ids.add(net.minecraft.resources.Identifier.withDefaultNamespace(n[i]));
      FeatureFlagSet set = FeatureFlags.REGISTRY.fromNames(ids);
      out.println("@@ flags_" + mask);
      out.println("FLAGS " + FeatureFlags.CODEC.encodeStart(NbtOps.INSTANCE, set).getOrThrow());
    }
    out.close();
  }
}
