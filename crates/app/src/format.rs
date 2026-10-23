use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc, Weekday};

pub(crate) fn file_size(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = "B";
    for larger in ["K", "M", "G", "T", "P", "E"] {
        if value < 1023.95 {
            break;
        }
        value /= 1024.0;
        unit = larger;
    }
    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {unit}")
    }
}

pub(crate) fn file_time<Tz: TimeZone>(time: Option<SystemTime>, now: &DateTime<Tz>) -> String {
    let Some(time) = time.and_then(utc_time) else {
        return String::new();
    };
    let time = time.with_timezone(&now.timezone());
    let days_ago = now
        .date_naive()
        .signed_duration_since(time.date_naive())
        .num_days();
    let clock = format!(
        "{:02}:{:02}:{:02}",
        time.hour(),
        time.minute(),
        time.second()
    );
    match days_ago {
        0 => format!("Today {clock}"),
        1..=6 => format!("{} {clock}", weekday_name(time.weekday())),
        _ => format!("{:02}.{:02}.{:04}", time.day(), time.month(), time.year()),
    }
}

fn utc_time(time: SystemTime) -> Option<DateTime<Utc>> {
    let (seconds, nanoseconds) = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => (i64::try_from(after.as_secs()).ok()?, after.subsec_nanos()),
        Err(error) => {
            let before = error.duration();
            let seconds = i64::try_from(before.as_secs()).ok()?.checked_neg()?;
            match before.subsec_nanos() {
                0 => (seconds, 0),
                nanoseconds => (seconds.checked_sub(1)?, 1_000_000_000 - nanoseconds),
            }
        }
    };
    DateTime::from_timestamp(seconds, nanoseconds)
}

fn weekday_name(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}
