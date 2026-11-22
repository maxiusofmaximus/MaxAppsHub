//! A plain record of what the Hub did, kept in its data folder so it can be sent
//! when something fails. Nothing leaves the PC.

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Past this size the file starts again. shortcut: one file, no rotation; keep the last
/// run's history only, upgrade to rotated files if a bug report needs more.
const MAX_BYTES: u64 = 512 * 1024;

pub fn path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("MaxAppsHub").join("hub.log"))
}

/// Appends one line: the UTC time, then the event. Failures are ignored, since a log
/// that cannot be written must never stop an install.
pub fn record(event: &str) {
    let Some(path) = path() else { return };
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    append_at(&path, &stamp(seconds), event);
}

fn append_at(path: &std::path::Path, time: &str, event: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(path).is_ok_and(|meta| meta.len() > MAX_BYTES) {
        let _ = std::fs::remove_file(path);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{time} {event}");
    }
}

/// `YYYY-MM-DD HH:MM:SS` in UTC, from Unix seconds (proleptic Gregorian calendar).
fn stamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        day_seconds / 3_600,
        day_seconds % 3_600 / 60,
        day_seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_is_written_in_utc() {
        assert_eq!(stamp(0), "1970-01-01 00:00:00");
        assert_eq!(stamp(1_700_000_000), "2023-11-14 22:13:20");
        assert_eq!(stamp(951_782_400), "2000-02-29 00:00:00");
    }

    #[test]
    fn each_event_is_one_line_and_a_full_file_starts_again() {
        let dir = std::env::temp_dir().join(format!("maxapps-log-{}", std::process::id()));
        let file = dir.join("hub.log");
        let _ = std::fs::remove_dir_all(&dir);
        append_at(&file, "t1", "primero");
        append_at(&file, "t2", "segundo");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "t1 primero\nt2 segundo\n");
        // Past the limit the old lines go, so the file never grows without bound.
        std::fs::write(&file, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        append_at(&file, "t3", "nuevo");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "t3 nuevo\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
