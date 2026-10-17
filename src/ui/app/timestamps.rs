use std::time::{SystemTime, UNIX_EPOCH};

const WINDOWS_EPOCH_TICKS: i128 = 116_444_736_000_000_000;

fn filetime_ticks(time: SystemTime) -> Option<u64> {
    let ticks = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i128::try_from(duration.as_nanos() / 100).ok()?,
        Err(error) => -i128::try_from(error.duration().as_nanos().div_ceil(100)).ok()?,
    };
    u64::try_from(WINDOWS_EPOCH_TICKS.checked_add(ticks)?).ok()
}

#[cfg(windows)]
fn local_parts(time: SystemTime) -> Option<[u16; 6]> {
    use windows_sys::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTimeEx},
    };
    let ticks = filetime_ticks(time)?;
    let filetime = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    // Use the machine's time-zone rules for the timestamp's date, including DST.
    if unsafe { FileTimeToSystemTime(&filetime, &mut utc) } == 0
        || unsafe { SystemTimeToTzSpecificLocalTimeEx(std::ptr::null(), &utc, &mut local) } == 0
    {
        return None;
    }
    Some([
        local.wYear,
        local.wMonth,
        local.wDay,
        local.wHour,
        local.wMinute,
        local.wSecond,
    ])
}

#[cfg(any(not(windows), test))]
fn utc_parts(time: SystemTime) -> Option<[u16; 6]> {
    let seconds = (i128::from(filetime_ticks(time)?) - WINDOWS_EPOCH_TICKS).div_euclid(10_000_000);
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    year += i128::from(month <= 2);
    let seconds_of_day = seconds.rem_euclid(86_400);
    Some([
        u16::try_from(year).ok()?,
        month as u16,
        day as u16,
        (seconds_of_day / 3600) as u16,
        (seconds_of_day / 60 % 60) as u16,
        (seconds_of_day % 60) as u16,
    ])
}

pub(super) fn format_modified(time: Option<SystemTime>) -> String {
    let Some(time) = time else {
        return "—".into();
    };
    #[cfg(windows)]
    let parts = local_parts(time);
    #[cfg(not(windows))]
    let parts = utc_parts(time);
    let Some([year, month, day, hour, minute, second]) = parts else {
        return "无法读取".into();
    };
    let text = format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}");
    if cfg!(windows) {
        text
    } else {
        format!("{text} UTC")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn timestamps_preserve_seconds_and_handle_leap_days_and_pre_unix_dates() {
        assert_eq!(utc_parts(UNIX_EPOCH), Some([1970, 1, 1, 0, 0, 0]));
        assert_eq!(
            utc_parts(UNIX_EPOCH + Duration::from_secs(951_782_445)),
            Some([2000, 2, 29, 0, 0, 45])
        );
        assert_eq!(
            utc_parts(UNIX_EPOCH - Duration::from_secs(1)),
            Some([1969, 12, 31, 23, 59, 59])
        );
        assert_eq!(format_modified(None), "—");
    }

    #[cfg(windows)]
    #[test]
    fn windows_formatter_outputs_full_local_date_and_time() {
        let text = format_modified(Some(UNIX_EPOCH + Duration::from_secs(45)));
        assert_eq!(text.len(), 19);
        assert_eq!(&text[4..5], "-");
        assert_eq!(&text[10..11], " ");
        assert_eq!(&text[16..17], ":");
        assert_eq!(&text[17..19], "45");
    }
}
