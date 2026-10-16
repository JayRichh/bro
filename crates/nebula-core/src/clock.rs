//! Wall-clock stamps, in the units the database and the on-disk caches
//! store them in.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch — the stamp every row's `created_at`
/// and `status_changed_at` carries, so the daemon's and the TUI's compare.
/// A clock set before 1970 reads as 0.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Whole seconds since the Unix epoch — the on-disk caches' stamp. A clock
/// set before 1970 reads as 0.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The local time zone's offset from UTC at `epoch_secs`, in seconds east
/// (NZDT is +46800): the rule in force at that instant, daylight saving
/// included. 0 when the system can't say.
#[cfg(windows)]
pub fn local_offset_secs_at(epoch_secs: i64) -> i32 {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{
        FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime,
    };
    /// Seconds from 1601-01-01 (FILETIME's epoch) to 1970-01-01.
    const EPOCH_GAP: i64 = 11_644_473_600;
    let ticks = (epoch_secs + EPOCH_GAP).max(0) as u64 * 10_000_000;
    let utc_ft = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: plain out-parameter calls on stack values.
    unsafe {
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        let mut local_ft: FILETIME = std::mem::zeroed();
        if FileTimeToSystemTime(&utc_ft, &mut utc) == 0
            || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0
            || SystemTimeToFileTime(&local, &mut local_ft) == 0
        {
            return 0;
        }
        let local_ticks = ((local_ft.dwHighDateTime as u64) << 32) | local_ft.dwLowDateTime as u64;
        ((local_ticks as i64 - ticks as i64) / 10_000_000) as i32
    }
}

#[cfg(unix)]
pub fn local_offset_secs_at(epoch_secs: i64) -> i32 {
    let t = epoch_secs as libc::time_t;
    // SAFETY: localtime_r writes only into `tm`.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return 0;
    }
    tm.tm_gmtoff as i32
}

/// [`local_offset_secs_at`] now.
pub fn local_offset_secs() -> i32 {
    local_offset_secs_at(now_secs() as i64)
}
