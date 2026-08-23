//! The two clock readings every surface shares.
//!
//! [`now_ms`] stamps events and head caches; [`rfc3339_utc`] renders the one
//! human time verification output carries (`contracts/README.md`,
//! "Timestamps"). Both live here so the CLI and the HTTP API cannot spell a
//! timestamp two ways.

/// Milliseconds since the unix epoch, saturating at 0 before it.
#[must_use]
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Unix milliseconds as RFC 3339 UTC.
///
/// Seconds are whole: the statement sentence carries no subsecond digits.
#[must_use]
pub fn rfc3339_utc(timestamp_ms: u64) -> String {
    let seconds = timestamp_ms / 1000;
    let (days, time) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (time / 3600, (time % 3600) / 60, time % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days since 1970-01-01 as a civil date, by Howard Hinnant's algorithm.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let days = days + 719_468;
    let era = days / 146_097;
    let day_of_era = days % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::rfc3339_utc;

    #[test]
    fn the_fixture_timestamps_render_as_the_fixture_statements_spell_them() {
        assert_eq!(rfc3339_utc(1_700_000_500_000), "2023-11-14T22:21:40Z");
        assert_eq!(rfc3339_utc(1_700_000_560_000), "2023-11-14T22:22:40Z");
        assert_eq!(rfc3339_utc(1_700_000_620_000), "2023-11-14T22:23:40Z");
    }

    #[test]
    fn the_epoch_a_leap_day_and_the_timestamp_cap_render() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(1_709_164_800_000), "2024-02-29T00:00:00Z");
        assert_eq!(
            rfc3339_utc(mabel_core::MAX_TIMESTAMP_MS),
            "2100-01-01T00:00:00Z"
        );
    }
}
