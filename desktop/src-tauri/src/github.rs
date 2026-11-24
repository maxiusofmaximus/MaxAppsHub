//! Finds the newest stable desktop release of an app and its installer.

use serde_json::Value;

use crate::i18n::tr;
use crate::registry::DesktopApp;

/// The installer of the newest stable desktop release.
#[derive(Debug, PartialEq, Clone)]
pub struct Installer {
    pub version: String,
    pub file_name: String,
    pub download_url: String,
    /// `sha256:<hex>`, as GitHub records it for the asset.
    pub digest: String,
    pub size: u64,
    /// When the release was published, as GitHub writes it (`2026-11-22T10:00:00Z`).
    pub published_at: String,
    /// The `<installer>.sig` asset: the publisher's Ed25519 signature in hex, when there is one.
    pub signature_url: Option<String>,
    /// The release notes the publisher wrote (Markdown, shown as plain text).
    pub notes: String,
}

/// Unix seconds for a GitHub timestamp (`YYYY-MM-DDTHH:MM:SSZ`), or None if it is not one.
fn unix_seconds(stamp: &str) -> Option<u64> {
    let field = |from: usize, to: usize| stamp.get(from..to)?.parse::<i64>().ok();
    let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    // Days since 1970-01-01 (proleptic Gregorian calendar).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    u64::try_from(days * 86_400 + hour * 3_600 + minute * 60 + second).ok()
}

/// True when the release was published at least `delay_secs` before `now` (unix seconds).
/// A timestamp we cannot read counts as too new, so nothing installs on a guess.
pub fn older_than(published_at: &str, now: u64, delay_secs: u64) -> bool {
    unix_seconds(published_at).is_some_and(|published| now >= published.saturating_add(delay_secs))
}

/// The GitHub API root. Debug builds can point it at a local stand-in for tests.
fn api_base() -> String {
    #[cfg(debug_assertions)]
    if let Ok(base) = std::env::var("MAXAPPS_GITHUB_API") {
        return base;
    }
    "https://api.github.com".into()
}

/// True when `latest` is a higher version than `current`, using semver order:
/// 1.0.1 > 1.0.1-alpha.2 > 1.0.1-alpha.1 > 1.0.0.
pub fn newer(latest: &str, current: &str) -> bool {
    compare(latest, current) == std::cmp::Ordering::Greater
}

fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let split = |v: &str| -> (Vec<u64>, Option<String>) {
        let (core, pre) = match v.split_once('-') {
            Some((core, pre)) => (core, Some(pre.to_string())),
            None => (v, None),
        };
        (core.split('.').map(|p| p.parse().unwrap_or(0)).collect(), pre)
    };
    let (core_a, pre_a) = split(a);
    let (core_b, pre_b) = split(b);
    match core_a.cmp(&core_b) {
        Ordering::Equal => {}
        other => return other,
    }
    match (pre_a, pre_b) {
        (None, None) => Ordering::Equal,
        // A release without a pre-release tag is newer than its own alphas.
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => {
            let ids = |s: &str| -> Vec<String> { s.split('.').map(str::to_string).collect() };
            let (x, y) = (ids(&x), ids(&y));
            for (p, q) in x.iter().zip(y.iter()) {
                let order = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    _ => p.cmp(q),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
            x.len().cmp(&y.len())
        }
    }
}

/// Why no installer was picked: only alphas exist while the alpha channel is off, or a real failure.
#[derive(Debug, PartialEq)]
pub enum NoInstaller {
    OnlyAlpha,
    Failed(String),
}

impl std::fmt::Display for NoInstaller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoInstaller::OnlyAlpha => f.write_str(tr(
                "Solo hay versión alfa: actívala en Ajustes › Avanzado",
                "Only an alpha version: turn it on in Settings › Advanced",
            )),
            NoInstaller::Failed(text) => f.write_str(text),
        }
    }
}

/// The release asset that is this app's installer, if the release has one.
fn installer_asset<'a>(release: &'a Value, app: &DesktopApp) -> Option<&'a Value> {
    release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|asset| asset["name"].as_str().unwrap_or_default().ends_with(app.asset_suffix))
}

/// Picks the newest release of the app with an installer. Pre-releases (alphas) are
/// only considered when `include_prerelease` is set. GitHub lists releases newest
/// first, so the first match wins.
pub fn choose(app: &DesktopApp, releases: &Value, include_prerelease: bool) -> Result<Installer, NoInstaller> {
    let releases = releases.as_array().ok_or_else(|| {
        NoInstaller::Failed(tr("GitHub no devolvió una lista de versiones", "GitHub didn't return a list of versions").into())
    })?;
    let mut alpha_hidden = false;
    for release in releases {
        let tag = release["tag_name"].as_str().unwrap_or_default();
        if release["draft"].as_bool().unwrap_or(true) || !tag.starts_with(app.tag_prefix) {
            continue;
        }
        if release["prerelease"].as_bool().unwrap_or(true) && !include_prerelease {
            alpha_hidden |= installer_asset(release, app).is_some();
            continue;
        }
        let Some(asset) = installer_asset(release, app) else { continue };
        let digest = asset["digest"].as_str().unwrap_or_default();
        if !digest.starts_with("sha256:") {
            return Err(NoInstaller::Failed(
                tr(
                    "GitHub no dio la huella del instalador; no lo instalo sin comprobarla",
                    "GitHub gave no fingerprint for the installer; I won't install it unchecked",
                )
                .into(),
            ));
        }
        let file_name = asset["name"].as_str().unwrap_or_default();
        let signature_name = format!("{file_name}.sig");
        let signature_url = release["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|asset| asset["name"].as_str() == Some(signature_name.as_str()))
            .and_then(|asset| asset["browser_download_url"].as_str())
            .map(str::to_string);
        return Ok(Installer {
            version: tag[app.tag_prefix.len()..].to_string(),
            file_name: file_name.to_string(),
            download_url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
            digest: digest.to_string(),
            size: asset["size"].as_u64().unwrap_or(0),
            published_at: release["published_at"].as_str().unwrap_or_default().to_string(),
            signature_url,
            notes: release["body"].as_str().unwrap_or_default().to_string(),
        });
    }
    if alpha_hidden {
        return Err(NoInstaller::OnlyAlpha);
    }
    let kind = if include_prerelease { tr("publicada", "published") } else { tr("estable", "stable") };
    Err(NoInstaller::Failed(format!(
        "{} {kind} {} {}",
        tr("No encontré una versión", "I found no"),
        tr("de", "version of"),
        app.name
    )))
}

/// Fetches the release list and picks the installer, alphas included only when the
/// user turned that channel on. Blocking: call it off the UI thread.
pub fn latest_installer(app: &DesktopApp) -> Result<Installer, NoInstaller> {
    let url = format!(
        "{}/repos/{}/{}/releases?per_page=30",
        api_base(), app.owner, app.repo
    );
    let releases: Value = ureq::get(&url)
        .set("User-Agent", "maxapps-hub-desktop")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|_| {
            NoInstaller::Failed(tr("No pude consultar GitHub. Revisa la conexión.", "I couldn't reach GitHub. Check the connection.").into())
        })?
        .into_json()
        .map_err(|_| NoInstaller::Failed(tr("GitHub devolvió una respuesta que no entiendo", "GitHub sent a reply I can't read").into()))?;
    choose(app, &releases, crate::shell::alpha_channel_on() || crate::shell::alpha_app_on(app.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::APPS;
    use serde_json::json;

    fn excubia() -> &'static DesktopApp {
        &APPS[0]
    }

    fn release(tag: &str, prerelease: bool, asset: &str) -> Value {
        json!({
            "tag_name": tag,
            "draft": false,
            "prerelease": prerelease,
            "assets": [{
                "name": asset,
                "browser_download_url": format!("https://example.test/{asset}"),
                "digest": "sha256:abc"
            }]
        })
    }

    #[test]
    fn a_prerelease_is_skipped_unless_the_alpha_channel_is_on() {
        let releases = json!([
            release("desktop-v2.3.910-alpha.14", true, "Excubia-2.3.910-alpha.14-setup.exe"),
            release("desktop-v2.3.910", false, "Excubia-2.3.910-setup.exe"),
        ]);
        let installer = choose(excubia(), &releases, false).expect("installer");
        assert_eq!(installer.version, "2.3.910");
        assert!(installer.download_url.ends_with("Excubia-2.3.910-setup.exe"));
    }

    #[test]
    fn with_the_alpha_channel_on_the_newest_alpha_is_offered() {
        let releases = json!([
            release("desktop-v2.3.911-alpha.2", true, "Excubia-2.3.911-alpha.2-setup.exe"),
            release("desktop-v2.3.910", false, "Excubia-2.3.910-setup.exe"),
        ]);
        let installer = choose(excubia(), &releases, true).expect("installer");
        assert_eq!(installer.version, "2.3.911-alpha.2");
    }

    #[test]
    fn alphas_order_before_their_release_and_by_number() {
        assert!(newer("1.0.1", "1.0.1-alpha.2"));
        assert!(newer("1.0.1-alpha.2", "1.0.1-alpha.1"));
        assert!(newer("1.0.1-alpha.10", "1.0.1-alpha.9"));
        assert!(newer("1.0.1-alpha.1", "1.0.0"));
        assert!(!newer("1.0.1-alpha.1", "1.0.1"));
        assert!(!newer("1.0.1-alpha.1", "1.0.1-alpha.1"));
    }

    #[test]
    fn an_app_with_only_alphas_says_so_instead_of_failing() {
        let alpha = json!([release("desktop-v2.3.911-alpha.2", true, "Excubia-2.3.911-alpha.2-setup.exe")]);
        assert_eq!(choose(excubia(), &alpha, false).unwrap_err(), NoInstaller::OnlyAlpha);
        assert!(choose(excubia(), &alpha, true).is_ok());
        // No release at all is a real failure, not the alpha message.
        assert!(matches!(choose(excubia(), &json!([]), false), Err(NoInstaller::Failed(_))));
    }

    #[test]
    fn releases_of_other_apps_are_skipped() {
        let releases = json!([release("v2.3.4", false, "app-release.apk")]);
        assert!(choose(excubia(), &releases, false).is_err());
    }

    #[test]
    fn a_version_is_newer_only_when_its_numbers_are_higher() {
        assert!(newer("1.0.11", "1.0.7"));
        assert!(newer("2.0.0", "1.9.9"));
        assert!(!newer("1.0.0", "1.0.0"));
        assert!(!newer("1.0.0", "1.0.1"));
    }

    /// Live check against GitHub: run with `cargo test live_ -- --ignored`, with LOCALAPPDATA
    /// pointing at a folder whose settings.json turns the alpha channel on.
    #[test]
    #[ignore]
    fn live_the_alpha_channel_finds_the_published_alpha_and_its_digest_matches() {
        let installer = crate::github::latest_installer(&crate::registry::SELF_APP).expect("live release");
        assert_eq!(installer.version, "1.0.1-alpha.1");
        let bytes = crate::install::download(&installer.download_url).expect("download");
        assert!(crate::install::digest_matches(&installer.digest, &bytes));
    }

    #[test]
    fn a_release_waits_out_the_delay_before_it_installs_itself() {
        // 2000-01-01T00:00:00Z is 946684800 seconds after 1970.
        assert!(older_than("2000-01-01T00:00:00Z", 946_684_800 + 7 * 86_400, 7 * 86_400));
        assert!(!older_than("2000-01-01T00:00:00Z", 946_684_800 + 7 * 86_400 - 1, 7 * 86_400));
        assert!(!older_than("no es una fecha", u64::MAX, 0));
        assert!(!older_than("", u64::MAX, 0));
    }

    #[test]
    fn the_signature_and_the_notes_of_a_release_are_taken_with_its_installer() {
        let mut signed = release("desktop-v1.0.2", false, "MaxAppsHub-1.0.2-desktop.exe");
        signed["body"] = json!("Corrige la cola.");
        signed["assets"].as_array_mut().unwrap().push(json!({
            "name": "MaxAppsHub-1.0.2-desktop.exe.sig",
            "browser_download_url": "https://example.test/firma.sig",
            "digest": "sha256:abc"
        }));
        let hub = &crate::registry::SELF_APP;
        let installer = choose(hub, &json!([signed]), false).expect("installer");
        assert_eq!(installer.signature_url.as_deref(), Some("https://example.test/firma.sig"));
        assert_eq!(installer.notes, "Corrige la cola.");

        let unsigned = release("desktop-v1.0.2", false, "MaxAppsHub-1.0.2-desktop.exe");
        assert!(choose(hub, &json!([unsigned]), false).expect("installer").signature_url.is_none());
    }

    #[test]
    fn an_installer_without_a_github_digest_is_refused() {
        let mut no_digest = release("desktop-v2.3.910", false, "Excubia-2.3.910-setup.exe");
        no_digest["assets"][0]["digest"] = Value::Null;
        assert!(choose(excubia(), &json!([no_digest]), false).is_err());
    }
}
