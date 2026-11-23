//! The time zone a flight was flown in, so that its times read as the
//! clock there showed them: a flight in California at 15:39, not at 00:39
//! the next day because the phone is in Europe.
//!
//! The zone is the IANA identifier of where the flight took off, from the
//! outlines of `venti-sources-timezones`, which work offline; a shell
//! formats a time with its platform's own table of the zones' rules
//! (`TimeZone` on the iPhone, `ZoneId` on Android, `Intl` in the browser).
//! A file that declares its recorder's offset from UTC in whole hours
//! (`HFTZN`) has that instead, as `Etc/GMT-2` for two hours east of
//! Greenwich: every platform knows those names, and a fixed offset has no
//! summer time. An offset of half an hour has no such name and is left to
//! the place's zone. The core's own calendar (a flight's year, the months
//! of the statistics) goes by the same zone, with the rules of `chrono-tz`.

use chrono::{DateTime, NaiveDateTime, Offset, TimeZone};

use crate::Coordinate;

/// The IANA time zone of a place on land, as `Europe/Vienna`; `None` at
/// sea away from a coast. Zones whose clocks have agreed since 1970 have
/// one name, the larger city's: Ljubljana is in `Europe/Belgrade`.
#[uniffi::export]
pub fn time_zone_at(lat: f64, lon: f64) -> Option<String> {
    venti_sources_timezones::zone_at(lon, lat).map(str::to_owned)
}

/// The zone a flight file declares for its recorder's clock, from its
/// `HFTZN` header: `Etc/GMT-2` for two hours east of Greenwich (the
/// names of the `Etc` zones have the sign the other way round). `None`
/// when the file declares no offset, one that is not a whole number of
/// hours, or zero: a recorder that writes `HFTZNTIMEZONE:0` by default
/// would show a flight in UTC, and where zero is right the place's zone
/// says the same (the owner's call, 2026-11-23).
pub(crate) fn declared_zone(igc_text: &str) -> Option<String> {
    let offset_s = venti_flight::igc::declared_utc_offset_s(igc_text)?;
    if offset_s == 0 || offset_s % 3600 != 0 {
        return None;
    }
    let hours = offset_s / 3600;
    Some(match hours {
        hours if hours > 0 => format!("Etc/GMT-{hours}"),
        hours => format!("Etc/GMT+{}", -hours),
    })
}

/// A flight's zone: the one its file declares, else that of where it took
/// off.
pub(crate) fn flight_zone(declared: Option<&str>, takeoff: Option<Coordinate>) -> Option<String> {
    declared
        .map(str::to_owned)
        .or_else(|| takeoff.and_then(|place| time_zone_at(place.lat, place.lon)))
}

/// What the clock in `zone` showed at a moment; the time in UTC where the
/// zone is `None` or unknown to the table of rules.
pub(crate) fn local_time(zone: Option<&str>, unix_s: i64) -> Option<NaiveDateTime> {
    let utc = DateTime::from_timestamp(unix_s, 0)?;
    let offset_s = zone
        .and_then(|zone| zone.parse::<chrono_tz::Tz>().ok())
        .map_or(0, |tz| {
            tz.offset_from_utc_datetime(&utc.naive_utc())
                .fix()
                .local_minus_utc()
        });
    Some(utc.naive_utc() + chrono::Duration::seconds(i64::from(offset_s)))
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, Timelike};

    use super::*;

    #[test]
    fn a_place_has_its_zone() {
        assert_eq!(
            time_zone_at(47.2692, 11.3933).as_deref(),
            Some("Europe/Vienna")
        );
        assert_eq!(
            time_zone_at(32.8899, -117.2516).as_deref(),
            Some("America/Los_Angeles")
        );
        assert_eq!(time_zone_at(40.0, -30.0), None);
    }

    #[test]
    fn a_declared_offset_in_whole_hours_is_a_zone_of_its_own() {
        let file = |header: &str| {
            format!(
                "AXCT001\r\nHFDTE181126\r\n{header}\r\nB1602405407121N00249342WA002800042120509950\r\n"
            )
        };
        assert_eq!(
            declared_zone(&file("HFTZNTIMEZONE:+2")).as_deref(),
            Some("Etc/GMT-2")
        );
        assert_eq!(
            declared_zone(&file("HFTZNTIMEZONE:-7")).as_deref(),
            Some("Etc/GMT+7")
        );
        assert_eq!(declared_zone(&file("HFTZNTIMEZONE:0")), None);
        assert_eq!(declared_zone(&file("HFTZNTIMEZONE:+5.5")), None);
        assert_eq!(declared_zone(&file("HFPLTPILOTINCHARGE:Jane")), None);
        // Every name it makes is one the table of rules knows.
        for hours in (-12..=14).filter(|hours| *hours != 0) {
            let header = format!("HFTZNTIMEZONE:{hours:+}");
            let zone = declared_zone(&file(&header)).unwrap();
            let tz: chrono_tz::Tz = zone.parse().unwrap();
            let offset = tz.offset_from_utc_datetime(&DateTime::UNIX_EPOCH.naive_utc());
            assert_eq!(offset.fix().local_minus_utc(), hours * 3600, "{zone}");
        }
    }

    #[test]
    fn the_declared_zone_wins_over_the_place() {
        let innsbruck = Some(Coordinate {
            lat: 47.2692,
            lon: 11.3933,
        });
        assert_eq!(
            flight_zone(Some("Etc/GMT-1"), innsbruck).as_deref(),
            Some("Etc/GMT-1")
        );
        assert_eq!(
            flight_zone(None, innsbruck).as_deref(),
            Some("Europe/Vienna")
        );
        assert_eq!(flight_zone(None, None), None);
    }

    #[test]
    fn the_local_time_follows_the_zone_s_summer_time() {
        // 2026-07-15 22:39 UTC is 15:39 that day in California, and
        // 2026-01-15 23:39 UTC is 15:39 too, an hour later in UTC.
        let summer = local_time(Some("America/Los_Angeles"), 1_784_155_140).unwrap();
        assert_eq!((summer.day(), summer.hour(), summer.minute()), (15, 15, 39));
        let winter = local_time(Some("America/Los_Angeles"), 1_768_520_340).unwrap();
        assert_eq!((winter.day(), winter.hour(), winter.minute()), (15, 15, 39));
        // At sea, or with a name the table does not know, the time in UTC.
        let utc = local_time(None, 1_768_520_340).unwrap();
        assert_eq!((utc.day(), utc.hour()), (15, 23));
        assert_eq!(local_time(Some("Nowhere/Else"), 1_768_520_340), Some(utc));
    }
}
