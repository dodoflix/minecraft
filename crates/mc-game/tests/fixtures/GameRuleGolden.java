import com.mojang.serialization.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.resources.*;
import net.minecraft.world.level.gamerules.*;
import net.minecraft.world.flag.*;
import java.util.*;

/** Golden values for GameRules/GameRuleMap.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/gamerule.golden crates/mc-game/tests/fixtures/GameRuleGolden.java crates/mc-game/tests/fixtures/gamerule.golden
 * Output: crates/mc-game/tests/fixtures/gamerule.golden */
public class GameRuleGolden {
  static String sorted(GameRuleMap m) {
    TreeMap<String, String> t = new TreeMap<>();
    for (GameRule<?> g : m.keySet()) t.put(g.id(), String.valueOf(m.get(g)));
    return t.toString();
  }
  static String show(DataResult<GameRuleMap> r) {
    String res = r.result().map(GameRuleGolden::sorted).orElse("none");
    String part = r.resultOrPartial(e -> {}).map(GameRuleGolden::sorted).orElse("none");
    String err = r.error().map(e -> e.message()).orElse("");
    return "result=" + res + "\tpartial=" + part + "\terr=" + err.replace("\n", "\\n");
  }
  @SuppressWarnings("unchecked")
  static <T> String cmd(GameRule<T> g) { return g.serialize(g.defaultValue()) + "/" + g.getCommandResult(g.defaultValue()); }
  @SuppressWarnings("unchecked")
  static <T> String deser(GameRule<T> g, String s) {
    DataResult<T> r = g.deserialize(s);
    return "result=" + r.result().map(String::valueOf).orElse("none") + "\tpartial=" + r.resultOrPartial(e -> {}).map(String::valueOf).orElse("none") + "\terr=" + r.error().map(e -> e.message()).orElse("");
  }
  static String argDesc(GameRule<?> g) {
    Object a = g.argument();
    if (a instanceof com.mojang.brigadier.arguments.IntegerArgumentType i) return "integer(" + i.getMinimum() + "," + i.getMaximum() + ")";
    return a.getClass().getSimpleName();
  }
  static java.io.PrintStream out;
  public static void main(String[] args) throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion();
    net.minecraft.server.Bootstrap.bootStrap();
    out = new java.io.PrintStream(new java.io.FileOutputStream(args[0]), false, "UTF-8");
    BuiltInRegistries.GAME_RULE.listElements().forEach(h -> {
      GameRule<?> g = h.value();
      out.println("RULE\t" + g.id() + "\t" + g.getIdentifier() + "\t" + g.gameRuleType() + "\t" + g.defaultValue() + "\t" + g.category().id() + "\t" + g.getDescriptionId()
        + "\t" + argDesc(g) + "\t" + FeatureFlags.REGISTRY.toNames(g.requiredFeatures()) + "\t" + cmd(g) + "\t" + g.valueClass().getSimpleName());
    });
    String[] inputs = {"true", "false", "1", "-1", "0", "2147483647", "2147483648", "abc", "1 ", "true ", " 1", "1.0", "-0", "+1", "100", "8", "9", "1000", "1001", "tr", "TRUE", "", "00012", "1x"};
    BuiltInRegistries.GAME_RULE.listElements().forEach(h -> {
      GameRule<?> g = h.value();
      for (String s : inputs) out.println("DESER\t" + g.id() + "\t[" + s + "]\t" + deser(g, s));
    });
    String[] cases = {
      "{}",
      "{\"minecraft:pvp\":0b,\"minecraft:random_tick_speed\":5}",
      "{\"minecraft:pvp\":0b,\"minecraft:nonexistent\":5,\"minecraft:random_tick_speed\":5}",
      "{\"pvp\":0b}",
      "{\"minecraft:pvp\":\"x\",\"minecraft:random_tick_speed\":5}",
      "{\"minecraft:pvp\":2,\"minecraft:random_tick_speed\":5.7d}",
      "{\"minecraft:random_tick_speed\":-1}",
      "{\"minecraft:random_tick_speed\":-1,\"minecraft:pvp\":1b}",
      "{\"minecraft:max_snow_accumulation_height\":9}",
      "{\"minecraft:max_minecart_speed\":1001}",
      "{\"minecraft:fire_spread_radius_around_player\":-1}",
      "{\"minecraft:fire_spread_radius_around_player\":-2}",
      "{\"minecraft:pvp\":0.5d}",
      "{\"minecraft:pvp\":0.0d}",
      "{\"minecraft:pvp\":[1b]}",
      "5", "[]", "{\"minecraft:random_tick_speed\":3000000000L}", "{\"minecraft:random_tick_speed\":\"4\"}"
    };
    for (String c : cases) {
      Tag t = c.startsWith("{") ? TagParser.parseCompoundFully(c) : (c.equals("5") ? IntTag.valueOf(5) : new ListTag());
      out.println("CODEC\t" + c + "\t" + show(GameRuleMap.CODEC.parse(NbtOps.INSTANCE, t)));
    }
    // encode: defaults, and every rule set to a non-default value
    GameRuleMap def = GameRuleMap.of(BuiltInRegistries.GAME_RULE.stream());
    out.println("ENCODE_DEFAULT\t" + GameRuleMap.CODEC.encodeStart(NbtOps.INSTANCE, def).getOrThrow());
    GameRuleMap alt = GameRuleMap.of();
    BuiltInRegistries.GAME_RULE.stream().forEach(g -> {
      if (g.gameRuleType() == GameRuleType.BOOL) ((GameRuleMap) alt).set((GameRule<Boolean>) g, !((Boolean) g.defaultValue()));
      else ((GameRuleMap) alt).set((GameRule<Integer>) g, (Integer) g.defaultValue() == 1 ? 2 : 1);
    });
    out.println("ENCODE_ALT\t" + GameRuleMap.CODEC.encodeStart(NbtOps.INSTANCE, alt).getOrThrow());
    // GameRules with feature flags
    GameRules none = new GameRules(FeatureFlagSet.of());
    out.println("RULES_VANILLA\t" + none.availableRules().map(GameRule::id).sorted().toList().size());
    GameRules exp = new GameRules(FeatureFlags.REGISTRY.allFlags());
    out.println("RULES_ALL\t" + exp.availableRules().map(GameRule::id).sorted().toList().size());
    out.println("RULES_MINECART_ONLY\t" + new GameRules(FeatureFlagSet.of(FeatureFlags.MINECART_IMPROVEMENTS)).availableRules().count());
    out.println("CATEGORIES\t" + GameRuleCategory.PLAYER.label().getString());
    out.close();
  }
}
