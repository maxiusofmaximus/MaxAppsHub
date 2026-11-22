//! Looks for newer releases in the background and tells the user once per version,
//! with a Windows notification. Nothing is installed without the user's click.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use crate::i18n::tr;
use crate::{github, install, registry, self_update, shell};

/// How often the Hub asks GitHub while it runs.
pub const CHECK_EVERY: Duration = Duration::from_secs(2 * 60 * 60);

/// A release installs by itself only after this long since it was published, so a bad or
/// compromised release has time to be reported and pulled before it reaches this PC.
pub const AUTO_UPDATE_DELAY_SECS: u64 = 7 * 24 * 60 * 60;

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_secs())
}

/// True when the user allows automatic updates and this release has waited long enough.
fn due(installer: &github::Installer) -> bool {
    shell::auto_update_on() && github::older_than(&installer.published_at, now_secs(), AUTO_UPDATE_DELAY_SECS)
}

fn seen_file() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("MaxAppsHub").join("notified.txt"))
}

/// One "name version" line per release already announced.
fn seen() -> HashSet<String> {
    seen_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn remember(seen: &HashSet<String>) {
    if let Some(path) = seen_file() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut lines: Vec<&String> = seen.iter().collect();
        lines.sort();
        let text = lines.iter().map(|line| line.as_str()).collect::<Vec<_>>().join("\n");
        let _ = std::fs::write(path, text);
    }
}

fn toast(name: &str, version: &str) {
    // Plain toast with no activation handlers: the version with handlers did not show on screen.
    let _ = tauri_winrt_notification::Toast::new(shell::APP_ID)
        .title(tr("Actualización disponible", "Update available"))
        .text1(&format!("{name} {version} {}", tr("ya está publicada.", "is out.")))
        .text2(tr("Ábrelo en MaxApps Hub para actualizar.", "Open MaxApps Hub to update."))
        .duration(tauri_winrt_notification::Duration::Short)
        .show();
}

fn toast_updated(name: &str, version: &str) {
    let _ = tauri_winrt_notification::Toast::new(shell::APP_ID)
        .title(tr("Actualización instalada", "Update installed"))
        .text1(&format!("{name} {} {version}.", tr("se actualizó a", "was updated to")))
        .text2(tr("Se instaló una semana después de publicarse.", "Installed a week after it was published."))
        .duration(tauri_winrt_notification::Duration::Short)
        .show();
}

/// Checks GitHub now. Releases that have waited out the delay install by themselves when the
/// user allows it; the rest are announced once. `first` is the check right after start: only
/// then may the Hub restart itself to update, because nothing is open to interrupt.
/// Blocking: call it off the UI thread.
pub fn check_and_announce(first: bool) {
    let mut seen = seen();
    let mut changed = false;
    for app in registry::APPS {
        let Some(installed) = install::installed(app) else { continue };
        let Ok(latest) = github::latest_installer(app) else { continue };
        if !github::newer(&latest.version, &installed.version) {
            continue;
        }
        let key = format!("{} {}", app.name, latest.version);
        if due(&latest) && install::install(app, &latest, &install::Progress::default()).is_ok() {
            toast_updated(app.name, &latest.version);
            seen.insert(key);
            changed = true;
        } else if seen.insert(key) {
            toast(app.name, &latest.version);
            changed = true;
        }
    }
    if let Some(update) = self_update::available() {
        let key = format!("{} {}", registry::SELF_APP.name, update.version);
        if first && due(&update) && self_update::apply(&update).is_ok() {
            remember(&seen);
            std::process::exit(0);
        }
        if seen.insert(key) {
            toast(registry::SELF_APP.name, &update.version);
            changed = true;
        }
    }
    if changed {
        remember(&seen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_is_announced_once_and_remembered_as_a_line() {
        let mut seen: HashSet<String> = HashSet::new();
        assert!(seen.insert("Excubia 2.3.911".to_string()));
        assert!(!seen.insert("Excubia 2.3.911".to_string()));
        assert!(seen.insert("Excubia 2.3.912".to_string()));
    }
}
