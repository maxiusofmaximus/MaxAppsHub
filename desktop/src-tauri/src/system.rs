//! Start menu shortcuts and "Installed apps" entries for the programs the Hub copies onto this PC:
//! the portable apps and the Hub itself. Installers write their own, so the Hub leaves them alone.
//! Every such folder keeps an `uninstall.exe`: a copy of the Hub that runs with `--uninstall`.

use std::path::{Path, PathBuf};

use crate::i18n::tr;
use crate::install::folder_size;

/// The Start menu folder that groups the shortcuts.
const GROUP: &str = "MaxApps";
/// The uninstaller kept beside each program. The Hub copies itself under this name.
pub const UNINSTALLER: &str = "uninstall.exe";
/// The folder and program file the Hub gives itself when it is installed.
pub const HUB_FOLDER: &str = "maxapps-hub";
pub const HUB_PROGRAM: &str = "MaxAppsHub.exe";
const HUB_NAME: &str = "MaxApps Hub";
#[cfg(windows)]
const UNINSTALL_ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
/// Keeps a console window from flashing up for the helper processes.
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

/// What the Hub puts on this PC for one program.
pub struct Entry<'a> {
    /// The folder the program lives in, under the Hub's apps folder.
    pub folder: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub dir: &'a Path,
    pub program: &'a Path,
}

/// The Windows registry name of the entry for a folder.
fn key_name(folder: &str) -> String {
    format!("MaxAppsHub-{folder}")
}

/// The command Windows runs from "Installed apps" to remove the program.
fn uninstall_command(dir: &Path) -> String {
    format!("\"{}\" --uninstall", dir.join(UNINSTALLER).display())
}

fn start_menu_dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("Microsoft").join("Windows").join("Start Menu").join("Programs").join(GROUP))
}

/// Puts the shortcut, the uninstaller and the "Installed apps" entry in place for one program.
pub fn register(entry: &Entry<'_>) -> Result<(), String> {
    let copy = entry.dir.join(UNINSTALLER);
    let current = std::env::current_exe().map_err(|_| tr("No encuentro el programa del Hub", "I can't find the Hub's program").to_string())?;
    if !same_file(&current, &copy) {
        std::fs::copy(&current, &copy).map_err(|_| tr("No pude copiar el desinstalador.", "I couldn't copy the uninstaller.").to_string())?;
    }
    let menu = start_menu_dir().ok_or_else(|| tr("No encuentro el menú Inicio", "I can't find the Start menu").to_string())?;
    std::fs::create_dir_all(&menu).map_err(|e| e.to_string())?;
    write_shortcut(&menu.join(format!("{}.lnk", entry.name)), entry.program, entry.dir)?;
    let size = folder_size(entry.dir).map(|bytes| (bytes / 1024) as u32).unwrap_or(0);
    write_entry(&key_name(entry.folder), entry, &uninstall_command(entry.dir), size)
}

/// Removes the shortcut and the "Installed apps" entry of a folder. The program files stay.
pub fn unregister(folder: &str) -> Result<(), String> {
    if let (Some(name), Some(menu)) = (display_name(&key_name(folder)), start_menu_dir()) {
        let _ = std::fs::remove_file(menu.join(format!("{name}.lnk")));
        let _ = std::fs::remove_dir(&menu);
    }
    remove_entry(&key_name(folder));
    Ok(())
}

/// True when the folder has its "Installed apps" entry.
pub fn is_registered(folder: &str) -> bool {
    display_name(&key_name(folder)).is_some()
}

/// Installs the Hub into its own folder, so it has a shortcut and an uninstaller too.
pub fn install_hub() -> Result<(), String> {
    let dir = crate::install::portable_dir(HUB_FOLDER).ok_or_else(|| tr("No encuentro LOCALAPPDATA", "I can't find LOCALAPPDATA").to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let program = dir.join(HUB_PROGRAM);
    let current = std::env::current_exe().map_err(|_| tr("No encuentro el programa del Hub", "I can't find the Hub's program").to_string())?;
    if !same_file(&current, &program) {
        std::fs::copy(&current, &program).map_err(|_| tr("No pude copiar el Hub a su carpeta.", "I couldn't copy the Hub to its folder.").to_string())?;
    }
    register(&Entry { folder: HUB_FOLDER, name: HUB_NAME, version: env!("CARGO_PKG_VERSION"), dir: &dir, program: &program })
}

/// True when the Hub has been installed into its own folder with its entry.
pub fn hub_installed() -> bool {
    crate::install::portable_dir(HUB_FOLDER).is_some_and(|dir| dir.join(HUB_PROGRAM).is_file()) && is_registered(HUB_FOLDER)
}

/// Runs as the uninstaller: removes the shortcut and the entry, then the folder once this program has closed.
pub fn uninstall_self() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|_| tr("No encuentro el desinstalador", "I can't find the uninstaller").to_string())?;
    let dir = exe.parent().ok_or_else(|| tr("No encuentro la carpeta de la app", "I can't find the app's folder").to_string())?.to_path_buf();
    let folder = dir.file_name().and_then(|name| name.to_str()).ok_or_else(|| tr("Carpeta sin nombre", "Folder without a name").to_string())?;
    unregister(folder)?;
    remove_folder_later(&dir)
}

/// A program cannot delete the folder it runs from, so a shell waits a moment and then removes it.
fn remove_folder_later(dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .arg("/C")
            .raw_arg(format!("ping -n 3 127.0.0.1 >nul & rmdir /S /Q \"{}\"", dir.display()))
            .creation_flags(NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|_| tr("No pude borrar la carpeta de la app.", "I couldn't delete the app's folder.").to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Err(tr("Solo funciona en Windows.", "It only works on Windows.").to_string())
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// A shortcut to the program, written by Windows Script Host. The paths go in through
/// environment variables, so no quoting can break them.
#[cfg(windows)]
fn write_shortcut(path: &Path, target: &Path, work_dir: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg("$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:LNK_PATH); $s.TargetPath=$env:LNK_TARGET; $s.WorkingDirectory=$env:LNK_DIR; $s.IconLocation=$env:LNK_TARGET; $s.Save()")
        .env("LNK_PATH", path)
        .env("LNK_TARGET", target)
        .env("LNK_DIR", work_dir)
        .creation_flags(NO_WINDOW)
        .status()
        .map_err(|_| tr("No pude crear el acceso directo.", "I couldn't create the shortcut.").to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(tr("No pude crear el acceso directo.", "I couldn't create the shortcut.").to_string())
    }
}

#[cfg(not(windows))]
fn write_shortcut(_: &Path, _: &Path, _: &Path) -> Result<(), String> {
    Err(tr("Solo funciona en Windows.", "It only works on Windows.").to_string())
}

#[cfg(windows)]
fn write_entry(key: &str, entry: &Entry<'_>, uninstall: &str, size_kb: u32) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let failed = || tr("No pude guardar la entrada en Aplicaciones instaladas.", "I couldn't save the entry in Installed apps.").to_string();
    let (entry_key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!("{UNINSTALL_ROOT}\\{key}")).map_err(|_| failed())?;
    let install_location = entry.dir.display().to_string();
    let icon = entry.program.display().to_string();
    entry_key.set_value("DisplayName", &entry.name).map_err(|_| failed())?;
    entry_key.set_value("DisplayVersion", &entry.version).map_err(|_| failed())?;
    entry_key.set_value("Publisher", &"MaxApps").map_err(|_| failed())?;
    entry_key.set_value("InstallLocation", &install_location).map_err(|_| failed())?;
    entry_key.set_value("DisplayIcon", &icon).map_err(|_| failed())?;
    entry_key.set_value("UninstallString", &uninstall).map_err(|_| failed())?;
    entry_key.set_value("EstimatedSize", &size_kb).map_err(|_| failed())?;
    entry_key.set_value("NoModify", &1u32).map_err(|_| failed())?;
    entry_key.set_value("NoRepair", &1u32).map_err(|_| failed())
}

#[cfg(not(windows))]
fn write_entry(_: &str, _: &Entry<'_>, _: &str, _: u32) -> Result<(), String> {
    Err(tr("Solo funciona en Windows.", "It only works on Windows.").to_string())
}

#[cfg(windows)]
fn display_name(key: &str) -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(format!("{UNINSTALL_ROOT}\\{key}"))
        .ok()?
        .get_value::<String, _>("DisplayName")
        .ok()
}

#[cfg(not(windows))]
fn display_name(_: &str) -> Option<String> {
    None
}

#[cfg(windows)]
fn remove_entry(key: &str) {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(format!("{UNINSTALL_ROOT}\\{key}"));
}

#[cfg(not(windows))]
fn remove_entry(_: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uninstaller_is_named_after_its_folder_and_runs_in_uninstall_mode() {
        let dir = Path::new(r"C:\Users\Max\AppData\Local\MaxAppsHub\apps\download-lovable");
        assert_eq!(key_name("download-lovable"), "MaxAppsHub-download-lovable");
        assert_eq!(
            uninstall_command(dir),
            format!("\"{}\\uninstall.exe\" --uninstall", dir.display())
        );
    }
}
