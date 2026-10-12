use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn format_rfc3339(seconds: i64) -> String {
    let (year, month, day, hour, minute, second) = utc_parts_from_unix_seconds(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

pub(crate) fn format_compact(seconds: i64) -> String {
    let (year, month, day, hour, minute, second) = utc_parts_from_unix_seconds(seconds);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

fn utc_parts_from_unix_seconds(seconds: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let time_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = (time_of_day / 3600) as u32;
    let minute = ((time_of_day % 3600) / 60) as u32;
    let second = (time_of_day % 60) as u32;
    (year, month, day, hour, minute, second)
}

/// Howard Hinnant's `civil_from_days`: http://howardhinnant.github.io/date_algorithms.html
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_epoch_formats_as_rfc3339() {
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn known_timestamp_formats_as_rfc3339() {
        assert_eq!(format_rfc3339(1_756_000_000), "2025-08-24T01:46:40Z");
    }

    #[test]
    fn known_timestamp_formats_compactly() {
        assert_eq!(format_compact(1_756_000_000), "20250824-014640");
    }

    #[test]
    fn leap_day_round_trips() {
        assert_eq!(format_rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    }
}
