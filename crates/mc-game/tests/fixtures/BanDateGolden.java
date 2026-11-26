import java.text.SimpleDateFormat;
import java.util.*;

/** `PlayerList.BAN_DATE_FORMAT` in several default time zones: `zone|epochMillis|formatted`. The epochs run forward and
 * then backward through one format object per zone, because the zone's rule cache moves with every date.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/ban_date_golden.txt crates/mc-game/tests/fixtures/BanDateGolden.java
 * Output: crates/mc-game/tests/fixtures/ban_date_golden.txt */
public class BanDateGolden {
    public static void main(String[] a) {
        long[] epochs = {0L, 1_700_000_000_000L, 1_711_846_800_000L, 1_711_846_799_000L, 1_730_000_000_000L, 1_729_990_000_000L, 1_752_000_000_000L, 1_782_000_000_000L,
            -2_000_000_000_000L, -12_219_292_800_001L, -12_219_292_800_000L, -30_000_000_000_000L, -62_135_769_600_000L, -70_000_000_000_000L,
            4_102_444_800_000L, 4_118_000_000_000L, 4_133_980_800_000L, 10_000_000_000_000L, 32_503_680_000_000L, 200_000_000_000_000L,
            -1_000_000_000_000_000L, 1_000_000_000_000_000L};
        String[] zones = {"UTC", "Etc/UTC", "Europe/Berlin", "America/New_York", "Asia/Tokyo", "Australia/Sydney", "Europe/London", "Europe/Amsterdam",
            "Asia/Kolkata", "Australia/Lord_Howe", "Europe/Dublin", "Africa/Casablanca", "IET", "Etc/GMT+5", "America/Sao_Paulo", "Europe/Istanbul",
            "Asia/Kathmandu", "Pacific/Apia", "America/St_Johns", "Asia/Tehran", "Europe/Moscow"};
        for (String zone : zones) {
            TimeZone.setDefault(TimeZone.getTimeZone(zone));
            SimpleDateFormat f = new SimpleDateFormat("yyyy-MM-dd 'at' HH:mm:ss z", Locale.ROOT);
            for (long e : epochs) System.out.println(zone + "|" + e + "|" + f.format(new Date(e)));
            for (int i = epochs.length - 1; i >= 0; i--) System.out.println(zone + "|" + epochs[i] + "|" + f.format(new Date(epochs[i])));
        }
    }
}
