import com.mojang.serialization.*;
import net.minecraft.core.HolderLookup;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.nbt.*;
import net.minecraft.resources.*;
import net.minecraft.server.bossevents.CustomBossEvents;
import net.minecraft.world.RandomSequences;
import net.minecraft.world.Stopwatches;
import net.minecraft.world.clock.ServerClockManager;
import net.minecraft.world.entity.raid.Raids;
import net.minecraft.world.level.TicketStorage;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.dimension.end.EnderDragonFight;
import net.minecraft.world.level.gamerules.GameRuleMap;
import net.minecraft.world.level.levelgen.WorldGenSettings;
import net.minecraft.world.level.saveddata.*;
import net.minecraft.world.level.saveddata.maps.MapId;
import net.minecraft.world.level.saveddata.maps.MapIndex;
import net.minecraft.world.level.saveddata.maps.MapItemSavedData;
import net.minecraft.world.level.timers.TimerQueue;
import net.minecraft.world.scores.ScoreboardSaveData;
import java.util.*;

/** What `SavedDataStorage.readSavedData` reports for damaged `data` tags: for every saved-data type a valid `data` tag with
 * every compound key removed, replaced by a string, a compound, a list, a negative int and a float, an unknown key added, all
 * keys damaged at once (aggregation order), and `data` itself of another tag type.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/saved_data_errors.golden.gz crates/mc-game/tests/fixtures/SavedDataErrorsGolden.java crates/mc-game/tests/fixtures/saved_data_errors.golden.gz
 * Run it from the repository root (the fixture world is read by relative path).
 * Output: crates/mc-game/tests/fixtures/saved_data_errors.golden.gz, lines `<type>\t<input snbt>\t<result>` with the result `ok\t<encoded>`, `partial\t<message>\t<encoded>`, `err\t<message>`
 * or `throw\t<exception>`; newlines are written as `¶`. */
public class SavedDataErrorsGolden {
  static java.io.PrintStream out;
  static RegistryOps<Tag> ops;
  static final String FIXTURE_WORLD = "crates/mc-world/tests/fixtures/level_storage/world/";
  static final String[] VALUES = {"\"x\"", "{}", "[]", "-1", "2.5d"};

  static String line(String s) { return s.replace("\n", "¶").replace("\t", "    "); }

  /** Players/heroes are `Set.copyOf` sets: their list order is random per JVM run, so sorted. */
  static Tag canon(Tag t) {
    if (t instanceof CompoundTag c) {
      CompoundTag r = new CompoundTag();
      for (String k : c.keySet()) {
        Tag v = canon(c.get(k));
        // a map's 16384 colors: the first eight identify the value
        if (k.equals("colors") && v instanceof ByteArrayTag b) v = new ByteArrayTag(Arrays.copyOf(b.getAsByteArray(), 8));
        if ((k.equals("Players") || k.equals("heroes_of_the_village")) && v instanceof ListTag l) {
          List<Tag> items = new ArrayList<>(l);
          items.sort(Comparator.comparing(Tag::toString));
          ListTag sorted = new ListTag();
          sorted.addAll(items);
          v = sorted;
        }
        r.put(k, v);
      }
      return r;
    }
    if (t instanceof ListTag l) {
      ListTag r = new ListTag();
      for (Tag e : l) r.add(canon(e));
      return r;
    }
    return t;
  }

  static Tag at(Tag root, List<Object> path) {
    Tag t = root;
    for (Object p : path) t = p instanceof String s ? ((CompoundTag) t).get(s) : ((ListTag) t).get((Integer) p);
    return t;
  }

  static Tag value(String snbt) throws Exception { return TagParser.create(NbtOps.INSTANCE).parseFully(snbt); }

  /** The mutants of `base` at the compound/list node `path`, then the nodes below it. */
  static void mutate(Tag base, List<Object> path, List<Tag> out, int depth, int maxDepth) throws Exception {
    Tag node = at(base, path);
    if (node instanceof CompoundTag c) {
      List<String> keys = new ArrayList<>(c.keySet());
      Collections.sort(keys);
      for (String k : keys) {
        Tag copy = base.copy();
        ((CompoundTag) at(copy, path)).remove(k);
        out.add(copy);
        for (String v : VALUES) {
          copy = base.copy();
          ((CompoundTag) at(copy, path)).put(k, value(v));
          out.add(copy);
        }
      }
      for (String added : new String[] {"zzz", "Bad Key"}) {
        Tag copy = base.copy();
        ((CompoundTag) at(copy, path)).put(added, IntTag.valueOf(1));
        out.add(copy);
      }
      if (keys.size() > 1) {
        Tag copy = base.copy();
        for (String k : keys) ((CompoundTag) at(copy, path)).remove(k);
        out.add(copy);
        for (String v : VALUES) {
          copy = base.copy();
          for (String k : keys) ((CompoundTag) at(copy, path)).put(k, value(v));
          out.add(copy);
        }
      }
      if (depth < maxDepth) for (String k : keys) {
        List<Object> sub = new ArrayList<>(path);
        sub.add(k);
        if (c.get(k) instanceof CompoundTag || c.get(k) instanceof ListTag) mutate(base, sub, out, depth + 1, maxDepth);
      }
    } else if (node instanceof ListTag l) {
      for (int i = 0; i < Math.min(2, l.size()); i++) {
        for (String v : VALUES) {
          Tag copy = base.copy();
          ((ListTag) at(copy, path)).set(i, value(v));
          out.add(copy);
        }
        List<Object> sub = new ArrayList<>(path);
        sub.add(i);
        if (l.get(i) instanceof CompoundTag || l.get(i) instanceof ListTag) mutate(base, sub, out, depth + 1, maxDepth);
      }
    }
  }

  static <T extends SavedData> String eval(SavedDataType<T> type, Tag in, boolean encode) {
    try {
      DataResult<T> r = type.codec().parse(ops, in);
      String[] msg = {null};
      Optional<T> v = r.resultOrPartial(e -> msg[0] = e);
      if (v.isEmpty()) return "err\t" + line(msg[0]);
      String enc = encode ? canon(type.codec().encodeStart(ops, v.get()).getOrThrow()).toString() : "-";
      return r.result().isPresent() ? "ok\t" + line(enc) : "partial\t" + line(msg[0]) + "\t" + line(enc);
    } catch (Throwable e) {
      return "throw\t" + line(e.toString());
    }
  }

  static <T extends SavedData> void run(String name, SavedDataType<T> type, boolean wgs, boolean encode, Tag... bases) throws Exception {
    Set<String> seen = new LinkedHashSet<>();
    List<Tag> inputs = new ArrayList<>();
    for (Tag base : bases) {
      inputs.add(base);
      mutate(base, List.of(), inputs, 0, wgs ? 0 : 5);
      for (String v : new String[] {"\"x\"", "7", "[]", "1.5d", "[I;1]"}) inputs.add(value(v));
    }
    for (Tag in : inputs) {
      String snbt = in.toString();
      if (!seen.add(snbt)) continue;
      // `readExistingSavedData` (world gen settings) reads `getCompoundOrEmpty("data")`
      Tag parsed = wgs && !(in instanceof CompoundTag) ? new CompoundTag() : in;
      out.println(name + "\t" + line(snbt) + "\t" + eval(type, parsed, encode));
    }
  }

  static Tag file(String path) throws Exception {
    return NbtIo.readCompressed(java.nio.file.Path.of(FIXTURE_WORLD + path), NbtAccounter.unlimitedHeap()).get("data");
  }

  static Tag snbt(String s) throws Exception { return value(s); }

  @SuppressWarnings("unchecked")
  public static void main(String[] a) throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion();
    net.minecraft.server.Bootstrap.bootStrap();
    out = new java.io.PrintStream(new java.io.FileOutputStream(a[0]), false, "UTF-8");
    HolderLookup.Provider registries = VanillaRegistries.createWorldLookup();
    ops = RegistryOps.create(NbtOps.INSTANCE, registries);
    var typeFactory = Class.forName("net.minecraft.world.level.storage.CommandStorage$Container").getDeclaredMethod("type", String.class);
    typeFactory.setAccessible(true);
    String raid = "started:1b,active:1b,ticks_active:100L,raid_omen_level:2,groups_spawned:1,cooldown_ticks:300,post_raid_ticks:0,total_health:12.5f,group_count:5,status:\"ongoing\",center:[I;10,64,-20]";
    run("custom_boss_events", CustomBossEvents.TYPE, false, true, file("data/minecraft/custom_boss_events.dat"),
        snbt("{\"test:b\":{Name:{text:\"Red\",color:\"red\"},Visible:1b,Value:5,Max:20,Color:\"blue\",Overlay:\"notched_12\",DarkenScreen:1b,PlayBossMusic:1b,CreateWorldFog:1b,Players:[[I;1,2,3,4]]}}"));
    run("random_sequences", RandomSequences.TYPE, false, true, file("data/minecraft/random_sequences.dat"),
        snbt("{salt:3,include_world_seed:0b,include_sequence_id:0b,sequences:{\"a:b\":{source:[L;1L,2L]}}}"));
    run("stopwatches", Stopwatches.TYPE, false, false, snbt("{stopwatches:{\"minecraft:a\":5L,\"b:c\":0L}}"));
    run("world_clocks", ServerClockManager.TYPE, false, true, file("data/minecraft/world_clocks.dat"),
        snbt("{\"minecraft:overworld\":{total_ticks:5L,partial_tick:0.5f,rate:2.0f,paused:1b}}"));
    run("raids", Raids.TYPE, false, true,
        snbt("{raids:[{id:1," + raid + ",heroes_of_the_village:[[I;1,2,3,4]]}],next_id:2,tick:77}"));
    run("chunk_tickets", TicketStorage.TYPE, false, true,
        snbt("{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:forced\",level:31,ticks_left:5L},{chunk_pos:[I;0,0],type:\"minecraft:player_loading\",level:30}]}"));
    run("world_border", WorldBorder.TYPE, false, true, file("dimensions/minecraft/overworld/data/minecraft/world_border.dat"));
    run("ender_dragon_fight", EnderDragonFight.TYPE, false, true,
        snbt("{needs_state_scanning:0b,dragon_killed:1b,previously_killed:1b,respawn_stage:\"start\",respawn_time:3,dragon_uuid:[I;1,2,3,4],exit_portal_location:[I;0,64,0],gateways:[1,2],respawn_crystals:[[I;1,2,3,4]]}"));
    run("game_rules", GameRuleMap.TYPE, false, true,
        snbt("{\"minecraft:max_entity_cramming\":24,\"minecraft:random_tick_speed\":3,\"minecraft:keep_inventory\":1b}"));
    run("world_gen_settings", WorldGenSettings.TYPE, true, true,
        snbt("{seed:5L,generate_structures:1b,bonus_chest:0b,legacy_custom_options:\"x\",dimensions:{\"minecraft:overworld\":{generator:{biome_source:{preset:\"minecraft:overworld\",type:\"minecraft:multi_noise\"},settings:\"minecraft:overworld\",type:\"minecraft:noise\"},type:\"minecraft:overworld\"}}}"),
        snbt("{seed:5L,dimensions:{\"minecraft:overworld\":{generator:{biome_source:{preset:\"minecraft:overworld\",type:\"minecraft:multi_noise\"},settings:\"minecraft:overworld\",type:\"minecraft:noise\"},type:\"minecraft:overworld\"},\"Bad Key\":{},\"x:y\":5}}"));
    run("wandering_trader", WanderingTraderData.TYPE, false, true, snbt("{spawn_delay:5,spawn_chance:7}"));
    run("weather", WeatherData.TYPE, false, true, file("data/minecraft/weather.dat"));
    run("map_index", MapIndex.TYPE, false, true, snbt("{map:5}"));
    run("map_data", MapItemSavedData.type(new MapId(1)), false, true,
        snbt("{dimension:\"minecraft:overworld\",xCenter:1,zCenter:2,scale:1b,colors:[B;0b,1b],trackingPosition:0b,unlimitedTracking:1b,locked:1b,banners:[{pos:[I;1,2,3],color:\"red\",name:\"n\"}],frames:[{pos:[I;1,2,3],rotation:90,entity_id:5}]}"));
    run("command_storage", (SavedDataType<SavedData>) typeFactory.invoke(null, "minecraft"), false, true,
        snbt("{contents:{a:{x:1b},\"b/c\":{s:\"str\"}}}"));
    run("scheduled_events", TimerQueue.TYPE, false, true, file("data/minecraft/scheduled_events.dat"),
        snbt("{events:[{callback:{id:\"t:u\",type:\"minecraft:function_tag\"},id:\"y\",trigger_time:3L},{callback:{id:\"c:d\",type:\"minecraft:function\"},id:\"x\",trigger_time:7L}]}"));
    run("scoreboard", ScoreboardSaveData.TYPE, false, true, file("data/minecraft/scoreboard.dat"),
        snbt("{Objectives:[{Name:\"o\",DisplayName:\"O\",CriteriaName:\"dummy\",RenderType:\"integer\",display_auto_update:1b}],PlayerScores:[{Name:\"p\",Objective:\"o\",Score:5}],DisplaySlots:{sidebar:\"o\"},Teams:[{Name:\"t\",DisplayName:\"T\",TeamColor:\"red\",AllowFriendlyFire:1b,SeeFriendlyInvisibles:1b,NameTagVisibility:\"always\",DeathMessageVisibility:\"always\",CollisionRule:\"always\",Players:[\"p\"]}]}"));
    out.close();
  }
}
