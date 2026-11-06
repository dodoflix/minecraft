import java.text.SimpleDateFormat;
import java.util.*;

/** `PlayerList.BAN_DATE_FORMAT` in several default time zones: `zone|epochMillis|formatted`.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/ban_date_golden.txt crates/mc-game/tests/fixtures/BanDateGolden.java
 * Output: crates/mc-game/tests/fixtures/ban_date_golden.txt */
public class BanDateGolden {
    public static void main(String[] a) {
        long[] epochs = {0L, 1_700_000_000_000L, 1_711_846_800_000L, 1_711_846_799_000L, 1_730_000_000_000L, 1_729_990_000_000L, 1_752_000_000_000L, 1_782_000_000_000L};
        for (String zone : new String[]{"UTC", "Europe/Berlin", "America/New_York", "Asia/Tokyo", "Australia/Sydney", "Europe/London"}) {
            TimeZone.setDefault(TimeZone.getTimeZone(zone));
            SimpleDateFormat f = new SimpleDateFormat("yyyy-MM-dd 'at' HH:mm:ss z", Locale.ROOT);
            for (long e : epochs) System.out.println(zone + "|" + e + "|" + f.format(new Date(e)));
        }
    }
}
