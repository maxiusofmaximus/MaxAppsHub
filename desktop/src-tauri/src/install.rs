//! Installs, finds and opens the apps in the registry. Every download is checked
//! against the GitHub digest before anything runs.

use std::io::Read;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::github::Installer;
use crate::i18n::tr;
use crate::registry::{Delivery, DesktopApp};

/// An app found on this PC.
#[derive(Clone)]
pub struct Installed {
    pub version: String,
    /// The program to open, when it can be found.
    pub exe: Option<PathBuf>,
    /// The uninstall command the installer registered (installers only).
    pub uninstall: Option<String>,
    /// A portable app whose program file is missing or no longer matches what was installed.
    pub damaged: bool,
    /// The version a rollback brings back, when one is kept.
    pub previous: Option<String>,
}

/// Holds the digest of the program as it was installed.
const DIGEST_FILE: &str = "installed.sha256";
const VERSION_FILE: &str = "version.txt";
/// The folder inside an app's folder that keeps the one previous version.
const PREVIOUS: &str = "previous";

/// False only when the program file no longer matches the digest recorded at install time.
/// Without a recorded digest (installed before this check) it cannot be judged, so it passes.
fn intact(dir: &std::path::Path, exe: &std::path::Path) -> bool {
    let Ok(recorded) = std::fs::read_to_string(dir.join(DIGEST_FILE)) else {
        return true;
    };
    let Ok(bytes) = std::fs::read(exe) else {
        return false;
    };
    digest_matches(recorded.trim(), &bytes)
}

/// True when the bytes hash to the digest GitHub recorded (`sha256:<hex>`).
pub fn digest_matches(digest: &str, bytes: &[u8]) -> bool {
    let Some(expected) = digest.strip_prefix("sha256:") else {
        return false;
    };
    let actual = hex::encode(Sha256::digest(bytes));
    actual.eq_ignore_ascii_case(expected)
}

/// Live state of one download, shared with the window: bytes so far, the total
/// (0 when the server did not say) and a request to stop.
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

pub fn download(url: &str) -> Result<Vec<u8>, String> {
    download_with(url, &Progress::default())
}

pub fn download_with(url: &str, progress: &Progress) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .set("User-Agent", "maxapps-hub-desktop")
        .call()
        .map_err(|_| tr("No pude descargar la versión.", "I couldn't download the version.").to_string())?;
    let total = response.header("Content-Length").and_then(|value| value.parse().ok()).unwrap_or(0);
    progress.total.store(total, Ordering::SeqCst);
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        if progress.cancel.load(Ordering::SeqCst) {
            return Err(tr("Descarga cancelada.", "Download cancelled.").into());
        }
        let read = reader
            .read(&mut chunk)
            .map_err(|_| tr("La descarga se cortó.", "The download was interrupted.").to_string())?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        progress.done.store(bytes.len() as u64, Ordering::SeqCst);
    }
    Ok(bytes)
}

fn portable_dir(folder: &str) -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("MaxAppsHub").join("apps").join(folder))
}

/// Downloads the release, refuses it unless its digest matches, and installs it.
/// Blocking: call it off the UI thread.
pub fn install(app: &DesktopApp, installer: &Installer, progress: &Progress) -> Result<(), String> {
    let bytes = download_with(&installer.download_url, progress)?;
    if !digest_matches(&installer.digest, &bytes) {
        return Err(tr("La huella de la descarga no coincide; no la instalé.", "The download's fingerprint doesn't match; I didn't install it.").into());
    }
    match &app.delivery {
        Delivery::Installer { .. } => {
            let file = std::env::temp_dir().join(&installer.file_name);
            std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;
            let status = Command::new(&file)
                .arg("/S")
                .status()
                .map_err(|_| tr("No pude arrancar el instalador.", "I couldn't start the installer.").to_string())?;
            let _ = std::fs::remove_file(&file);
            if status.success() {
                // The install itself worked; if keeping a copy fails, only the rollback is lost.
                if let Some(dir) = app_dir(app) {
                    let _ = record(&dir, &installer.file_name, &bytes, &installer.digest, &installer.version);
                }
                Ok(())
            } else {
                Err(format!(
                    "{} {}.",
                    tr("El instalador terminó con código", "The installer ended with code"),
                    status.code().unwrap_or(-1)
                ))
            }
        }
        Delivery::Portable { folder } => {
            let dir = portable_dir(folder).ok_or_else(|| tr("No encuentro LOCALAPPDATA", "I can't find LOCALAPPDATA").to_string())?;
            record(&dir, &installer.file_name, &bytes, &installer.digest, &installer.version)
        }
    }
}

/// The folder for an app's program: its portable folder, or its repository name for installers.
fn app_dir(app: &DesktopApp) -> Option<PathBuf> {
    match &app.delivery {
        Delivery::Portable { folder } => portable_dir(folder),
        Delivery::Installer { .. } => portable_dir(app.repo),
    }
}

/// The program files directly in a folder: its .exe files.
fn programs_in(dir: &std::path::Path) -> Vec<PathBuf> {
    let Ok(listing) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    listing
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "exe"))
        .collect()
}

/// Moves the program now in the folder into `previous/`, replacing the version kept there before.
fn keep_previous(dir: &std::path::Path) -> Result<(), String> {
    let programs = programs_in(dir);
    if programs.is_empty() {
        return Ok(());
    }
    let previous = dir.join(PREVIOUS);
    let _ = std::fs::remove_dir_all(&previous);
    std::fs::create_dir_all(&previous).map_err(|e| e.to_string())?;
    for program in programs {
        if let Some(name) = program.file_name() {
            std::fs::rename(&program, previous.join(name)).map_err(|e| e.to_string())?;
        }
    }
    for meta in [VERSION_FILE, DIGEST_FILE] {
        let _ = std::fs::rename(dir.join(meta), previous.join(meta));
    }
    Ok(())
}

fn write_meta(dir: &std::path::Path, digest: &str, version: &str) -> Result<(), String> {
    std::fs::write(dir.join(DIGEST_FILE), digest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(VERSION_FILE), version).map_err(|e| e.to_string())
}

/// Keeps the program in the folder as the previous version, then stores the new one in its place.
fn record(dir: &std::path::Path, file_name: &str, bytes: &[u8], digest: &str, version: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    keep_previous(dir)?;
    std::fs::write(dir.join(file_name), bytes).map_err(|e| e.to_string())?;
    write_meta(dir, digest, version)
}

/// The version kept as the previous one, when its program is still there.
fn previous_version_in(dir: &std::path::Path) -> Option<String> {
    let previous = dir.join(PREVIOUS);
    if programs_in(&previous).is_empty() {
        return None;
    }
    let version = std::fs::read_to_string(previous.join(VERSION_FILE)).ok()?;
    Some(version.trim().to_string()).filter(|version| !version.is_empty())
}

fn previous_version(app: &DesktopApp) -> Option<String> {
    previous_version_in(&app_dir(app)?)
}

/// Swaps the program in the folder with the one kept as previous, versions and digests included.
fn swap_in(dir: &std::path::Path) -> Result<(), String> {
    let previous = dir.join(PREVIOUS);
    let incoming = programs_in(&previous)
        .into_iter()
        .next()
        .ok_or_else(|| tr("No hay una versión anterior guardada.", "There is no previous version saved.").to_string())?;
    let name = incoming.file_name().ok_or_else(|| tr("Archivo sin nombre.", "File without a name.").to_string())?.to_owned();
    let read = |path: PathBuf| std::fs::read_to_string(path).unwrap_or_default();
    let (in_version, in_digest) = (read(previous.join(VERSION_FILE)), read(previous.join(DIGEST_FILE)));
    let (out_version, out_digest) = (read(dir.join(VERSION_FILE)), read(dir.join(DIGEST_FILE)));
    // Park the incoming program first, so the outgoing one can take its place in the previous folder.
    let parked = dir.join("swap.part");
    std::fs::rename(&incoming, &parked).map_err(|e| e.to_string())?;
    for program in programs_in(dir) {
        if let Some(file) = program.file_name() {
            std::fs::rename(&program, previous.join(file)).map_err(|e| e.to_string())?;
        }
    }
    std::fs::rename(&parked, dir.join(&name)).map_err(|e| e.to_string())?;
    write_meta(dir, &in_digest, &in_version)?;
    write_meta(&previous, &out_digest, &out_version)
}

/// Brings the previous version back. Its installer runs over the current one first; the two then swap
/// places, so a second rollback returns to the version it left.
pub fn rollback(app: &DesktopApp) -> Result<(), String> {
    let dir = app_dir(app).ok_or_else(|| tr("No encuentro LOCALAPPDATA", "I can't find LOCALAPPDATA").to_string())?;
    let previous = dir.join(PREVIOUS);
    let program = programs_in(&previous)
        .into_iter()
        .next()
        .ok_or_else(|| tr("No hay una versión anterior guardada.", "There is no previous version saved.").to_string())?;
    let digest = std::fs::read_to_string(previous.join(DIGEST_FILE)).unwrap_or_default();
    let bytes = std::fs::read(&program).map_err(|e| e.to_string())?;
    if !digest_matches(digest.trim(), &bytes) {
        return Err(tr(
            "La versión anterior guardada no coincide con su huella; no la uso.",
            "The saved previous version doesn't match its fingerprint; I won't use it.",
        )
        .into());
    }
    if matches!(app.delivery, Delivery::Installer { .. }) {
        let status = Command::new(&program)
            .arg("/S")
            .status()
            .map_err(|_| tr("No pude arrancar el instalador.", "I couldn't start the installer.").to_string())?;
        if !status.success() {
            return Err(format!(
                "{} {}.",
                tr("El instalador terminó con código", "The installer ended with code"),
                status.code().unwrap_or(-1)
            ));
        }
    }
    swap_in(&dir)
}

/// The app as it is on this PC, if it is installed, and the version a rollback would bring back.
pub fn installed(app: &DesktopApp) -> Option<Installed> {
    let mut found = installed_here(app)?;
    found.previous = previous_version(app);
    Some(found)
}

fn installed_here(app: &DesktopApp) -> Option<Installed> {
    match &app.delivery {
        Delivery::Installer { display_name } => uninstall_entry(display_name),
        Delivery::Portable { folder } => {
            let dir = portable_dir(folder)?;
            let version = std::fs::read_to_string(dir.join("version.txt")).ok()?.trim().to_string();
            let exe = std::fs::read_dir(&dir)
                .ok()?
                .flatten()
                .map(|entry| entry.path())
                .find(|path| path.extension().is_some_and(|ext| ext == "exe"));
            let damaged = match &exe {
                None => true,
                Some(path) => !intact(&dir, path),
            };
            Some(Installed { version, exe, uninstall: None, damaged, previous: None })
        }
    }
}

/// Removes the app: its own silent uninstaller for installers, the folder for portable apps.
pub fn uninstall(app: &DesktopApp, installed: &Installed) -> Result<(), String> {
    match &app.delivery {
        Delivery::Installer { .. } => {
            let command = installed
                .uninstall
                .as_deref()
                .ok_or_else(|| tr("No encuentro el desinstalador", "I can't find the uninstaller").to_string())?;
            let (program, args) = split_command(command);
            // NSIS uninstallers take /S for silent and _?= (their folder) to run in place.
            let dir = program.parent().map(PathBuf::from).unwrap_or_default();
            let status = Command::new(&program)
                .args(&args)
                .arg("/S")
                .arg(format!("_?={}", dir.display()))
                .status()
                .map_err(|_| tr("No pude arrancar el desinstalador.", "I couldn't start the uninstaller.").to_string())?;
            if status.success() {
                remove_leftover_uninstaller(&dir);
                Ok(())
            } else {
                Err(format!(
                    "{} {}.",
                    tr("El desinstalador terminó con código", "The uninstaller ended with code"),
                    status.code().unwrap_or(-1)
                ))
            }
        }
        Delivery::Portable { folder } => {
            let dir = portable_dir(folder).ok_or_else(|| tr("No encuentro LOCALAPPDATA", "I can't find LOCALAPPDATA").to_string())?;
            std::fs::remove_dir_all(&dir)
                .map_err(|_| tr("No pude borrar la carpeta de la app.", "I couldn't delete the app's folder.").to_string())
        }
    }
}

/// NSIS uninstallers run in place cannot delete themselves, so the folder keeps only `uninstall.exe`.
/// Removes that leftover and the folder, and nothing when other files are present.
/// shortcut: only handles NSIS's own leftover, not any other file a publisher leaves behind.
fn remove_leftover_uninstaller(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let entries: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    let only_uninstaller = entries.iter().all(|path| {
        path.file_name().is_some_and(|name| name.eq_ignore_ascii_case("uninstall.exe")) && path.is_file()
    });
    if only_uninstaller {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Splits a command such as `"C:\App\uninstall.exe" /x` into the program and its arguments.
fn split_command(command: &str) -> (PathBuf, Vec<String>) {
    let command = command.trim();
    let (program, rest) = match command.strip_prefix('"').and_then(|quoted| quoted.split_once('"')) {
        Some((program, rest)) => (program, rest),
        None => command.split_once(' ').unwrap_or((command, "")),
    };
    (PathBuf::from(program), rest.split_whitespace().map(String::from).collect())
}

#[cfg(windows)]
fn uninstall_entry(display_name: &str) -> Option<Installed> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let uninstall = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall")
        .ok()?;
    uninstall.enum_keys().flatten().find_map(|key_name| {
        let entry = uninstall.open_subkey(&key_name).ok()?;
        let name: String = entry.get_value("DisplayName").ok()?;
        if name != display_name {
            return None;
        }
        let version: String = entry.get_value("DisplayVersion").ok()?;
        let icon: String = entry.get_value("DisplayIcon").unwrap_or_default();
        let exe = PathBuf::from(icon.trim_matches('"').split(',').next().unwrap_or_default());
        let uninstall = entry.get_value::<String, _>("UninstallString").ok();
        Some(Installed { version, exe: exe.exists().then_some(exe), uninstall, damaged: false, previous: None })
    })
}

#[cfg(not(windows))]
fn uninstall_entry(_display_name: &str) -> Option<Installed> {
    None
}

/// The bytes under a folder, or None when it cannot be read or holds too many entries to
/// count quickly. Links are not followed, so a link cannot make the count loop.
pub fn folder_size(dir: &std::path::Path) -> Option<u64> {
    const MAX_ENTRIES: usize = 50_000;
    if !dir.is_dir() {
        return None;
    }
    let mut total = 0u64;
    let mut entries = 0usize;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(listing) = std::fs::read_dir(&current) else { continue };
        for entry in listing.flatten() {
            entries += 1;
            if entries > MAX_ENTRIES {
                return None;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                pending.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    Some(total)
}

/// A size for people to read: "1.5 GB", "120 MB" or "< 1 MB".
pub fn human_size(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb < 1.0 {
        "< 1 MB".into()
    } else if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_counted_by_its_files_and_sizes_are_readable() {
        let dir = std::env::temp_dir().join(format!("maxapps-size-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.bin"), vec![0u8; 1000]).unwrap();
        std::fs::write(dir.join("sub").join("b.bin"), vec![0u8; 24]).unwrap();
        assert_eq!(folder_size(&dir), Some(1024));
        assert_eq!(folder_size(&dir.join("missing")), None);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(human_size(0), "< 1 MB");
        assert_eq!(human_size(120 * 1024 * 1024), "120 MB");
        assert_eq!(human_size(1536 * 1024 * 1024), "1.5 GB");
    }

    #[test]
    fn an_uninstall_command_is_split_into_program_and_arguments() {
        let (program, args) = split_command(r#""C:\Apps\Excubia\uninstall.exe" /keep"#);
        assert_eq!(program, PathBuf::from(r"C:\Apps\Excubia\uninstall.exe"));
        assert_eq!(args, vec!["/keep"]);
        let (program, args) = split_command(r"C:\Apps\uninstall.exe");
        assert_eq!(program, PathBuf::from(r"C:\Apps\uninstall.exe"));
        assert!(args.is_empty());
    }

    #[test]
    fn a_program_that_no_longer_matches_its_recorded_digest_is_not_intact() {
        let dir = std::env::temp_dir().join(format!("maxapps-intact-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("app.exe");
        std::fs::write(&exe, b"programa").unwrap();
        // Without a recorded digest the program cannot be judged, so it passes.
        assert!(intact(&dir, &exe));
        std::fs::write(dir.join(DIGEST_FILE), format!("sha256:{}", hex::encode(Sha256::digest(b"programa")))).unwrap();
        assert!(intact(&dir, &exe));
        std::fs::write(&exe, b"programa roto").unwrap();
        assert!(!intact(&dir, &exe));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_leftover_uninstaller_is_removed_but_other_files_are_kept() {
        let base = std::env::temp_dir().join(format!("maxapps-leftover-{}", std::process::id()));
        let alone = base.join("alone");
        std::fs::create_dir_all(&alone).unwrap();
        std::fs::write(alone.join("uninstall.exe"), b"u").unwrap();
        remove_leftover_uninstaller(&alone);
        assert!(!alone.exists());

        let kept = base.join("kept");
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::write(kept.join("uninstall.exe"), b"u").unwrap();
        std::fs::write(kept.join("datos.db"), b"d").unwrap();
        remove_leftover_uninstaller(&kept);
        assert!(kept.join("datos.db").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_digest_matches_only_the_exact_bytes() {
        let bytes = b"instalador";
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));
        assert!(digest_matches(&digest, bytes));
        assert!(!digest_matches(&digest, b"otro"));
        assert!(!digest_matches("md5:abc", bytes));
    }

    #[test]
    fn a_new_version_keeps_the_old_one_and_a_rollback_swaps_them_back() {
        fn digest_of(bytes: &[u8]) -> String {
            format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
        }
        let dir = std::env::temp_dir().join(format!("maxapps-rollback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        record(&dir, "app-1.exe", b"uno", &digest_of(b"uno"), "1.0.0").unwrap();
        assert_eq!(previous_version_in(&dir), None);
        record(&dir, "app-2.exe", b"dos", &digest_of(b"dos"), "2.0.0").unwrap();
        assert_eq!(previous_version_in(&dir).as_deref(), Some("1.0.0"));

        swap_in(&dir).unwrap();
        assert_eq!(std::fs::read(dir.join("app-1.exe")).unwrap(), b"uno");
        assert!(!dir.join("app-2.exe").exists());
        assert_eq!(std::fs::read_to_string(dir.join(VERSION_FILE)).unwrap(), "1.0.0");
        assert_eq!(previous_version_in(&dir).as_deref(), Some("2.0.0"));

        swap_in(&dir).unwrap();
        assert_eq!(std::fs::read(dir.join("app-2.exe")).unwrap(), b"dos");
        assert_eq!(previous_version_in(&dir).as_deref(), Some("1.0.0"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
