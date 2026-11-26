import java.io.*;
import java.lang.reflect.Field;
import java.text.*;
import java.util.*;
import java.util.zip.GZIPOutputStream;

/**
 * The rules `java.util.TimeZone.getTimeZone(id)` (a `sun.util.calendar.ZoneInfo`) gives each zone of
 * `DateFormatSymbols.getInstance(Locale.ROOT).getZoneStrings()`: its history is what `SimpleDateFormat` uses to place a
 * parsed local time in a zone it named. One line per zone, tab separated: `id`, raw offset, DST saving, the `offsets`
 * table, the `transitions` (`utc millis << 12 | dst index << 4 | offset index`), the parameters of the `SimpleTimeZone`
 * that runs after the last transition (`-` for none). A zone with the same rules as an earlier one is `id` and `=` and
 * that zone's id.
 * Run with the JDK the vanilla server runs on first on PATH; the class is read by reflection.
 * Usage: java --add-opens java.base/sun.util.calendar=ALL-UNNAMED crates/mc-game/tests/fixtures/ZoneRulesGen.java crates/mc-game-core/src/server/players/java_zone_rules.tsv.gz
 * Output: crates/mc-game-core/src/server/players/java_zone_rules.tsv.gz (gzip)
 */
public class ZoneRulesGen {
   static Object get(Object zone, String name) throws Exception {
      Field f = zone.getClass().getDeclaredField(name);
      f.setAccessible(true);
      return f.get(zone);
   }

   static String join(Object array) {
      if (array == null) return "-";
      StringBuilder sb = new StringBuilder();
      int n = java.lang.reflect.Array.getLength(array);
      for (int i = 0; i < n; i++) sb.append(i > 0 ? "," : "").append(java.lang.reflect.Array.get(array, i));
      return n == 0 ? "." : sb.toString();
   }

   public static void main(String[] args) throws Exception {
      StringBuilder out = new StringBuilder();
      Map<String, String> seen = new HashMap<>();
      for (String[] z : DateFormatSymbols.getInstance(Locale.ROOT).getZoneStrings()) {
         TimeZone t = TimeZone.getTimeZone(z[0]);
         if (!t.getClass().getName().equals("sun.util.calendar.ZoneInfo") || !t.getID().equals(z[0])) throw new IllegalStateException(z[0]);
         if ((int) get(t, "rawOffsetDiff") != 0) throw new IllegalStateException(z[0]);
         String body = get(t, "rawOffset") + "\t" + get(t, "dstSavings") + "\t" + join(get(t, "offsets")) + "\t" + join(get(t, "transitions"))
            + "\t" + join(get(t, "simpleTimeZoneParams"));
         String same = seen.putIfAbsent(body, z[0]);
         out.append(z[0]).append('\t').append(same == null ? body : "=\t" + same).append('\n');
      }
      try (Writer w = new OutputStreamWriter(new GZIPOutputStream(new FileOutputStream(args[0])), "UTF-8")) {
         w.write(out.toString());
      }
   }
}
