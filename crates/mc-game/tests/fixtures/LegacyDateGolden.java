import java.text.*;
import java.util.*;

/**
 * `BanListEntry.DATE_FORMAT` (`new SimpleDateFormat("yyyy-MM-dd HH:mm:ss Z", Locale.ROOT)`), lenient, in the JDK the
 * vanilla server runs on. Lines (text fields are UTF-16 units as lowercase hex joined by commas):
 *   parse  input                result (epoch milliseconds or `null`)   one parse per fresh format, default zone UTC
 *   seq    inputs (joined by ;) formatted epoch 0 and 1.7e12 before and after each input, default zone Europe/Berlin:
 *                               the format is one shared object, as in vanilla, so a parsed zone name stays
 *   fmt    epoch ms, offset     the format of the instant in the zone GMT+offset (offset in seconds)
 *   zseq   operations on one format (p<text hex> parse, f<epoch ms> format, joined by ;), then their results
 *   zfmt   zone name, then the format of six instants at the ends of the `long` range in the zone a parse of that name left
 * Pinned to Temurin 25.0.5.1 (tzdata 2026f): run it with that JDK first on PATH; regenerate together with `java_zone_rules.tsv.gz`
 * and `legacy_date_golden.txt.gz` when the tzdata of the JDK changes.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/legacy_date_golden.txt.gz crates/mc-game/tests/fixtures/LegacyDateGolden.java
 * Output: crates/mc-game/tests/fixtures/legacy_date_golden.txt.gz
 */
public class LegacyDateGolden {
   static String hex(String s) {
      StringBuilder sb = new StringBuilder();
      for (int i = 0; i < s.length(); i++) sb.append(i > 0 ? "," : "").append(Integer.toHexString(s.charAt(i)));
      return sb.toString();
   }

   static SimpleDateFormat fresh() {
      return new SimpleDateFormat("yyyy-MM-dd HH:mm:ss Z", Locale.ROOT);
   }

   static String parse(SimpleDateFormat f, String s) {
      Date d = f.parse(s, new ParsePosition(0));
      return d == null ? "null" : Long.toString(d.getTime());
   }

   static final String[] YEARS = {
      "2013", "2013", "2013", "2000", "1999", "1970", "1969", "0", "00", "000", "0000", "1", "01", "001", "0001", "99", "100", "9999", "10000", "99999",
      "262142", "262143", "262144", "1582", "1581", "1583", "1500", "1400", "1000", "4", "-1", "-4", "-4712", "-5", "+2013", "2013.7", "2013.", "2013E0", "2013E1",
      "2013e1", "201E-1", "2E3", "2E", "2E+3", "2E-", "2147483647", "2147483648", "-2147483648", "-2147483649", "4294967296", "4294967297", "9223372036854775807",
      "9223372036854775808", "-9223372036854775808", "-9223372036854775809", "99999999999999999999", "123456789012345678901234567890", "1E30", "1E400", "1E-30", "1E-9223372036854775808", "1E9223372036854775807", "1E-9223372036854775809", "1E99999999999999999999", "1E-99999999999999999999",
      "0.5", ".5", "-.5", "-", "+", "", "x", "NaN", "-NaN", "NaNx", "\u221e", "-\u221e", "\u221ex", "\uff12\uff10\uff11\uff13", "\u0662\u0660\u0661\u0663",
      "\u0968\u0966\u0967\u0969", "2\u0660\u0661\u0663", "20 13", "20\t13", " 2013", "\t2013", "  2013", "\u00a02013", "0002013", "000", "00000000000"
   };
   static final String[] SMALL = {
      "02", "02", "02", "2", "1", "12", "13", "00", "0", "99", "31", "32", "30", "29", "28", "24", "25", "23", "59", "60", "61", "62", "100", "-1", "-2", "-12", "-13",
      "+1", "1.5", "1E1", "2147483647", "2147483648", "-2147483648", "99999999999", "x", "", " 3", "\t3", "  03", "3 ", "\u00a03", "\uff10\uff13", "\u0663", "NaN", "\u221e", "007",
      "0E0", "01E0", "1E+1", "1E1x"
   };
   static final String[] ZONES = {
      "+0100", "+0100", "+0100", "-0100", "+0000", "-0000", "+2300", "+2359", "-2359", "+2400", "+0060", "+0059", "+0160", "+01", "+1", "+010", "+01:00", "+0100x",
      "+0100 trailing", "+0100+0100", "+01000", "-", "+", "+x", "+ 0100", "+\uff10\uff11\uff10\uff10", "GMT", "gmt", "Gmt", "GMTx", "GMT+01:00", "GMT-01:00", "GMT+1", "GMT+01",
      "GMT+0100", "GMT+23:59", "GMT+24:00", "GMT+01:60", "GMT+1:00", "GMT+01:0", "GMT+01:00x", "gmt+05:30", "GMT+", "GMT-", "GMT+x", "GMT +01:00", "UTC", "utc", "Utc", "UTCx", "UTC+01:00",
      "UT", "Z", "z", "U", "EST", "EDT", "est", "eSt", "ESTx", "EET", "EEST", "eest", "PST", "PDT", "CET", "CEST", "MST", "JST", "IST", "Belize (+0)", "belize (+0)", "Belize (+1)", "Belize (+0",
      "Belize", "Tokyo (+0)", "London (+0)", "London (+1)", "Kyiv (+0)", "Kyiv (+1)", "Chicago (+1)", "Chicago (+0)x", "Petersburg, Indiana (+0)", "Cuiab\u00e1 (+0)", "CUIAB\u00c1 (+0)",
      "\u00dcr\u00fcmqi (+0)", "Europe/Berlin", "America/New_York", "Pacific Standard Time", "", " ", "x", "\u00a0+0100", " +0100", "\t+0100", " GMT+01:00", " UTC", "  GMT+01:00",
      "\u00a0GMT", "GMT\u00a0", "\ufeff+0100"
   };
   // zones that are the same for every date of a year since 2000 (names of zones with DST rules need the zone's history)
   static final String[] SEPARATORS_DATE = {"-", "-", "-", "-", "/", ".", " ", "", "--", "\u00a0", "\u2007", "\u2212", "\u2013"};
   static final String[] SEPARATORS_SPACE = {" ", " ", " ", " ", "  ", "\t", "\u00a0", "\u2003", "\u202f", "T", "", "_", "\u2007", "\u3000", "\u180e", "\r", "\n"};
   static final String[] SEPARATORS_TIME = {":", ":", ":", ":", ".", "-", " ", "", "::"};
   static final String[] TRAILING = {"", "", "", "", "", " ", "x", " trailing", "\n", "|more", "\u0000", "0", "-", "+0100", "\uff10", "\ud835\udfd8"};
   static final String[] LEADING = {"", "", "", "", "", "", " ", "\t", "x", "\u00a0", "\ufeff"};

   static String pick(Random r, String[] a) {
      return a[r.nextInt(a.length)];
   }

   public static void main(String[] args) {
      StringBuilder out = new StringBuilder();
      TimeZone.setDefault(TimeZone.getTimeZone("UTC"));
      List<String> inputs = new ArrayList<>();
      String[] base = {"2013-02-03 04:05:06 +0100", "2013-02-30 25:61:61 +0100", "2013-13-32 24:60:60 +0100", "2013-00-00 00:00:00 +0000", "2013-1-3 4:5:6 +0100",
         "0000-01-01 00:00:00 +0000", "0001-01-01 00:00:00 +0000", "1582-10-04 23:59:59 +0000", "1582-10-05 00:00:00 +0000", "1582-10-14 12:00:00 +0000", "1582-10-15 00:00:00 +0000",
         "1582-10-10 00:00:00 +0000", "1582-12-31 23:59:59 +0000", "1582-01-01 00:00:00 +0000", "1582-13-01 00:00:00 +0000", "1581-13-01 00:00:00 +0000", "1583-00-01 00:00:00 +0000",
         "1500-02-29 00:00:00 +0000", "1500-03-01 00:00:00 +0000", "1900-02-29 00:00:00 +0000", "2000-02-29 00:00:00 +0000", "2100-02-29 00:00:00 +0000", "1970-01-01 00:00:00 +0000",
         "-0001-01-01 00:00:00 +0000", "-1-01-01 00:00:00 +0000", "2013-02-03 04:05:06", "2013-02-03 04:05:06 ", "2013-02-03 04:05", "2013-02-03", "2013-02", "2013", "",
         " ", "x", "2013-02-03 04:05:06 +0100", "  2013-02-03 04:05:06 +0100", "2013-02-03  04:05:06  +0100", "2013-02-03\t04:05:06\t+0100", "2013-02-03 04:05:06 +0100\n",
         "2013-02-03 04:05:06 +0100 more", "2013-02-03 04:05:06 +01001", "2013-02-03 04:05:06 +0100|x", "2013-02-03 04:05:06 +0160", "2013-02-03 04:05:06 +2400",
         "2013-02-03 24:00:00 +0000", "2013-02-03 -1:-1:-1 +0000", "2013-02-03 99:99:99 +0000", "2013-02-03 2147483647:0:0 +0000", "2013-02-03 0:2147483647:0 +0000",
         "2013-02-03 0:0:2147483647 +0000", "2013-02-2147483647 0:0:0 +0000", "2013-2147483647-3 0:0:0 +0000", "2147483647-02-03 0:0:0 +0000", "-2147483648-02-03 0:0:0 +0000",
         "2147483647-12-31 23:59:59 +0000", "2013--2147483648-03 0:0:0 +0000", "2013-02--2147483648 0:0:0 +0000", "2013-2147483647-2147483647 2147483647:2147483647:2147483647 +0000",
         "1000000-01-01 00:00:00 +0000", "262142-12-31 23:59:59 +0000", "262143-01-01 00:00:00 +0000", "-262143-01-01 00:00:00 +0000", "-262144-12-31 23:59:59 +0000", "292277026596-12-04 15:30:07 +0000",
         "292278994-08-17 07:12:55 +0000", "292278994-08-17 07:12:56 +0000", "292269055-12-02 16:47:04 +0000", "292269055-12-02 16:47:03 +0000"};
      for (String b : base) inputs.add(b);
      for (String y : YEARS) inputs.add(y + "-02-03 04:05:06 +0100");
      for (String s : SMALL) {
         inputs.add("2013-" + s + "-03 04:05:06 +0100");
         inputs.add("2013-02-" + s + " 04:05:06 +0100");
         inputs.add("2013-02-03 " + s + ":05:06 +0100");
         inputs.add("2013-02-03 04:" + s + ":06 +0100");
         inputs.add("2013-02-03 04:05:" + s + " +0100");
      }
      for (String z : ZONES) {
         inputs.add("2013-02-03 04:05:06 " + z);
         inputs.add("2013-07-03 04:05:06 " + z);
         inputs.add("2013-02-03 04:05:06" + z);
      }
      Random r = new Random(20261121);
      for (int i = 0; i < 20000; i++) {
         String y = r.nextInt(4) == 0 ? pick(r, YEARS) : pick(r, new String[] {"2013", "2014", "1582", "1500", "0", "2000", "1969", "10000", "1"});
         String mo = r.nextInt(3) == 0 ? pick(r, SMALL) : pick(r, new String[] {"01", "02", "10", "12", "7"});
         String d = r.nextInt(3) == 0 ? pick(r, SMALL) : pick(r, new String[] {"01", "03", "15", "31", "5", "4", "14"});
         String h = r.nextInt(3) == 0 ? pick(r, SMALL) : pick(r, new String[] {"00", "04", "12", "23"});
         String mi = r.nextInt(3) == 0 ? pick(r, SMALL) : pick(r, new String[] {"00", "05", "59"});
         String se = r.nextInt(3) == 0 ? pick(r, SMALL) : pick(r, new String[] {"00", "06", "59"});
         String z = r.nextInt(2) == 0 ? pick(r, new String[] {"+0100", "-0530", "+0000", "GMT", "UTC", "GMT+05:30", "+1200", "-1100"}) : pick(r, ZONES);
         // a zone name of a zone with DST or a history is only exact for the dates of the zone's current rules
         boolean named = z.matches(".*[A-Za-z].*") && !z.startsWith("GMT") && !z.startsWith("gmt") && !z.startsWith("Gmt") && !z.toUpperCase().startsWith("UTC");
         if (named) {
            y = "2013";
            mo = pick(r, new String[] {"01", "07"});
            d = pick(r, new String[] {"01", "15", "28"});
            h = pick(r, new String[] {"00", "04", "12", "23"});
            mi = pick(r, new String[] {"00", "05", "59"});
            se = pick(r, new String[] {"00", "06", "59"});
         }
         String s = pick(r, LEADING) + y + (r.nextInt(8) == 0 ? pick(r, SEPARATORS_DATE) : "-") + mo + (r.nextInt(8) == 0 ? pick(r, SEPARATORS_DATE) : "-") + d
            + (r.nextInt(6) == 0 ? pick(r, SEPARATORS_SPACE) : " ") + h + (r.nextInt(8) == 0 ? pick(r, SEPARATORS_TIME) : ":") + mi
            + (r.nextInt(8) == 0 ? pick(r, SEPARATORS_TIME) : ":") + se + (r.nextInt(6) == 0 ? pick(r, SEPARATORS_SPACE) : " ") + z + pick(r, TRAILING);
         inputs.add(s);
      }
      // names of zones with a history: two of the names of every zone at dates over that history
      String[] zoneDates = {"1850-06-15 12:00:00", "1900-01-01 00:00:00", "1945-08-15 03:30:00", "1970-01-01 00:00:00", "1987-07-01 12:00:00", "2007-03-11 02:30:00",
         "2011-11-06 01:30:00", "2024-03-31 02:30:00", "2024-10-27 01:30:00", "2040-07-01 12:00:00", "2040-11-04 01:30:00", "2100-03-14 02:30:00"};
      String[][] zoneStrings = DateFormatSymbols.getInstance(Locale.ROOT).getZoneStrings();
      for (int i = 0; i < zoneStrings.length; i++) {
         for (int k = 0; k < 2; k++) {
            for (String d : zoneDates) inputs.add(d + " " + zoneStrings[i][1 + (i + 2 * k) % 4]);
         }
      }
      // the ends of the `long` range in zones with daylight rules (the rule's transitions overflow there)
      for (String zone : new String[] {"New York (+0)", "New York (+1)", "Sydney (+0)", "Sydney (+1)", "Kyiv (+0)", "Tokyo (+0)"}) {
         for (String d : new String[] {"292278994-01-01 00:00:00", "292278994-08-17 07:00:00", "292278994-08-17 07:12:55", "292278994-12-31 23:59:59", "-292275055-05-16 16:47:04",
            "-292275055-01-01 00:00:00", "262143-07-01 12:00:00", "300000-03-15 02:30:00"}) {
            inputs.add(d + " " + zone);
         }
      }
      Set<String> seen = new LinkedHashSet<>();
      for (String s : inputs) {
         if (!seen.add(s) || !valid(s)) continue;
         out.append("parse\t").append(hex(s)).append('\t').append(parse(fresh(), s)).append('\n');
      }

      // One shared format, as vanilla's DATE_FORMAT: the zone of a parsed name stays.
      String[][] sequences = {
         {"2013-02-03 04:05:06 +0100"},
         {"2013-02-03 04:05:06 UTC"},
         {"2013-02-03 04:05:06 UTC", "2013-02-03 04:05:06 +0100"},
         {"2013-02-03 04:05:06 +0100", "2013-02-03 04:05:06 UTC", "2013-02-03 04:05:06 GMT"},
         {"2013-02-03 04:05:06 GMT"},
         {"2013-02-03 04:05:06 GMT+02:00", "2013-02-03 04:05:06 utc"},
         {"2013-02-03 04:05:06 UTCx", "2013-02-03 04:05:06 +0100", "bad", "2013-02-03 04:05:06 Zulu"},
         {"2013-02-03 04:05:06  GMT+02:00"},
         {"2013-02-03 04:05:06 UT", "2013-02-03 04:05:06 UTC"},
         {"2013-02-03 04:05:06 EST"},
         {"2013-07-03 04:05:06 EDT"},
         {"1900-01-01 00:00:00 EST", "2013-07-03 04:05:06 +0100"},
         {"2013-07-03 04:05:06 PST", "2013-07-03 04:05:06 PDT"},
         {"2013-07-03 04:05:06 CET", "2013-07-03 04:05:06 CEST", "2013-02-03 04:05:06 EET"},
         {"2013-02-03 04:05:06 JST", "2013-02-03 04:05:06 UTC"},
         {"2013-02-03 04:05:06 Tokyo (+0)", "2013-07-03 04:05:06 Kyiv (+1)"},
         {"2013-07-03 04:05:06 London (+1)", "2013-02-03 04:05:06 London (+0)"},
         {"2013-07-03 04:05:06 Belize (+0)", "2013-07-03 04:05:06 GMT+02:00"}
      };
      // the instants at the ends of the `long` range, printed in the zone a parsed name left
      for (String zone : new String[] {"New York (+0)", "Sydney (+1)", "Kyiv (+0)", "Tokyo (+0)"}) {
         SimpleDateFormat f = fresh();
         parse(f, "2013-02-03 04:05:06 " + zone);
         StringBuilder line = new StringBuilder("zfmt\t").append(hex(zone));
         for (long e : new long[] {Long.MAX_VALUE, Long.MIN_VALUE, Long.MAX_VALUE - 86_400_000L * 120, Long.MAX_VALUE - 86_400_000L * 300, Long.MIN_VALUE + 86_400_000L * 120, 8_210_266_876_800_000L}) {
            line.append('\t').append(f.format(new Date(e)));
         }
         out.append(line).append('\n');
      }
      // one format, the rule cache of its zone: a date at the end of the `long` range makes the cached range of a
      // southern zone cover almost every later date
      for (String zone : new String[] {"Sydney (+0)", "Auckland (+1)", "New York (+0)", "Santiago (+0)", "Tokyo (+0)"}) {
         String[][] scripts = {
            {"p292278994-01-01 00:00:00 " + zone, "p2150-07-01 12:00:00 " + zone, "f5695927200000", "f4108692600000", "f1700000000000", "p2013-02-03 04:05:06 " + zone},
            {"p2013-02-03 04:05:06 " + zone, "f9223372036854775807", "p2150-07-01 12:00:00 " + zone, "f5695927200000", "f4108692600000"},
            {"p292278994-01-01 00:00:00 " + zone, "p2013-02-03 04:05:06 +0100", "p2150-07-01 12:00:00 " + zone, "f5695927200000"},
            {"p292278994-01-01 00:00:00 " + zone, "p2013-02-03 04:05:06 Tokyo (+1)", "p2150-07-01 12:00:00 " + zone, "f5695927200000"},
            {"p292278994-08-17 07:12:55 " + zone, "f5695927200000", "p2150-07-01 12:00:00 " + zone, "p2150-01-01 12:00:00 " + zone}
         };
         for (String[] script : scripts) {
            SimpleDateFormat f = fresh();
            StringBuilder ops = new StringBuilder("zseq\t");
            StringBuilder results = new StringBuilder();
            for (int i = 0; i < script.length; i++) {
               String op = script[i];
               ops.append(i > 0 ? ";" : "").append(op.charAt(0)).append(op.charAt(0) == 'p' ? hex(op.substring(1)) : op.substring(1));
               results.append('\t').append(op.charAt(0) == 'p' ? parse(f, op.substring(1)) : f.format(new Date(Long.parseLong(op.substring(1)))));
            }
            out.append(ops).append(results).append('\n');
         }
      }
      // the seq lines: the default zone is Europe/Berlin (the zfmt and zseq lines above use UTC)
      TimeZone.setDefault(TimeZone.getTimeZone("Europe/Berlin"));
      for (String[] seq : sequences) {
         SimpleDateFormat f = fresh();
         StringBuilder line = new StringBuilder("seq\t");
         for (int i = 0; i < seq.length; i++) line.append(i > 0 ? ";" : "").append(hex(seq[i]));
         line.append('\t').append(f.format(new Date(0))).append('|').append(f.format(new Date(1_700_000_000_000L)));
         for (String s : seq) {
            line.append('\t').append(parse(f, s)).append('|').append(f.format(new Date(0))).append('|').append(f.format(new Date(1_700_000_000_000L)));
         }
         out.append(line).append('\n');
      }

      long[] instants = {0, 1_700_000_000_000L, -1, 86_399_999, -62_135_596_800_000L, -62_135_596_800_001L, -62_167_219_200_000L, -62_167_219_200_001L,
         -12_219_292_800_000L, -12_219_292_800_001L, -12_219_292_799_999L, -12_220_156_800_000L, -12_219_379_200_000L, -12_219_379_200_001L, -30_610_224_000_000L,
         -30_610_224_000_001L, 951_782_400_000L, 4_107_542_400_000L, 253_402_300_799_000L, 253_402_300_800_000L, 8_210_266_876_799_999L, 8_210_266_876_800_000L,
         -377_705_116_800_000L, -377_705_116_800_001L, -62_198_755_200_000L, -62_198_755_200_001L, -100_000_000_000_000L, -8_000_000_000_000_000L,
         Long.MAX_VALUE, Long.MIN_VALUE, Long.MAX_VALUE - 3_600_000L, Long.MIN_VALUE + 3_600_000L, 9_000_000_000_000_000_000L, -9_000_000_000_000_000_000L};
      int[] offsets = {0, 3600, -18000, 19800, 45000, -12600, 86399 - 3600 * 24 + 3600};
      for (long e : instants) {
         for (int off : offsets) {
            SimpleDateFormat f = fresh();
            f.setTimeZone(new SimpleTimeZone(off * 1000, "x"));
            out.append("fmt\t").append(e).append('\t').append(off).append('\t').append(f.format(new Date(e))).append('\n');
         }
      }
      System.out.print(out);
   }

   static boolean valid(String s) {
      for (int i = 0; i < s.length(); i++) {
         char c = s.charAt(i);
         if (Character.isHighSurrogate(c)) {
            if (i + 1 >= s.length() || !Character.isLowSurrogate(s.charAt(i + 1))) return false;
            i++;
         } else if (Character.isLowSurrogate(c)) return false;
      }
      return true;
   }
}
