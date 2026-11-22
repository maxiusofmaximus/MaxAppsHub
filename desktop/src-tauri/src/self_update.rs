//! The Hub updates itself: it downloads the newer release, checks its digest,
//! swaps its own program file and starts the new copy.

use std::path::PathBuf;
use std::process::Command;

use crate::github::{self, Installer};
use crate::i18n::tr;
use crate::{install, registry, signing};

const CURRENT: &str = env!("CARGO_PKG_VERSION");
const PREVIOUS: &str = "maxapps-hub-desktop.old.exe";
const STAGED: &str = "maxapps-hub-desktop.new.exe";

/// The newer release of the Hub, if there is one.
pub fn available() -> Option<Installer> {
    let latest = github::latest_installer(&registry::SELF_APP).ok()?;
    github::newer(&latest.version, CURRENT).then_some(latest)
}

fn program_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|_| "No encuentro el programa del Hub".to_string())?;
    exe.parent()
        .map(|dir| dir.to_path_buf())
        .ok_or_else(|| tr("No encuentro la carpeta del Hub", "I can't find the Hub's folder").to_string())
}

/// Replaces this program with the release and starts the new copy. The caller
/// must quit right after a success: the new copy waits for this one to close.
pub fn apply(installer: &Installer) -> Result<(), String> {
    let dir = program_dir()?;
    let exe = std::env::current_exe()
        .map_err(|_| tr("No encuentro el programa del Hub", "I can't find the Hub's program").to_string())?;
    let bytes = install::download(&installer.download_url)?;
    if !install::digest_matches(&installer.digest, &bytes) {
        crate::log::record("self-update refused: GitHub digest does not match");
        return Err(tr("La huella de la actualización no coincide; no la instalé.", "The update's fingerprint doesn't match; I didn't install it.").into());
    }
    // The publisher's own signature is the check that matters: a hijacked release page
    // can carry matching digests, but not a signature from the update key.
    let Some(signature_url) = installer.signature_url.as_deref() else {
        crate::log::record("self-update refused: release has no signature");
        return Err(tr("Esta versión no trae firma; no la instalé.", "This version has no signature; I didn't install it.").into());
    };
    let signature = install::download(signature_url)?;
    if !signing::verified(&String::from_utf8_lossy(&signature), &bytes) {
        crate::log::record("self-update refused: signature does not verify");
        return Err(tr("La firma de la actualización no es válida; no la instalé.", "The update's signature isn't valid; I didn't install it.").into());
    }
    crate::log::record(&format!("self-update {} verified, replacing the program", installer.version));
    let staged = dir.join(STAGED);
    let previous = dir.join(PREVIOUS);
    std::fs::write(&staged, &bytes)
        .map_err(|_| tr("No pude guardar la actualización", "I couldn't save the update").to_string())?;
    let _ = std::fs::remove_file(&previous);
    // A running program can be renamed on Windows, not overwritten.
    std::fs::rename(&exe, &previous)
        .map_err(|_| tr("No pude reemplazar el programa del Hub", "I couldn't replace the Hub's program").to_string())?;
    if std::fs::rename(&staged, &exe).is_err() {
        let _ = std::fs::rename(&previous, &exe);
        return Err(tr("No pude colocar la nueva versión; sigo con la anterior.", "I couldn't put the new version in place; I'm keeping the old one.").into());
    }
    Command::new(&exe)
        .arg("--updated")
        .spawn()
        .map_err(|_| tr("No pude arrancar la nueva versión", "I couldn't start the new version").to_string())?;
    Ok(())
}

/// Removes the copy the last update left behind.
pub fn clean_previous() {
    if let Ok(dir) = program_dir() {
        let _ = std::fs::remove_file(dir.join(PREVIOUS));
    }
}
