import com.mojang.serialization.*;
import net.minecraft.nbt.*;
import net.minecraft.resources.*;
import net.minecraft.world.*;
import net.minecraft.world.clock.*;
import net.minecraft.world.level.saveddata.*;
import net.minecraft.world.level.saveddata.maps.MapIndex;
import net.minecraft.world.entity.raid.Raids;
import net.minecraft.world.level.TicketStorage;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.dimension.end.EnderDragonFight;
import net.minecraft.server.bossevents.CustomBossEvents;
import net.minecraft.world.level.timers.*;
import net.minecraft.server.MinecraftServer;
import com.google.gson.*;
import net.minecraft.stats.*;
import net.minecraft.util.datafix.*;
import java.util.*;

/** SNBT of vanilla-encoded saved data.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/saved_data.golden crates/mc-game/tests/fixtures/SavedDataGolden.java crates/mc-game/tests/fixtures/saved_data.golden
 * Run it from the repository root (the fixture world is read by relative path).
 * Output: crates/mc-game/tests/fixtures/saved_data.golden */
public class SavedDataGolden {
  static java.io.PrintStream out;
  static void p(String k, Object v) { out.println(k + "\t" + v); }
  static <T> String enc(Codec<T> c, T v) { return c.encodeStart(NbtOps.INSTANCE, v).getOrThrow().toString(); }
  /** Players/heroes are `Set.copyOf` sets: their list order is random per JVM run, so sorted. */
  static Tag canon(Tag t) {
    if (t instanceof CompoundTag c) {
      CompoundTag r = new CompoundTag();
      for (String k : c.keySet()) {
        Tag v = canon(c.get(k));
        if (k.equals("Players") && v instanceof ListTag l) {
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

  /** `ok <encoded>` for a complete decode, `partial <encoded>` for a partial one, `err` otherwise. With `fuzzy`, longs within 50 of
   * the input are replaced by it (stopwatches add the milliseconds elapsed since the decode). */
  static <T extends SavedData> void codec(String key, SavedDataType<T> type, String snbt, boolean fuzzy) throws Exception {
    CompoundTag in = TagParser.parseCompoundFully(snbt);
    DataResult<T> r = type.codec().parse(NbtOps.INSTANCE, in);
    Optional<T> v = r.resultOrPartial(e -> {});
    if (v.isEmpty()) { p(key, "err"); return; }
    Tag enc = canon(type.codec().encodeStart(NbtOps.INSTANCE, v.get()).getOrThrow());
    if (fuzzy) enc = unfuzz(enc, in);
    p(key, (r.result().isPresent() ? "ok " : "partial ") + enc);
  }

  static final Gson GSON = new GsonBuilder().setPrettyPrinting().create();

  /** The file bytes of `GSON.toJson(json, GSON.newJsonWriter(writer))` (what the stats and advancement files use), newlines as `¶`. */
  static String gsonFile(JsonElement json) throws Exception {
    java.io.StringWriter w = new java.io.StringWriter();
    GSON.toJson(json, GSON.newJsonWriter(w));
    return w.toString().replace("\n", "¶");
  }

  /** `ServerStatsCounter.save()` of a counter with the given custom stats, the stat types of the file in sorted order. */
  static void statsFile(String key, Object... statsAndCounts) throws Exception {
    java.nio.file.Path file = java.nio.file.Files.createTempDirectory("stats").resolve("stats.json");
    ServerStatsCounter counter = new ServerStatsCounter(null, file);
    for (int i = 0; i < statsAndCounts.length; i += 2) counter.increment(null, (Stat<?>) statsAndCounts[i], (Integer) statsAndCounts[i + 1]);
    counter.save();
    JsonObject saved = JsonParser.parseString(java.nio.file.Files.readString(file)).getAsJsonObject();
    JsonObject stats = new JsonObject();
    new TreeMap<>(saved.getAsJsonObject("stats").asMap()).forEach(stats::add);
    saved.add("stats", stats);
    p(key, gsonFile(saved));
    java.nio.file.Files.delete(file);
    java.nio.file.Files.delete(file.getParent());
  }

  static final String FIXTURE_WORLD = "crates/mc-world/tests/fixtures/level_storage/world/";

  /** Decode -> encode of a vanilla-written file of the committed fixture world (the file's `data` tag), `prepare` in between. */
  static <T extends SavedData> void fixture(String dir, SavedDataType<T> type, java.util.function.Consumer<T> prepare) throws Exception {
    String name = type.id().getPath();
    CompoundTag file = NbtIo.readCompressed(java.nio.file.Path.of(FIXTURE_WORLD + dir + "/data/minecraft/" + name + ".dat"), NbtAccounter.unlimitedHeap());
    T v = type.codec().parse(NbtOps.INSTANCE, file.getCompoundOrEmpty("data")).getOrThrow();
    prepare.accept(v);
    p("fixture " + dir + " " + name, type.codec().encodeStart(NbtOps.INSTANCE, v).getOrThrow());
  }

  static Tag unfuzz(Tag out, CompoundTag in) {
    CompoundTag o = (CompoundTag) out;
    CompoundTag inner = o.getCompoundOrEmpty("stopwatches");
    CompoundTag given = in.getCompound("stopwatches").orElse(new CompoundTag());
    for (String k : inner.keySet()) {
      long got = ((LongTag) inner.get(k)).value();
      Tag was = given.get(k);
      if (was instanceof NumericTag n && Math.abs(got - n.longValue()) <= 50) inner.putLong(k, n.longValue());
    }
    return o;
  }

  public static void main(String[] a) throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion();
    net.minecraft.server.Bootstrap.bootStrap();
    out = new java.io.PrintStream(new java.io.FileOutputStream(a[0]), false, "UTF-8");
    p("weather", enc(WeatherData.CODEC, new WeatherData(1, 2, 3, true, false)));
    p("wandering_default", enc(WanderingTraderData.CODEC, new WanderingTraderData()));
    p("wandering_delay", enc(WanderingTraderData.CODEC, new WanderingTraderData(1, 25)));
    p("wandering_chance", enc(WanderingTraderData.CODEC, new WanderingTraderData(24000, 3)));
    RandomSequences rs = new RandomSequences();
    rs.get(Identifier.parse("minecraft:foo"), 12345L).nextInt();
    rs.get(Identifier.parse("bar:baz/q"), 12345L);
    p("random_sequences_1", enc(RandomSequences.CODEC, rs));
    rs.setSeedDefaults(5, false, false);
    rs.reset(Identifier.parse("a:b"), 77L);
    rs.reset(Identifier.parse("c:d"), 77L, -9, true, false);
    p("random_sequences_2", enc(RandomSequences.CODEC, rs));
    p("clock_default", enc(ClockState.CODEC, new ClockState(7L, 0.0F, 1.0F, false)));
    p("clock_all", enc(ClockState.CODEC, new ClockState(7L, 0.25F, 2.5F, true)));
    p("clock_neg_zero_partial", enc(ClockState.CODEC, new ClockState(7L, -0.0F, 1.0F, false)));
    p("clock_decode_rate_zero", ClockState.CODEC.parse(NbtOps.INSTANCE, TagParser.parseCompoundFully("{total_ticks:1L,rate:0.0f}")).error().isPresent());
    p("clock_decode_rate_neg", ClockState.CODEC.parse(NbtOps.INSTANCE, TagParser.parseCompoundFully("{total_ticks:1L,rate:-1.0f}")).error().isPresent());
    p("clock_decode_lenient", ClockState.CODEC.parse(NbtOps.INSTANCE, TagParser.parseCompoundFully("{total_ticks:3,paused:2b,partial_tick:1}")).getOrThrow());
    // partial results of an unbounded map inside a record
    Tag badRs = TagParser.parseCompoundFully("{salt:1,sequences:{\"a:b\":{source:[L;1L,2L]},\"c:d\":{source:[L;1L]},\"e:f\":5}}");
    DataResult<RandomSequences> r = RandomSequences.CODEC.parse(NbtOps.INSTANCE, badRs);
    p("random_sequences_partial", r.resultOrPartial(e -> {}).map(x -> enc(RandomSequences.CODEC, x)).orElse("none") + " | complete=" + r.result().isPresent());
    // timer queue codec with both server callbacks
    TimerQueue<MinecraftServer> q = new TimerQueue<>();
    q.schedule("x", 7L, new FunctionCallback(Identifier.parse("a:b")));
    q.schedule("y", 3L, new FunctionTagCallback(Identifier.parse("t:u")));
    q.schedule("x", 3L, new FunctionCallback(Identifier.parse("c:d")));
    p("timer_queue", enc(TimerQueue.CODEC, q));
    // `CommandStorage.Container` is private: its codec comes from the type factory
    var typeFactory = Class.forName("net.minecraft.world.level.storage.CommandStorage$Container").getDeclaredMethod("type", String.class);
    typeFactory.setAccessible(true);
    @SuppressWarnings("unchecked")
    SavedDataType<SavedData> commandStorage = (SavedDataType<SavedData>) typeFactory.invoke(null, "minecraft");
    for (String in : new String[] {
        "{contents:{}}",
        "{contents:{a:{x:1b,y:[1,2],z:{k:\"v\"}},\"b/c\":{s:\"str\"}}}",
        "{}",
        "{contents:{A:{}}}",
        "{contents:{a:5}}",
        "{contents:{ok:{x:1},\"bad key\":{y:2}}}",
        "{contents:{empty:{}}}",
        "{contents:[]}",
    }) codec("command_storage " + in, commandStorage, in, false);
    for (String in : new String[] {
        "{stopwatches:{}}",
        "{stopwatches:{\"minecraft:a\":5L,\"b:c\":0L,\"minecraft:z\":-100L,\"x:y\":4000000000L}}",
        "{stopwatches:{\"minecraft:a\":5,\"minecraft:b\":2.5d}}",
        "{stopwatches:{\"Bad:A\":1L,\"ok:id\":2L}}",
        "{stopwatches:{\"a\":\"x\"}}",
        "{}",
    }) codec("stopwatches " + in, Stopwatches.TYPE, in, true);
    for (String in : new String[] {"{}", "{map:5}", "{map:-5}", "{map:7L}", "{map:3.7d}", "{map:\"x\"}", "{map:2147483647}"}) {
      codec("map_index " + in, MapIndex.TYPE, in, false);
    }
    String raid = "started:1b,active:1b,ticks_active:100L,raid_omen_level:2,groups_spawned:1,cooldown_ticks:300,post_raid_ticks:0,total_health:12.5f,group_count:5,status:\"ongoing\",center:[I;10,64,-20]";
    String heroes = "heroes_of_the_village:[[I;1,2,3,4],[I;-5,6,7,-8],[I;100,200,300,400],[I;0,0,0,1],[I;9,9,9,9]]";
    for (String in : new String[] {
        "{next_id:1,tick:0}",
        "{raids:[],next_id:3,tick:5}",
        "{raids:[{id:1," + raid + ",heroes_of_the_village:[]}],next_id:2,tick:77}",
        "{raids:[{id:1," + raid + "," + heroes + "},{id:5," + raid.replace("ongoing", "victory") + ",heroes_of_the_village:[]},"
            + "{id:17," + raid.replace("ongoing", "loss").replace("-20", "99") + ",heroes_of_the_village:[]},"
            + "{id:100000," + raid.replace("ongoing", "stopped") + ",heroes_of_the_village:[]},"
            + "{id:-3," + raid + ",heroes_of_the_village:[]},{id:33," + raid + ",heroes_of_the_village:[]},{id:16," + raid + ",heroes_of_the_village:[]}],next_id:9,tick:2147483647}",
        "{raids:[{id:1," + raid.replace("ongoing", "nope") + ",heroes_of_the_village:[]}],next_id:2,tick:0}",
        "{raids:[{id:1," + raid.replace("center:[I;10,64,-20]", "center:[I;1,2]") + ",heroes_of_the_village:[]}],next_id:2,tick:0}",
        "{raids:[{id:1," + raid + ",heroes_of_the_village:[[I;1,2,3]]}],next_id:2,tick:0}",
        "{raids:[{id:1," + raid + "}],next_id:2,tick:0}",
        "{raids:[{" + raid + ",heroes_of_the_village:[]}],next_id:2,tick:0}",
        "{raids:[],tick:0}",
        "{raids:[],next_id:1}",
    }) codec("raids " + in, Raids.TYPE, in, false);
    for (String in : new String[] {
        "{}",
        "{\"minecraft:a\":{Name:\"Boss\"}}",
        "{\"test:b\":{Name:{text:\"Red\",color:\"red\",bold:1b},Visible:1b,Value:5,Max:20,Color:\"blue\",Overlay:\"notched_12\",DarkenScreen:1b,PlayBossMusic:1b,CreateWorldFog:1b,Players:[[I;1,2,3,4],[I;5,6,7,8],[I;9,9,9,9]]}}",
        "{\"x:t\":{Name:{translate:\"x.y\",with:[\"a\",1]}},\"y:u\":{Name:\"U\",Visible:5b,Value:-4,Max:0},\"z:v\":{Name:\"V\",Color:\"pink\",Overlay:\"progress\"}}",
        "{\"minecraft:a\":{Name:\"Boss\",Color:\"mauve\"}}",
        "{\"minecraft:a\":{Name:\"Boss\",Overlay:\"notched_7\"}}",
        "{\"minecraft:a\":{Name:\"Boss\",Value:\"x\"}}",
        "{\"minecraft:a\":{Visible:1b}}",
        "{\"minecraft:a\":{Name:\"Boss\",Players:[[I;1,2,3]]}}",
        "{\"Bad:Id\":{Name:\"Boss\"},\"ok:id\":{Name:\"Boss\"}}",
        "{\"minecraft:a\":5}",
    }) codec("custom_boss_events " + in, CustomBossEvents.TYPE, in, false);
    for (String dim : new String[] {"overworld", "the_nether", "the_end"}) {
      String dir = "dimensions/minecraft/" + dim;
      fixture(dir, WorldBorder.TYPE, b -> b.applyInitialSettings(0L));
      fixture(dir, Raids.TYPE, b -> {});
      fixture(dir, TicketStorage.TYPE, b -> {});
    }
    fixture("dimensions/minecraft/the_end", EnderDragonFight.TYPE, f -> {});
    // Gson `htmlSafe` escapes: `< > & = '` (and U+2028/U+2029 always) inside keys and strings
    for (String in : new String[] {
        "{\"a<b>\":\"x&y=z'w\",\"n\":[1,2,{\"k\":[]}],\"e\":{},\"s\":\"tab\\there\\nnl \\\"q\\\" \\\\ / é ☃ \\u2028 \\u2029 \\u0001 \\u007f\"}",
        "{\"stats\":{},\"DataVersion\":5023}",
        "[[],[[]],{}]",
    }) p("gson " + in, gsonFile(JsonParser.parseString(in)));
    // `PlayerAdvancements.save()`: the private `Data` codec wrapped with the data version, criterion names are arbitrary strings
    java.util.TimeZone.setDefault(java.util.TimeZone.getTimeZone("UTC"));
    Class<?> dataClass = Class.forName("net.minecraft.server.PlayerAdvancements$Data");
    var dataCodec = dataClass.getDeclaredField("CODEC");
    dataCodec.setAccessible(true);
    @SuppressWarnings("unchecked")
    Codec<Object> advancementData = DataFixTypes.ADVANCEMENTS.wrapCodec((Codec<Object>) dataCodec.get(null), DataFixers.getDataFixer(), 1343);
    for (String in : new String[] {
        "{}",
        "{\"minecraft:story/root\":{\"criteria\":{\"a<b>&c='d'\":\"2024-01-02 03:04:05 +0000\",\"plain\":\"2024-02-03 04:05:06 +0000\",\"x=y\":\"2023-12-31 23:59:59 +0000\",\"\\u2028\":\"2023-12-31 23:59:58 +0000\"},\"done\":true},\"test:b\":{\"criteria\":{\"it's\":\"2024-03-04 05:06:07 +0000\"},\"done\":false}}",
    }) {
      // `asData()`: a `LinkedHashMap` in the order of the player's progress (the decoded map of the codec has its own order)
      Map<Identifier, Object> progress = new LinkedHashMap<>();
      for (var entry : JsonParser.parseString(in).getAsJsonObject().entrySet()) {
        progress.put(Identifier.parse(entry.getKey()), net.minecraft.advancements.AdvancementProgress.CODEC.parse(JsonOps.INSTANCE, entry.getValue()).getOrThrow());
      }
      var dataConstructor = dataClass.getDeclaredConstructors()[0];
      dataConstructor.setAccessible(true);
      JsonElement json = advancementData.encodeStart(JsonOps.INSTANCE, dataConstructor.newInstance(progress)).getOrThrow();
      p("advancements " + in, gsonFile(json));
    }
    // `ServerStatsCounter.save()`
    Stat<?> jump = Stats.CUSTOM.get(Stats.JUMP);
    statsFile("stats_file custom", jump, 7, Stats.CUSTOM.get(Stats.WALK_ONE_CM), 1234, Stats.CUSTOM.get(Stats.PLAY_TIME), 2147483647);
    statsFile("stats_file mixed", jump, 1, Stats.BLOCK_MINED.get(net.minecraft.world.level.block.Blocks.STONE), 5, Stats.ITEM_CRAFTED.get(net.minecraft.world.item.Items.STICK), 3);
    statsFile("stats_file empty");
    out.close();
  }
}
