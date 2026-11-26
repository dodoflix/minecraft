//! `PlayerList.BAN_DATE_FORMAT` (`yyyy-MM-dd 'at' HH:mm:ss z`) against `SimpleDateFormat(.., Locale.ROOT)` in 21 default
//! time zones over dates from year 29720 BC to AD 31690 (`fixtures/BanDateGolden.java`): the JDK's zone history, not the
//! C library's. One test function: it changes the process time zone.

use mc_game::server::players::ban_list_entry::Date;
use mc_game::server::players::player_list::format_ban_date;

#[test]
fn ban_date_format_matches_java_in_every_zone() {
    let mut checked = 0;
    for line in include_str!("../../../mc-game/tests/fixtures/ban_date_golden.txt").lines() {
        let mut parts = line.splitn(3, '|');
        let (zone, epoch, expected) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        // SAFETY: this binary has a single test function and starts no other threads, so nothing reads the
        // environment concurrently with `set_var`.
        unsafe {
            std::env::set_var("TZ", zone);
            libc_tzset();
        }
        let date = Date::from_millis(epoch.parse().unwrap());
        assert_eq!(format_ban_date(&date), expected, "{zone} {epoch}");
        checked += 1;
    }
    assert!(checked > 900);
}

unsafe extern "C" {
    #[link_name = "tzset"]
    fn libc_tzset();
}
