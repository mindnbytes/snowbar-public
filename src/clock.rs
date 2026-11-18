//! Local clock display: one wall-clock read per minute, on a local minute boundary.
//!
//! The side effect is deliberately tiny: [`sample`] reads `SystemTime::now()` once
//! and converts that instant to local time. Everything else — the wait to the next
//! boundary, the displayed `HH:MM`, and whether a fresh sample actually changes it —
//! is pure and tested without a clock or a compositor.
//!
//! Time is re-sampled on every wake, never counted or replayed from elapsed frames.
//! That makes forward and backward steps (NTP correction, resume, time-zone
//! changes) self-correcting: the display always comes from the latest absolute
//! sample. The only stale window is until the next scheduled wake: a forward jump,
//! resume, or live time-zone change is shown within about one minute of awake time
//! (the timer is not event-driven for those changes). Rune's `Tx::wake_after`
//! schedules the wake itself; no worker thread or polling loop is involved.
//!
//! The boundary is the local one, not UTC's: a POSIX zone may have a seconds
//! offset (`TZ=UTC-00:00:15`), so local minutes can start at UTC `:15` or `:45`.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const NANOS_PER_MINUTE: u64 = 60_000_000_000;
const SECONDS_PER_DAY: u64 = 86_400;

#[cfg(unix)]
unsafe extern "C" {
    /// POSIX `tzset`: reload the process time-zone rules. `libc` does not
    /// declare it for every Unix, so name the standard symbol directly.
    fn tzset();
}

/// A local wall-clock time at minute precision, in 24-hour form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockTime {
    hour: u8,
    minute: u8,
}

impl ClockTime {
    /// `None` outside the ranges a real local time can have.
    pub fn new(hour: u8, minute: u8) -> Option<Self> {
        (hour < 24 && minute < 60).then_some(Self { hour, minute })
    }

    /// The four zero-padded glyph indices the view draws, in reading order:
    /// `[H tens, H ones, M tens, M ones]`.
    pub fn digits(self) -> [u8; 4] {
        [
            self.hour / 10,
            self.hour % 10,
            self.minute / 10,
            self.minute % 10,
        ]
    }
}

impl fmt::Display for ClockTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

/// One observation: the local minute the view shows, plus that same instant's
/// local second and subsecond fraction. The second locates the next *local*
/// minute even when the zone's offset is not a whole number of minutes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub time: ClockTime,
    /// Local second. libc can report a leap second as 60; it is clamped when the
    /// wait is computed so the timer stays positive instead of spinning.
    pub second: u8,
    /// Subsecond fraction of the same instant, in nanoseconds.
    pub nanos: u32,
}

impl Sample {
    /// Time from this sample to the next local minute boundary strictly after it.
    ///
    /// Exactly on a boundary this returns a full minute: zero would poll the
    /// same minute again. A leap second (libc's 60) is treated as the end of
    /// second 59, so the wait is the remaining fraction of that second, within
    /// `(0, 1] s`, and then the next local minute is sampled normally.
    pub fn until_next_minute(self) -> Duration {
        let second = u64::from(self.second.min(59));
        let within_minute = second * 1_000_000_000 + u64::from(self.nanos.min(999_999_999));
        Duration::from_nanos(NANOS_PER_MINUTE - within_minute)
    }
}

/// The committed display and the app-clock deadline of its next sample.
///
/// `app_clock` is Rune's logical frame clock (the `t` argument of `poll`), the
/// same clock `Tx::wake_after` schedules on. No system time is read here.
#[derive(Clone, Debug, Default)]
pub struct Display {
    shown: Option<ClockTime>,
    due: Option<f64>,
}

/// What a fresh sample did, plus how long until the next one is due.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Advance {
    pub wait: Duration,
    pub changed: bool,
}

impl Display {
    /// Whether this poll should read the system clock. Frames caused by input,
    /// hover or IPC do no clock work unless the minute deadline has arrived.
    pub fn is_due(&self, app_clock: f64) -> bool {
        self.due.is_none_or(|due| app_clock >= due)
    }

    /// Record a sample taken at `app_clock` and schedule the next local boundary
    /// from that sample's own second. `changed` is true only when the displayed
    /// `HH:MM` differs, so a repeated sample does not re-run the view.
    pub fn advance(&mut self, app_clock: f64, sample: Sample) -> Advance {
        let wait = sample.until_next_minute();
        // Rune usually stores the wake as `clock + wait.as_secs_f64()` from this
        // same clock, but some deferred host paths schedule it from a later
        // `now`. `due` is therefore a due guard, not an exact instant: a late
        // wake re-samples and recomputes rather than trusting this value.
        self.due = Some(app_clock + wait.as_secs_f64());
        let changed = self.shown != Some(sample.time);
        self.shown = Some(sample.time);
        Advance { wait, changed }
    }

    /// The time currently committed to the view, if any sample has landed.
    pub fn shown(&self) -> Option<ClockTime> {
        self.shown
    }

    /// The app-clock deadline of the next sample.
    pub fn due(&self) -> Option<f64> {
        self.due
    }
}

/// Read the system wall clock and its current local time. Called only when
/// [`Display::is_due`] says so.
pub fn sample() -> Sample {
    let wall = SystemTime::now();
    let (time, second) = local_time(wall);
    let nanos = wall
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    Sample {
        time,
        second,
        nanos,
    }
}

/// Convert an absolute instant to local time.
///
/// On Unix this asks libc and first calls `tzset()`. Without it, `localtime_r`
/// keeps the rules it read first, so a long-running bar would show a stale zone
/// after `/etc/localtime` is retargeted (as NixOS does on a system time-zone
/// change). With `TZ` unset — NixOS's default — glibc's `tzset()` re-reads
/// `/etc/localtime`; if `TZ` names a file that is retargeted in place, glibc
/// may still keep its cache until restart, a libc limit this slice does not
/// work around. The re-read is a few KiB at most once a minute. `tzset()` is
/// process-global and thread-safe in glibc; this is the only place Snowbar asks
/// for local time.
pub fn local_time(wall: SystemTime) -> (ClockTime, u8) {
    let secs = wall
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());

    #[cfg(unix)]
    {
        let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
        // SAFETY: `tm` points to writable storage of the right type; `time` is a
        // valid `time_t`; libc only reads `time` and writes `tm`. `tzset` reads
        // process environment and time-zone files and writes libc's own cache.
        unsafe {
            tzset();
            if let Ok(time) = libc::time_t::try_from(secs)
                && !libc::localtime_r(&time, tm.as_mut_ptr()).is_null()
            {
                let tm = tm.assume_init();
                if let (Ok(hour), Ok(minute), Ok(second)) = (
                    u8::try_from(tm.tm_hour),
                    u8::try_from(tm.tm_min),
                    u8::try_from(tm.tm_sec),
                ) && let Some(time) = ClockTime::new(hour, minute)
                    && second <= 60
                {
                    // 60 is a leap second; the schedule treats it as the end of
                    // second 59 so it never asks for a zero or negative wait.
                    return (time, second);
                }
            }
        }
    }

    utc_time(secs)
}

/// Fallback when a local lookup is unavailable or out of range. This is UTC, not
/// a guess at the local zone, and does not affect the normal Unix path.
fn utc_time(secs: u64) -> (ClockTime, u8) {
    let seconds_of_day = secs % SECONDS_PER_DAY;
    (
        ClockTime {
            hour: (seconds_of_day / 3_600) as u8,
            minute: ((seconds_of_day % 3_600) / 60) as u8,
        },
        (seconds_of_day % 60) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic observation. The local second and fraction drive the wait,
    /// just as libc's local fields would in production.
    fn at(hour: u8, minute: u8, second: u8, nanos: u32) -> Sample {
        Sample {
            time: ClockTime::new(hour, minute).expect("valid sample"),
            second,
            nanos,
        }
    }

    #[test]
    fn times_are_zero_padded_24_hour_and_map_to_four_glyphs() {
        let cases = [
            (ClockTime::new(0, 0).unwrap(), "00:00", [0, 0, 0, 0]),
            (ClockTime::new(9, 5).unwrap(), "09:05", [0, 9, 0, 5]),
            (ClockTime::new(9, 41).unwrap(), "09:41", [0, 9, 4, 1]),
            (ClockTime::new(23, 59).unwrap(), "23:59", [2, 3, 5, 9]),
        ];
        for (time, text, digits) in cases {
            assert_eq!(time.to_string(), text);
            assert_eq!(time.digits(), digits);
            assert_eq!(
                ClockTime::new(digits[0] * 10 + digits[1], digits[2] * 10 + digits[3]),
                Some(time)
            );
        }
        assert_eq!(ClockTime::new(24, 0), None);
        assert_eq!(ClockTime::new(0, 60), None);
    }

    #[test]
    fn next_local_minute_is_exact_for_fractional_exact_and_offset_times() {
        // Half a minute into the local minute.
        assert_eq!(
            at(9, 5, 30, 250_000_000).until_next_minute(),
            Duration::from_millis(29_750)
        );
        // Half a second before the boundary.
        assert_eq!(
            at(9, 5, 59, 500_000_000).until_next_minute(),
            Duration::from_millis(500)
        );
        // Exactly on a local boundary: the next one is a minute away, never zero.
        assert_eq!(at(9, 5, 0, 0).until_next_minute(), Duration::from_secs(60));
        assert_eq!(
            at(9, 5, 0, 1).until_next_minute(),
            Duration::from_nanos(NANOS_PER_MINUTE - 1)
        );
        assert_eq!(
            at(9, 5, 59, 999_999_999).until_next_minute(),
            Duration::from_nanos(1)
        );
        // A POSIX zone offset with seconds: UTC :30 is local :45 in
        // `TZ=UTC-00:00:15`, so only 15 s remain to the next *local* minute.
        // Aligning to UTC whole minutes would wrongly wait 30 s.
        assert_eq!(
            at(10, 30, 45, 0).until_next_minute(),
            Duration::from_secs(15)
        );
        // libc reports a leap second as 60. Treat it as the end of second 59:
        // the wait is the rest of that second, never zero.
        assert_eq!(
            at(23, 59, 60, 0).until_next_minute(),
            Duration::from_secs(1)
        );
        assert_eq!(
            at(23, 59, 60, 750_000_000).until_next_minute(),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn display_is_due_initially_and_only_at_its_deadline() {
        let mut display = Display::default();
        assert!(display.is_due(0.0), "the first poll must sample");
        assert_eq!(display.shown(), None);

        let advance = display.advance(0.0, at(9, 5, 0, 0));
        assert!(advance.changed);
        assert_eq!(advance.wait, Duration::from_secs(60));
        assert_eq!(display.due(), Some(60.0));
        assert_eq!(display.shown(), Some(ClockTime::new(9, 5).unwrap()));
        assert!(!display.is_due(59.999));
        assert!(display.is_due(60.0));
    }

    #[test]
    fn fractional_wait_is_scheduled_from_the_sampled_instant() {
        let mut display = Display::default();
        let advance = display.advance(10.0, at(9, 5, 30, 250_000_000));
        assert_eq!(advance.wait, Duration::from_millis(29_750));
        assert_eq!(display.due(), Some(39.75));
        assert!(!display.is_due(39.749));
        assert!(display.is_due(39.75));
    }

    #[test]
    fn an_unchanged_minute_still_reschedules_but_does_not_change_the_display() {
        let mut display = Display::default();
        display.advance(0.0, at(9, 5, 0, 0));
        let advance = display.advance(60.0, at(9, 5, 0, 0));
        assert!(!advance.changed, "same HH:MM must not re-run the view");
        assert_eq!(display.due(), Some(120.0));
        assert_eq!(display.shown(), Some(ClockTime::new(9, 5).unwrap()));
    }

    #[test]
    fn midnight_replaces_all_four_digits_with_zero_padded_zeros() {
        let mut display = Display::default();
        display.advance(0.0, at(23, 59, 0, 0));
        assert_eq!(display.shown().unwrap().to_string(), "23:59");
        let advance = display.advance(60.0, at(0, 0, 0, 0));
        assert!(advance.changed);
        assert_eq!(display.shown().unwrap().to_string(), "00:00");
        assert_eq!(display.shown().unwrap().digits(), [0, 0, 0, 0]);
        assert_eq!(display.due(), Some(120.0));
    }

    #[test]
    fn forward_backward_and_time_zone_samples_all_show_their_own_value() {
        let mut display = Display::default();
        display.advance(0.0, at(9, 5, 0, 0));

        // Same wall instant, different local time: a time-zone change.
        let zoned = display.advance(60.0, at(10, 6, 0, 0));
        assert!(zoned.changed);
        assert_eq!(display.shown(), Some(ClockTime::new(10, 6).unwrap()));

        // A forward jump between two wakes.
        assert!(display.advance(120.0, at(10, 43, 0, 0)).changed);
        assert_eq!(display.shown(), Some(ClockTime::new(10, 43).unwrap()));

        // A backward jump, as after an NTP correction.
        assert!(display.advance(180.0, at(10, 3, 0, 0)).changed);
        assert_eq!(display.shown(), Some(ClockTime::new(10, 3).unwrap()));

        // Resume-like: a long gap is not replayed minute by minute.
        assert!(display.advance(240.0, at(13, 0, 0, 0)).changed);
        assert_eq!(display.shown(), Some(ClockTime::new(13, 0).unwrap()));
    }

    /// Child process for [`local_time_follows_a_posix_seconds_offset`]. `TZ` is
    /// fixed before this process starts, so no multithreaded process has its
    /// environment mutated and no system file is touched.
    #[cfg(unix)]
    #[test]
    #[ignore = "spawned with a fixed TZ by local_time_follows_a_posix_seconds_offset"]
    fn posix_seconds_offset_child() {
        if std::env::var("TZ").as_deref() != Ok("UTC-00:00:15") {
            // Only meaningful under the parent's fixed environment; running it
            // by hand with another zone must not fail the suite.
            return;
        }
        let wall = UNIX_EPOCH + Duration::from_secs(30);
        let (time, second) = local_time(wall);
        assert_eq!(time.to_string(), "00:00");
        assert_eq!(second, 45, "UTC :30 is local :45 in TZ=UTC-00:00:15");
        let sample = Sample {
            time,
            second,
            nanos: 0,
        };
        assert_eq!(sample.until_next_minute(), Duration::from_secs(15));
    }

    #[cfg(unix)]
    #[test]
    fn local_time_follows_a_posix_seconds_offset() {
        let exe = std::env::current_exe().expect("test executable path");
        let output = std::process::Command::new(exe)
            .args([
                "--ignored",
                "--exact",
                "clock::tests::posix_seconds_offset_child",
                "--nocapture",
            ])
            .env("TZ", "UTC-00:00:15")
            .output()
            .expect("run the local-time child test");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "child failed (status {}):\n{}\n{}",
            output.status,
            stdout,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("1 passed"),
            "the child filter matched no test:\n{stdout}"
        );
    }
}
