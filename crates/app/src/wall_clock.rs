//! The time of day in the MASTER pill: "21:48" or "9:48 PM", following the
//! system's 12/24-hour setting where it can be read.

use chrono::{NaiveTime, Timelike};

/// The time as the pill shows it.
pub fn format(time: NaiveTime, twenty_four: bool) -> String {
    let (h, m) = (time.hour(), time.minute());
    if twenty_four {
        format!("{h:02}:{m:02}")
    } else {
        let hour = match h % 12 {
            0 => 12,
            h => h,
        };
        let half = if h < 12 { "AM" } else { "PM" };
        format!("{hour}:{m:02} {half}")
    }
}

/// Whether the system shows 24-hour time. Read once at start.
pub fn system_uses_24_hour() -> bool {
    system_setting().unwrap_or_else(|| region_uses_24_hour(&locale()))
}

/// An explicit 12/24-hour choice in the system settings, if any.
#[cfg(target_os = "macos")]
fn system_setting() -> Option<bool> {
    // Set by System Settings › General › Date & Time › 24-hour time when it
    // differs from the region's default.
    if defaults_flag("AppleICUForce24HourTime") {
        return Some(true);
    }
    if defaults_flag("AppleICUForce12HourTime") {
        return Some(false);
    }
    None
}

#[cfg(target_os = "macos")]
fn defaults_flag(key: &str) -> bool {
    defaults_read(key).is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

#[cfg(target_os = "macos")]
fn defaults_read(key: &str) -> Option<String> {
    let out = std::process::Command::new("defaults")
        .args(["read", "-g", key])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(windows)]
fn system_setting() -> Option<bool> {
    // The short time pattern, e.g. "HH:mm" or "h:mm tt".
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Control Panel\International",
            "/v",
            "sShortTime",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let pattern = text.lines().find(|l| l.contains("sShortTime"))?;
    let pattern = pattern.split_whitespace().last()?;
    Some(pattern.contains('H'))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn system_setting() -> Option<bool> {
    None
}

/// The user's locale, such as "en_US".
fn locale() -> String {
    #[cfg(target_os = "macos")]
    if let Some(l) = defaults_read("AppleLocale") {
        return l;
    }
    ["LC_ALL", "LC_TIME", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .unwrap_or_default()
}

/// Whether a locale's region writes the time with 24 hours. Regions that
/// mostly use 12-hour time say no; everything else (and no region) says yes.
fn region_uses_24_hour(locale: &str) -> bool {
    const TWELVE_HOUR: &[&str] = &[
        "US", "CA", "AU", "NZ", "IN", "PH", "PK", "BD", "EG", "SA", "MY", "CO", "MX", "SV", "HN",
        "NI", "VE",
    ];
    let base = locale.split(['.', '@']).next().unwrap_or("");
    let region = base.split(['_', '-']).nth(1).unwrap_or("");
    !TWELVE_HOUR.contains(&region)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn twenty_four_hour() {
        assert_eq!(format(at(21, 48), true), "21:48");
        assert_eq!(format(at(0, 5), true), "00:05");
    }

    #[test]
    fn twelve_hour() {
        assert_eq!(format(at(21, 48), false), "9:48 PM");
        assert_eq!(format(at(0, 5), false), "12:05 AM");
        assert_eq!(format(at(12, 0), false), "12:00 PM");
        assert_eq!(format(at(9, 7), false), "9:07 AM");
    }

    #[test]
    fn regions() {
        assert!(!region_uses_24_hour("en_US.UTF-8"));
        assert!(!region_uses_24_hour("en-AU"));
        assert!(region_uses_24_hour("en_GB.UTF-8"));
        assert!(region_uses_24_hour("es_AR"));
        assert!(region_uses_24_hour("de_DE@euro"));
        assert!(region_uses_24_hour(""));
    }
}
