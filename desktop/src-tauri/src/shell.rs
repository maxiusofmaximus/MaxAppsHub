//! What happens around the window: the saved close choice, hiding and showing
//! the window, and the tray icon. GPUI 0.2 cannot hide a window on Windows, so
//! the Win32 calls are made here on the window's own handle.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Duration;

use gpui::Window;
use crate::i18n::tr;
use crate::palette::{self, SystemLook, FALLBACK_ACCENT};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND};
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE};

/// The main window's handle, stored once the window exists (0 before that).
static MAIN_WINDOW: AtomicIsize = AtomicIsize::new(0);

/// Set by the listener thread when a second start asks for the window.
static SHOW_REQUEST: AtomicBool = AtomicBool::new(false);

/// Set by the listener thread when a newer start asks this Hub to close.
static QUIT_REQUEST: AtomicBool = AtomicBool::new(false);

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub enum Launch {
    First,
    /// Another Hub already runs and this start must exit. A non-empty text is a message to show first.
    Second(String),
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn running_file() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("MaxAppsHub").join("running.txt"))
}

/// The version the running Hub wrote when it started; None when it predates this check.
fn running_version() -> Option<String> {
    std::fs::read_to_string(running_file()?).ok().map(|text| text.trim().to_string())
}

/// A start replaces the running Hub only when it is newer. A Hub that predates this check
/// writes no version, so it counts as older.
pub fn should_replace(own: &str, running: Option<&str>) -> bool {
    running.map_or(true, |running| crate::github::newer(own, running))
}

/// One Hub per user session. A newer start asks the running Hub to close and takes over;
/// an equal or older one wakes the running Hub and exits.
pub fn claim_instance() -> Launch {
    let mutex_name = wide("Local\\MaxAppsHub.Single");
    let show_name = wide("Local\\MaxAppsHub.Show");
    let quit_name = wide("Local\\MaxAppsHub.Quit");
    unsafe {
        let mut mutex = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
        let mut already = GetLastError() == ERROR_ALREADY_EXISTS;
        if already {
            let running = running_version();
            if !should_replace(VERSION, running.as_deref()) {
                let show = CreateEventW(std::ptr::null(), 0, 0, show_name.as_ptr());
                SetEvent(show);
                CloseHandle(show);
                CloseHandle(mutex);
                return Launch::Second(String::new());
            }
            let quit = CreateEventW(std::ptr::null(), 0, 0, quit_name.as_ptr());
            SetEvent(quit);
            CloseHandle(quit);
            // The running copy closes within moments (after a self-update it may still be closing); wait up to 10 s.
            for _ in 0..100 {
                CloseHandle(mutex);
                std::thread::sleep(Duration::from_millis(100));
                mutex = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
                already = GetLastError() == ERROR_ALREADY_EXISTS;
                if !already {
                    break;
                }
            }
            if already {
                CloseHandle(mutex);
                // Hubs older than this check ignore the close request; the user closes them from the tray.
                let version = running.unwrap_or_else(|| tr("anterior", "older").to_string());
                let text = format!(
                    "{} ({} {version}); {} {VERSION}.",
                    tr("MaxApps Hub ya está abierto", "MaxApps Hub is already open"),
                    tr("versión", "version"),
                    tr("ciérralo desde la bandeja para abrir la", "close it from the tray to open"),
                );
                return Launch::Second(text);
            }
        }
        let show = CreateEventW(std::ptr::null(), 0, 0, show_name.as_ptr());
        let quit = CreateEventW(std::ptr::null(), 0, 0, quit_name.as_ptr());
        // Handles are plain numbers; the listeners own them for the process lifetime.
        let (show, quit) = (show as isize, quit as isize);
        std::thread::spawn(move || loop {
            WaitForSingleObject(show as HANDLE, INFINITE);
            SHOW_REQUEST.store(true, Ordering::SeqCst);
        });
        std::thread::spawn(move || loop {
            WaitForSingleObject(quit as HANDLE, INFINITE);
            QUIT_REQUEST.store(true, Ordering::SeqCst);
        });
        if let Some(file) = running_file() {
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(file, VERSION);
        }
        // The mutex handle is kept open on purpose: it marks this instance as the running one.
        let _ = mutex;
        Launch::First
    }
}

pub fn take_show_request() -> bool {
    SHOW_REQUEST.swap(false, Ordering::SeqCst)
}

/// True once when a newer start has asked this Hub to close.
pub fn take_quit_request() -> bool {
    QUIT_REQUEST.swap(false, Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::should_replace;

    #[test]
    fn a_newer_start_asks_the_running_hub_to_close_and_an_older_or_equal_one_does_not() {
        assert!(should_replace("1.0.1-alpha.8", Some("1.0.1-alpha.7")));
        assert!(should_replace("1.0.1-alpha.10", Some("1.0.1-alpha.5")));
        assert!(should_replace("1.0.1", Some("1.0.1-alpha.8")));
        assert!(should_replace("1.0.1-alpha.8", None));
        assert!(!should_replace("1.0.1-alpha.8", Some("1.0.1-alpha.8")));
        assert!(!should_replace("1.0.1-alpha.7", Some("1.0.1-alpha.8")));
    }
}

/// Identifies the Hub to Windows notifications; registered once so toasts show its name.
pub const APP_ID: &str = "MaxAppsHub";

pub fn register_app_id() {
    if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!("Software\\Classes\\AppUserModelId\\{APP_ID}")) {
        let _ = key.set_value("DisplayName", &"MaxApps Hub".to_string());
    }
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_NAME: &str = "MaxAppsHub";

/// Whether the Hub starts with Windows.
pub fn autostart_on() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|run| run.get_value::<String, _>(RUN_NAME))
        .is_ok()
}

/// Adds or removes the Hub from the Windows startup list. At logon it starts hidden in the tray.
pub fn set_autostart(on: bool) -> Result<(), String> {
    let (run, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(RUN_KEY)
        .map_err(|_| "No pude abrir el inicio de Windows".to_string())?;
    if on {
        let exe = std::env::current_exe().map_err(|_| "No encuentro el programa del Hub".to_string())?;
        run.set_value(RUN_NAME, &format!("\"{}\" --background", exe.display()))
            .map_err(|_| tr("No pude activar el inicio con Windows", "I couldn't turn on start with Windows").to_string())
    } else {
        match run.delete_value(RUN_NAME) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(tr("No pude desactivar el inicio con Windows", "I couldn't turn off start with Windows").into()),
        }
    }
}

fn settings_file() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("MaxAppsHub").join("settings.json"))
}

fn settings() -> serde_json::Value {
    settings_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Writes one key and keeps the others, so every setting survives every save.
fn save_value(key: &str, value: serde_json::Value) -> Result<(), String> {
    let no_folder = || tr("No encuentro la carpeta de datos", "I can't find the data folder").to_string();
    let path = settings_file().ok_or_else(no_folder)?;
    let dir = path.parent().ok_or_else(no_folder)?;
    std::fs::create_dir_all(dir)
        .map_err(|_| tr("No pude crear la carpeta de datos", "I couldn't create the data folder").to_string())?;
    let mut all = settings();
    all[key] = value;
    std::fs::write(&path, all.to_string())
        .map_err(|_| tr("No pude guardar el ajuste", "I couldn't save the setting").to_string())
}

fn save_setting(key: &str, on: bool) -> Result<(), String> {
    save_value(key, serde_json::json!(on))
}

/// Spanish unless the user chose English in Settings.
pub fn english_on() -> bool {
    settings()["language"].as_str() == Some("en")
}

pub fn save_english(on: bool) -> Result<(), String> {
    save_value("language", serde_json::json!(if on { "en" } else { "es" }))
}

/// The palette the user picked; "system" unless they chose another one in Settings.
pub fn palette_saved() -> String {
    settings()["palette"].as_str().unwrap_or("system").to_string()
}

pub fn save_palette(id: &str) -> Result<(), String> {
    save_value("palette", serde_json::json!(id))
}

/// Windows' light or dark mode and accent color, for the Sistema palette.
pub fn system_look() -> SystemLook {
    // AppsUseLightTheme is 1 in light mode; a missing value means light.
    let dark = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .and_then(|key| key.get_value::<u32, _>("AppsUseLightTheme"))
        .is_ok_and(|light| light == 0);
    let dwm = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Microsoft\\Windows\\DWM");
    let read = |name: &str| dwm.as_ref().ok().and_then(|key| key.get_value::<u32, _>(name).ok());
    // AccentColor is ABGR; older Windows only has ColorizationColor, which is ARGB.
    let accent = match read("AccentColor") {
        Some(abgr) => palette::accent_from_abgr(abgr),
        None => read("ColorizationColor").map_or(FALLBACK_ACCENT, |argb| argb & 0x00ff_ffff),
    };
    SystemLook { dark, accent }
}

/// Closing the window keeps the Hub in the tray unless the user chose otherwise.
pub fn close_to_tray_saved() -> bool {
    settings()["close_to_tray"].as_bool().unwrap_or(true)
}

pub fn save_close_to_tray(on: bool) -> Result<(), String> {
    save_setting("close_to_tray", on)
}

/// Off by default: alpha builds are offered only to whoever turns this on.
pub fn alpha_channel_on() -> bool {
    settings()["alpha_channel"].as_bool().unwrap_or(false)
}

pub fn save_alpha_channel(on: bool) -> Result<(), String> {
    save_setting("alpha_channel", on)
}

/// Alpha builds for one app only, switched on its card.
pub fn alpha_app_on(app: &str) -> bool {
    settings()["alpha_apps"].as_array().is_some_and(|apps| apps.iter().any(|name| name.as_str() == Some(app)))
}

pub fn save_alpha_app(app: &str, on: bool) -> Result<(), String> {
    let mut apps: Vec<String> = settings()["alpha_apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|name| name.as_str().map(String::from))
        .collect();
    apps.retain(|name| name != app);
    if on {
        apps.push(app.to_string());
    }
    save_value("alpha_apps", serde_json::json!(apps))
}

/// The folder the user chose for new installs. None keeps the default folder.
pub fn install_root_saved() -> Option<PathBuf> {
    settings()["install_root"].as_str().filter(|path| !path.is_empty()).map(PathBuf::from)
}

pub fn save_install_root(root: Option<&std::path::Path>) -> Result<(), String> {
    save_value("install_root", serde_json::json!(root.map(|path| path.display().to_string()).unwrap_or_default()))
}

/// Asks for a folder with the standard Windows folder dialog. None when the user cancels.
#[cfg(windows)]
pub fn pick_folder(start: &std::path::Path) -> Result<Option<PathBuf>, String> {
    use std::os::windows::process::CommandExt;
    let script = "Add-Type -AssemblyName System.Windows.Forms; \
        $owner = New-Object System.Windows.Forms.Form -Property @{ TopMost = $true }; \
        $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; \
        $dialog.SelectedPath = $env:HUB_START; $dialog.Description = $env:HUB_TEXT; $dialog.ShowNewFolderButton = $true; \
        if ($dialog.ShowDialog($owner) -eq 'OK') { Write-Output $dialog.SelectedPath }";
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script])
        .env("HUB_START", start)
        .env("HUB_TEXT", tr("Elige dónde instalar las apps nuevas", "Choose where to install new apps"))
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|_| tr("No pude abrir el selector de carpetas", "I couldn't open the folder picker").to_string())?;
    let chosen = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!chosen.is_empty()).then(|| PathBuf::from(chosen)))
}

#[cfg(not(windows))]
pub fn pick_folder(_: &std::path::Path) -> Result<Option<PathBuf>, String> {
    Err(tr("Solo funciona en Windows.", "It only works on Windows.").to_string())
}

/// The Hub's data folder: its settings and its log.
pub fn data_dir() -> Option<PathBuf> {
    settings_file().and_then(|file| file.parent().map(PathBuf::from))
}

/// Opens a file or folder in Explorer.
pub fn open_in_explorer(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|_| tr("No pude abrir el Explorador", "I couldn't open Explorer").to_string())
}

/// On by default. A release installs itself only after the waiting time in `updates`.
pub fn auto_update_on() -> bool {
    settings()["auto_update"].as_bool().unwrap_or(true)
}

pub fn save_auto_update(on: bool) -> Result<(), String> {
    save_setting("auto_update", on)
}

/// Remembers the window's handle so the tray can bring it back later.
pub fn remember_window(window: &Window) {
    // GPUI has its own window_handle(); the raw Win32 handle comes from the trait.
    if let Ok(handle) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(win) = handle.as_raw() {
            MAIN_WINDOW.store(win.hwnd.get(), Ordering::SeqCst);
        }
    }
}

fn main_window() -> HWND {
    MAIN_WINDOW.load(Ordering::SeqCst) as HWND
}

/// Hides the window: it leaves the screen and the taskbar, the app keeps running.
pub fn hide_window() {
    unsafe { ShowWindow(main_window(), SW_HIDE) };
}

/// Brings the window back, restoring it if it was minimized.
pub fn show_window() {
    unsafe {
        ShowWindow(main_window(), SW_RESTORE);
        SetForegroundWindow(main_window());
    }
}

pub struct Tray {
    _icon: TrayIcon,
    open: MenuId,
    update_all: MenuId,
    data: MenuId,
    quit: MenuId,
}

pub enum TrayAction {
    Show,
    UpdateAll,
    OpenData,
    Quit,
}

/// The Hub's own icon, the same file the program embeds, sized for the tray.
fn tile_icon() -> Result<Icon, String> {
    const ICON: &[u8] = include_bytes!("../icons/icon.ico");
    let rgba = image::load_from_memory_with_format(ICON, image::ImageFormat::Ico)
        .map_err(|error| error.to_string())?
        .resize_exact(32, 32, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (width, height) = rgba.dimensions();
    Icon::from_rgba(rgba.into_raw(), width, height).map_err(|error| error.to_string())
}

/// The tray icon. A left click shows the window; the menu has Open, Update all, the
/// data folder and Quit.
pub fn create_tray() -> Result<Tray, String> {
    let open = MenuItem::with_id("open", tr("Abrir MaxApps Hub", "Open MaxApps Hub"), true, None);
    let update_all = MenuItem::with_id("update-all", tr("Actualizar todo", "Update all"), true, None);
    let data = MenuItem::with_id("data", tr("Abrir carpeta de datos", "Open data folder"), true, None);
    let quit = MenuItem::with_id("quit", tr("Salir de MaxApps Hub", "Quit MaxApps Hub"), true, None);
    let menu = Menu::new();
    for item in [&open, &update_all, &data, &quit] {
        menu.append(item).map_err(|error| error.to_string())?;
    }
    let icon = TrayIconBuilder::new()
        .with_tooltip("MaxApps Hub")
        .with_icon(tile_icon()?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| error.to_string())?;
    Ok(Tray {
        _icon: icon,
        open: open.id().clone(),
        update_all: update_all.id().clone(),
        data: data.id().clone(),
        quit: quit.id().clone(),
    })
}

/// What the user asked the tray for since the last call.
pub fn pending(tray: &Tray) -> Vec<TrayAction> {
    let mut actions = Vec::new();
    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            actions.push(TrayAction::Show);
        }
    }
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        if event.id == tray.open {
            actions.push(TrayAction::Show);
        } else if event.id == tray.update_all {
            actions.push(TrayAction::UpdateAll);
        } else if event.id == tray.data {
            actions.push(TrayAction::OpenData);
        } else if event.id == tray.quit {
            actions.push(TrayAction::Quit);
        }
    }
    actions
}
