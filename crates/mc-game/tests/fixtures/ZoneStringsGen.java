import java.io.*;
import java.text.*;
import java.util.*;

/**
 * The zone strings `SimpleDateFormat(.., Locale.ROOT)` matches a parsed zone name against: for each zone of
 * `DateFormatSymbols.getInstance(Locale.ROOT).getZoneStrings()`, in that order, `id`, long and short standard name,
 * long and short daylight name, and the raw offset and DST saving of the zone in milliseconds, tab separated.
 * Run with JDK 25 (the version the vanilla server runs on) first on PATH.
 * Usage: java crates/mc-game/tests/fixtures/ZoneStringsGen.java crates/mc-game-core/src/server/players/java_zone_strings.tsv
 * Output: crates/mc-game-core/src/server/players/java_zone_strings.tsv
 */
public class ZoneStringsGen {
   public static void main(String[] args) throws Exception {
      StringBuilder out = new StringBuilder();
      for (String[] z : DateFormatSymbols.getInstance(Locale.ROOT).getZoneStrings()) {
         TimeZone t = TimeZone.getTimeZone(z[0]);
         out.append(String.join("\t", Arrays.copyOf(z, 5))).append('\t').append(t.getRawOffset()).append('\t').append(t.getDSTSavings()).append('\n');
      }
      try (Writer w = new OutputStreamWriter(new FileOutputStream(args[0]), "UTF-8")) {
         w.write(out.toString());
      }
   }
}
