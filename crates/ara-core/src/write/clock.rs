//! One UTC clock value per guarded write.
//!
//! The writer reads its clock once, after it holds the artifact lock and has
//! recovered any prepared transaction, and passes that value to every planner
//! as `batch_time`. Planners never read the clock themselves. The shipped
//! binary passes [`system_utc`]; tests pass a fixed function.
use super::WriteError;

/// Format whole Unix seconds as `YYYY-MM-DDTHH:MM:SSZ` (UTC).
pub fn format_unix(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        second_of_day / 3600,
        second_of_day / 60 % 60,
        second_of_day % 60
    )
}

/// The host's current UTC time in the native default format. A host clock
/// before the Unix epoch is an I/O-class error, never a silent 1970 value.
pub fn system_utc() -> Result<String, WriteError> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| WriteError::io("system clock is before the Unix epoch"))?;
    let seconds = i64::try_from(elapsed.as_secs())
        .map_err(|_| WriteError::io("system clock is out of range"))?;
    Ok(format_unix(seconds))
}

/// A captured batch time must be an explicit UTC `YYYY-MM-DDTHH:MM:SSZ`
/// instant that the timestamp grammar accepts.
pub fn validate_batch_time(value: &str) -> Result<(), WriteError> {
    let bytes = value.as_bytes();
    if bytes.len() != 20 || bytes[16] != b':' || bytes[19] != b'Z' {
        return Err(WriteError::io(format!(
            "writer clock returned `{value}`, expected UTC YYYY-MM-DDTHH:MM:SSZ"
        )));
    }
    super::sessions::validate_timestamp(value).map_err(|error| {
        WriteError::io(format!(
            "writer clock returned `{value}`: {}",
            error.message
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_epoch_leap_day_and_midnight() {
        assert_eq!(format_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_unix(1_791_158_399), "2026-10-04T23:59:59Z");
        assert_eq!(format_unix(1_791_158_400), "2026-10-05T00:00:00Z");
        assert_eq!(format_unix(-1), "1969-12-31T23:59:59Z");
    }
    #[test]
    fn batch_time_requires_explicit_utc_seconds() {
        validate_batch_time("2026-10-05T00:00:00Z").unwrap();
        for bad in [
            "2026-10-05T00:00Z",
            "2026-10-05T00:00:00+00:00",
            "2026-02-30T00:00:00Z",
            "2026-10-05T24:00:00Z",
            "garbage",
        ] {
            assert_eq!(validate_batch_time(bad).unwrap_err().exit_code(), 2);
        }
    }
}
