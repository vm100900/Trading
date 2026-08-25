use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::America::New_York;

/// Returns the UTC instant corresponding to 09:30 America/New_York on the
/// trading day containing `at` (also a UTC instant). Returns `None` only in
/// the practically-impossible case that 09:30 local time doesn't resolve to
/// a single unambiguous instant — US DST transitions happen at 02:00 local
/// time, nowhere near 09:30, so this should never actually occur.
pub fn session_start_utc(at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local = at.with_timezone(&New_York);
    let trading_date = local.date_naive();
    let session_start_naive = trading_date.and_hms_opt(9, 30, 0)?;
    let session_start_local = New_York.from_local_datetime(&session_start_naive).single()?;
    Some(session_start_local.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn utc(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
            .and_utc()
    }

    #[test]
    fn session_start_in_winter_est_is_1430_utc() {
        // 2026-01-15 is well before the 2026-03-08 US spring-forward
        // transition, so EST (UTC-5) applies: 09:30 ET = 14:30 UTC.
        let at = utc(2026, 1, 15, 20, 0);
        let start = session_start_utc(at).unwrap();
        assert_eq!(start, utc(2026, 1, 15, 14, 30));
    }

    #[test]
    fn session_start_in_summer_edt_is_1330_utc() {
        // 2026-06-15 is well after spring-forward and before fall-back, so
        // EDT (UTC-4) applies: 09:30 ET = 13:30 UTC.
        let at = utc(2026, 6, 15, 20, 0);
        let start = session_start_utc(at).unwrap();
        assert_eq!(start, utc(2026, 6, 15, 13, 30));
    }

    #[test]
    fn session_start_the_day_after_spring_forward_uses_edt() {
        // US DST began 2026-03-08 at 02:00 local; 2026-03-09 is fully EDT.
        let at = utc(2026, 3, 9, 20, 0);
        let start = session_start_utc(at).unwrap();
        assert_eq!(start, utc(2026, 3, 9, 13, 30));
    }

    #[test]
    fn session_start_the_day_after_fall_back_uses_est() {
        // US DST ended 2026-11-01 at 02:00 local; 2026-11-02 is fully EST.
        let at = utc(2026, 11, 2, 20, 0);
        let start = session_start_utc(at).unwrap();
        assert_eq!(start, utc(2026, 11, 2, 14, 30));
    }

    #[test]
    fn session_start_is_the_same_regardless_of_time_of_day_within_the_session() {
        let morning = utc(2026, 1, 15, 15, 0);
        let afternoon = utc(2026, 1, 15, 20, 0);
        assert_eq!(session_start_utc(morning), session_start_utc(afternoon));
    }
}
