// Golden for ServerStatsCounter: parse of stats files (valid and malformed), toJson (file content, incl. key order),
// and the ClientboundAwardStatsPacket entry order (dirty HashSet -> fastutil Object2IntOpenHashMap).
//   scripts/golden-stats.sh   (java -cp "$(scripts/vanilla-cp.sh)" StatsGolden.java > stats_golden.txt)
import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import it.unimi.dsi.fastutil.objects.Object2IntMap;
import it.unimi.dsi.fastutil.objects.Object2IntOpenHashMap;
import java.lang.reflect.Method;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.protocol.game.ClientboundAwardStatsPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.stats.*;
import net.minecraft.util.datafix.DataFixers;

public class StatsGolden {
   static class S extends ServerStatsCounter {
      S() { super(null, Path.of("/nonexistent/stats.json")); }
      String json() { return toJson().toString(); }
      @SuppressWarnings("unchecked")
      String packet() throws Exception {
         Method m = ServerStatsCounter.class.getDeclaredMethod("getDirty");
         m.setAccessible(true);
         Set<Stat<?>> dirty = (Set<Stat<?>>) m.invoke(this);
         Object2IntOpenHashMap<Stat<?>> map = new Object2IntOpenHashMap<>();
         for (Stat<?> s : dirty) map.put(s, getValue(s));
         StringBuilder sb = new StringBuilder();
         for (Object2IntMap.Entry<Stat<?>> e : new ClientboundAwardStatsPacket(map).stats().object2IntEntrySet()) {
            sb.append(e.getKey().getName()).append('=').append(e.getIntValue()).append(' ');
         }
         return sb.toString().trim();
      }
   }

   static java.io.PrintStream out = System.out; // captured before Bootstrap redirects System.out into the logger
   static int n = 0;

   static void parseCase(String input) throws Exception {
      S s = new S();
      // a file without `DataVersion` is treated as 1.12 (1343) and run through the legacy fixers: always give it the current one
      JsonElement element = JsonParser.parseString(input);
      if (element.isJsonObject() && !element.getAsJsonObject().has("DataVersion")) element.getAsJsonObject().addProperty("DataVersion", 5023);
      input = element.toString();
      s.parse(DataFixers.getDataFixer(), element);
      s.markAllDirty();
      out.println("case " + n++ + " parse");
      out.println("in\t" + input);
      out.println("out\t" + s.json());
      out.println("packet\t" + s.packet());
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   static Stat<?> pick(Random r) {
      List<StatType<?>> types = new ArrayList<>();
      BuiltInRegistries.STAT_TYPE.forEach(types::add);
      StatType t = types.get(r.nextInt(types.size()));
      Registry reg = t.getRegistry();
      List<Object> all = new ArrayList<>();
      reg.forEach(all::add);
      Object v = t == Stats.CUSTOM ? Stats.CUSTOM.getRegistry().getValue(Identifier.withDefaultNamespace(customName(r))) : all.get(r.nextInt(Math.min(all.size(), 400)));
      return t.get(v);
   }

   static String customName(Random r) {
      List<Identifier> all = new ArrayList<>();
      BuiltInRegistries.CUSTOM_STAT.keySet().forEach(all::add);
      return all.get(r.nextInt(all.size())).getPath();
   }

   static String typeName(Stat<?> s) { return BuiltInRegistries.STAT_TYPE.getKey(s.getType()).toString(); }

   @SuppressWarnings({"unchecked", "rawtypes"})
   static String valueName(Stat<?> s) { return ((Registry) s.getType().getRegistry()).getKey(s.getValue()).toString(); }

   public static void main(String[] args) throws Exception {
      out = new java.io.PrintStream(new java.io.FileOutputStream(java.io.FileDescriptor.out), true, "UTF-8");
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      parseCase("{}");
      parseCase("{\"stats\":{},\"DataVersion\":5023}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"minecraft:stone\":5,\"minecraft:dirt\":2},\"minecraft:custom\":{\"minecraft:jump\":7,\"minecraft:walk_one_cm\":1234}},\"DataVersion\":5023}");
      // malformed: unknown type / value, wrong value kinds, bare names, negative, huge, fractional, boolean, string
      parseCase("{\"stats\":{\"minecraft:nope\":{\"minecraft:stone\":1},\"minecraft:mined\":{\"minecraft:nope\":1,\"minecraft:stone\":2}}}");
      parseCase("{\"stats\":{\"mined\":{\"stone\":3},\"minecraft:custom\":{\"jump\":4}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"minecraft:stone\":\"7\",\"minecraft:dirt\":true,\"minecraft:sand\":1.5,\"minecraft:gravel\":3000000000,\"minecraft:clay\":-4,\"minecraft:grass_block\":1e3}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":[1,2],\"minecraft:custom\":{\"minecraft:jump\":9}}}");
      parseCase("{\"stats\":[1]}");
      parseCase("[1,2]");
      parseCase("{\"stats\":{\"minecraft:killed\":{\"minecraft:zombie\":3}}}");
      parseCase("{\"stats\":{\"minecraft:killed_by\":{\"minecraft:zombie\":3}}}");
      parseCase("{\"stats\":{\"minecraft:broken\":{\"minecraft:diamond_pickaxe\":3}}}");
      parseCase("{\"stats\":{\"minecraft:crafted\":{\"minecraft:stick\":3}}}");
      parseCase("{\"stats\":{\"minecraft:used\":{\"minecraft:bread\":3}}}");
      parseCase("{\"stats\":{\"minecraft:picked_up\":{\"minecraft:dirt\":3}}}");
      parseCase("{\"stats\":{\"minecraft:dropped\":{\"minecraft:dirt\":3}}}");
      parseCase("{\"stats\":{\"minecraft:custom\":{\"minecraft:jump\":3}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"minecraft:stone\":\"7\"}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"minecraft:dirt\":true}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"minecraft:sand\":1.5}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"minecraft:gravel\":3000000000}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"minecraft:clay\":-4}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"minecraft:grass_block\":1e3}}}");
      parseCase("{\"stats\":{\"mined\":{\"minecraft:stone\":3}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"stone\":3}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"minecraft:stone\":3},\"minecraft:custom\":{\"minecraft:nope\":1}}}");
      parseCase("{\"stats\":{\"minecraft:mined\":{\"Minecraft:stone\":3}}}");
      // random big files: ordering through table growth
      for (int seed = 1; seed <= 6; seed++) {
         Random r = new Random(seed);
         int count = new int[] {5, 17, 40, 90, 200, 600}[seed - 1];
         Map<String, Map<String, Integer>> files = new LinkedHashMap<>();
         for (int i = 0; i < count; i++) {
            Stat<?> st = pick(r);
            files.computeIfAbsent(typeName(st), k -> new LinkedHashMap<>()).put(valueName(st), r.nextInt(100000));
         }
         StringBuilder sb = new StringBuilder("{\"stats\":{");
         boolean firstType = true;
         for (var te : files.entrySet()) {
            if (!firstType) sb.append(',');
            firstType = false;
            sb.append('"').append(te.getKey()).append("\":{");
            boolean first = true;
            for (var ve : te.getValue().entrySet()) {
               if (!first) sb.append(',');
               first = false;
               sb.append('"').append(ve.getKey()).append("\":").append(ve.getValue());
            }
            sb.append('}');
         }
         sb.append("}}");
         parseCase(sb.toString());
      }
      // dirty tracking through increment/setValue (no parse): ops list "type|value|delta;..."
      for (int seed = 11; seed <= 16; seed++) {
         Random r = new Random(seed);
         int count = new int[] {3, 12, 30, 80, 150, 300}[seed - 11];
         S s = new S();
         StringBuilder ops = new StringBuilder();
         for (int i = 0; i < count; i++) {
            Stat<?> st = pick(r);
            int delta = r.nextInt(50) - 5;
            ops.append(typeName(st)).append('|').append(valueName(st)).append('|').append(delta).append(';');
            s.increment(null, st, delta);
         }
         out.println("case " + n++ + " ops");
         out.println("in\t" + ops);
         out.println("out\t" + s.json());
         out.println("packet\t" + s.packet());
      }
      // saturation
      S s = new S();
      Stat<?> jump = Stats.CUSTOM.get(Stats.JUMP);
      s.increment(null, jump, 2147483000);
      s.increment(null, jump, 2147483000);
      out.println("case " + n++ + " saturate\nvalue\t" + s.getValue(jump));
   }
}
