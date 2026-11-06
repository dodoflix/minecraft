import com.mojang.serialization.*;
import net.minecraft.nbt.*;
import net.minecraft.resources.*;
import net.minecraft.world.*;
import net.minecraft.world.clock.*;
import net.minecraft.world.level.saveddata.*;
import net.minecraft.world.level.timers.*;
import net.minecraft.server.MinecraftServer;
import java.util.*;

/** SNBT of vanilla-encoded saved data.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/saved_data.golden crates/mc-game/tests/fixtures/SavedDataGolden.java crates/mc-game/tests/fixtures/saved_data.golden
 * Output: crates/mc-game/tests/fixtures/saved_data.golden */
public class SavedDataGolden {
  static java.io.PrintStream out;
  static void p(String k, Object v) { out.println(k + "\t" + v); }
  static <T> String enc(Codec<T> c, T v) { return c.encodeStart(NbtOps.INSTANCE, v).getOrThrow().toString(); }
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
    out.close();
  }
}
