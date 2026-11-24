#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod github;
mod i18n;
mod install;
mod log;
mod palette;
mod registry;
mod self_update;
mod shell;
mod signing;
mod system;
mod updates;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    div, img, prelude::*, px, relative, rgb, size, App, Application, Bounds, Context, Div, ElementId, FocusHandle,
    FontWeight, Image, ImageFormat, KeyDownEvent, Pixels, Render, ScrollHandle, SharedString, Stateful, TitlebarOptions,
    Window, WindowBounds, WindowOptions,
};

use i18n::tr;
use install::{Installed, Progress};
use palette::{ink_on, Palette};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR};

/// The Hub's icon for the sidebar, 64 px: the approved icon scaled down.
const BRAND_ICON: &[u8] = include_bytes!("../icons/hub-64.png");

/// Lines of "Qué hay de nuevo" shown before "Ver todo".
const WHATS_NEW_LINES: usize = 6;

/// What the window shows: the library, one app's detail, or the settings.
#[derive(Clone, PartialEq)]
enum Screen {
    Apps,
    Settings,
    Detail(String),
}

/// One app as the window shows it.
#[derive(Clone)]
struct Row {
    name: String,
    accent: u32,
    installed: Option<Installed>,
    latest: Option<String>,
    /// The size of the latest release's download.
    download: Option<u64>,
    /// The release notes of the latest version, read before the user updates.
    notes: String,
    /// Alpha builds are offered for this app alone.
    alpha_app: bool,
    /// The app has only alpha builds and the alpha channel is off: not a failed check.
    alpha_only: bool,
    /// The bytes the installed program takes up, counted in the background when the list loads.
    size: Option<u64>,
    /// The app's languages and Windows requirements, from the registry. Empty means not shown.
    languages: &'static [&'static str],
    requirements: &'static str,
    /// Whether "Qué hay de nuevo" is drawn for this app.
    show_notes: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Install,
    Open,
    Uninstall,
    Rollback,
}

/// What a button does when clicked.
#[derive(Clone, Copy)]
enum Cta {
    Act(Action),
    /// Checks GitHub again, for an app whose versions could not be read.
    Refresh,
}

/// How a button looks: the main action, a secondary one, or greyed out with nothing to click.
#[derive(Clone, Copy)]
enum Look {
    Filled,
    Outline,
    Disabled,
}

/// The main button of an app: its label, and what a click does when there is an action.
struct Button {
    label: SharedString,
    cta: Option<Cta>,
}

/// The chip, the support line and the main button of an app, from its state.
struct Status {
    chip: &'static str,
    chip_colors: (u32, u32),
    line: String,
    line_color: u32,
    button: Button,
}

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Installed,
    Updates,
}

impl Filter {
    fn title(self) -> &'static str {
        match self {
            Filter::All => tr("Todas las apps", "All apps"),
            Filter::Installed => tr("Instaladas", "Installed"),
            Filter::Updates => tr("Actualizaciones", "Updates"),
        }
    }

    fn matches(self, row: &Row) -> bool {
        match self {
            Filter::All => true,
            Filter::Installed => row.installed.is_some(),
            Filter::Updates => has_update(row),
        }
    }
}

fn has_update(row: &Row) -> bool {
    match (&row.installed, &row.latest) {
        (Some(installed), Some(latest)) => installed.version != *latest,
        _ => false,
    }
}

/// Rank for "Por estado": updates first, then installed apps, then the rest.
fn status_rank(row: &Row) -> u8 {
    if has_update(row) {
        0
    } else if row.installed.is_some() {
        1
    } else {
        2
    }
}

/// The library's order: by status (and by name inside each group), or by name alone.
fn sort_rows(rows: &mut [&Row], by_status: bool) {
    rows.sort_by(|a, b| {
        let by_name = a.name.cmp(&b.name);
        if by_status {
            status_rank(a).cmp(&status_rank(b)).then(by_name)
        } else {
            by_name
        }
    });
}

fn matches_search(row: &Row, needle: &str) -> bool {
    let description = registry::find(&row.name).map_or("", |app| app.description());
    row.name.to_lowercase().contains(needle) || description.to_lowercase().contains(needle)
}

/// The first letter of the first two words, in capitals: "Download Lovable" gives "DL".
fn initials(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .collect()
}

/// The release notes as plain lines for "Qué hay de nuevo": no bullets, heading or Markdown marks.
fn whats_new(notes: &str) -> Vec<String> {
    notes
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(|c: char| matches!(c, '-' | '*' | '+' | '#' | ' '))
                .replace(|c: char| matches!(c, '`' | '*'), "")
                .trim()
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

/// A note line about the installer, the build, the pairing or the version channel: it goes under "Requisitos y detalles" only.
/// shortcut: filtro por palabras clave; el publicador debería escribir notas para usuarios
fn is_technical(line: &str) -> bool {
    let lower = line.to_lowercase();
    ["sha-256", "nsis", "commit", "instalador", "| iex", "irm ", "http", "tailscale", "(estable)", "(alfa)"]
        .iter()
        .any(|word| lower.contains(word))
        || lower.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ',' | '.' | ':' | ';')).any(|token| token == "/s")
}

/// Bytes received and in total. The total is 0 until the size is known.
fn progress_of(progress: &Progress) -> (u64, u64) {
    (progress.done.load(Ordering::SeqCst), progress.total.load(Ordering::SeqCst))
}

/// True once every byte is in: the program is installing now.
fn download_finished(progress: &Progress) -> bool {
    let (done, total) = progress_of(progress);
    total > 0 && done >= total
}

fn progress_label(done: u64, total: u64) -> SharedString {
    let word = tr("Descargando", "Downloading");
    if total == 0 {
        return format!("{word}…").into();
    }
    let percent = (done * 100 / total).min(100);
    format!("{word}… {percent}{}", tr(" %", "%")).into()
}

/// The install folder's name for the screens. The full path only appears under Advanced.
fn folder_label() -> String {
    match shell::install_root_saved() {
        Some(path) => path
            .file_name()
            .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned()),
        None => tr("Tu carpeta de usuario", "Your user folder").to_string(),
    }
}

fn full_folder() -> String {
    shell::install_root_saved()
        .or_else(install::apps_root)
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

fn status_of(app: &registry::DesktopApp) -> Row {
    let installed = install::installed(app);
    let (latest, download, notes, alpha_only) = match github::latest_installer(app) {
        Ok(installer) => (Some(installer.version), Some(installer.size), installer.notes, false),
        Err(error) => {
            log::record(&format!("could not read the releases of {}: {error}", app.name));
            (None, None, String::new(), error == github::NoInstaller::OnlyAlpha)
        }
    };
    let size = installed
        .as_ref()
        .and_then(|installed| installed.exe.as_ref())
        .and_then(|exe| exe.parent())
        .and_then(install::folder_size);
    Row {
        name: app.name.into(),
        accent: app.accent,
        installed,
        latest,
        download,
        notes,
        alpha_app: shell::alpha_app_on(app.name),
        alpha_only,
        size,
        languages: app.languages,
        requirements: app.requirements,
        show_notes: app.show_notes,
    }
}

/// Blocking work for one action, recorded in the Hub's log. Runs on a background thread.
fn run(action: Action, name: &str, progress: &Progress) -> Result<String, String> {
    let result = run_action(action, name, progress);
    let verb = match action {
        Action::Install => "install",
        Action::Open => "open",
        Action::Uninstall => "uninstall",
        Action::Rollback => "rollback",
    };
    match &result {
        Ok(_) => log::record(&format!("{verb} {name}: ok")),
        Err(error) => log::record(&format!("{verb} {name}: failed: {error}")),
    }
    result
}

fn run_action(action: Action, name: &str, progress: &Progress) -> Result<String, String> {
    let app = registry::find(name).ok_or(tr("App desconocida", "Unknown app"))?;
    match action {
        Action::Install => {
            let installer = github::latest_installer(app).map_err(|error| error.to_string())?;
            install::install(app, &installer, progress)?;
            Ok(String::new())
        }
        Action::Open => {
            let installed = install::installed(app).ok_or(tr("La app no está instalada", "The app is not installed"))?;
            let exe = installed.exe.ok_or(tr("No encuentro el programa instalado", "I can't find the installed program"))?;
            std::process::Command::new(exe)
                .spawn()
                .map(|_| String::new())
                .map_err(|_| tr("No pude abrir la app", "I couldn't open the app").to_string())
        }
        Action::Uninstall => {
            let installed = install::installed(app).ok_or(tr("La app no está instalada", "The app is not installed"))?;
            install::uninstall(app, &installed)?;
            Ok(String::new())
        }
        Action::Rollback => {
            install::rollback(app)?;
            Ok(String::new())
        }
    }
}

/// A square tile with the app's initials on its accent color. The letter takes the ink with more contrast.
fn monogram(name: &str, color: u32, dim: f32) -> Div {
    div()
        .size(px(dim))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_xl()
        .bg(rgb(color))
        .text_color(rgb(ink_on(color)))
        .font_weight(FontWeight::BOLD)
        .text_size(px((dim * 0.38).max(14.)))
        .child(initials(name))
}

fn chip(label: impl Into<SharedString>, colors: (u32, u32)) -> Div {
    let label: SharedString = label.into();
    div()
        .px_2()
        .py_1()
        .rounded_full()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .bg(rgb(colors.1))
        .text_color(rgb(colors.0))
        .child(label)
}

/// A text action: a discreet link, clickable.
fn link(id: impl Into<ElementId>, label: impl Into<SharedString>, color: u32) -> Stateful<Div> {
    let label: SharedString = label.into();
    div().id(id).text_sm().text_color(rgb(color)).cursor_pointer().child(label)
}

/// A button: filled for the main action, outlined for a secondary one, grey when there is nothing to click.
fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, p: Palette, look: Look) -> Stateful<Div> {
    let label: SharedString = label.into();
    let (bg, fg, border) = match look {
        Look::Filled => (p.primary, p.on_primary, p.primary),
        Look::Outline => (p.surface, p.text, p.border),
        Look::Disabled => (p.surface_2, p.text_muted, p.surface_2),
    };
    let base = div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .rounded_xl()
        .border_1()
        .border_color(rgb(border))
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .font_weight(FontWeight::SEMIBOLD)
        .child(label);
    match look {
        Look::Disabled => base,
        _ => base.cursor_pointer().hover(|style| style.opacity(0.9)),
    }
}

fn section_title(label: impl Into<SharedString>) -> Div {
    let label: SharedString = label.into();
    div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child(label)
}

/// One fact as a line: its name on the left, its value on the right.
fn info_line(label: &'static str, value: impl Into<SharedString>, p: Palette) -> Div {
    let value: SharedString = value.into();
    div()
        .flex()
        .justify_between()
        .gap_4()
        .text_sm()
        .child(div().text_color(rgb(p.text_secondary)).child(label))
        .child(div().text_color(rgb(p.text)).child(value))
}

/// A sort or filter switch of the library; the active one is filled.
fn sort_button(id: &'static str, label: &'static str, active: bool, p: Palette) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .text_xs()
        .cursor_pointer()
        .text_color(rgb(if active { p.on_primary } else { p.text_secondary }))
        .bg(rgb(if active { p.primary } else { p.surface_2 }))
        .child(label)
}

/// A setting as a card: its name and what it does, and its current value as a chip.
fn setting_row(
    p: Palette,
    name: &'static str,
    detail: impl Into<SharedString>,
    value: impl Into<SharedString>,
    on: bool,
) -> Div {
    let detail: SharedString = detail.into();
    div()
        .flex()
        .justify_between()
        .items_center()
        .gap_4()
        .p_3()
        .rounded_lg()
        .bg(rgb(p.surface))
        .border_1()
        .border_color(rgb(p.border))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .min_w_0()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                .child(div().text_sm().text_color(rgb(p.text_secondary)).child(detail)),
        )
        .child(chip(value, if on { p.tone(p.ok) } else { (p.text, p.surface_2) }))
}

/// A language choice: a button that is filled when it is the current language.
fn language_button(p: Palette, id: &'static str, label: &'static str, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .px_4()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .font_weight(FontWeight::SEMIBOLD)
        .bg(rgb(if active { p.primary } else { p.surface_2 }))
        .text_color(rgb(if active { p.on_primary } else { p.text }))
        .hover(|style| style.opacity(0.9))
        .child(label)
}

struct Hub {
    rows: Vec<Row>,
    filter: Filter,
    message: String,
    busy: bool,
    /// Shared with the window's close handler: true keeps the Hub in the tray.
    close_to_tray: Arc<AtomicBool>,
    tray: Option<shell::Tray>,
    /// A newer release of the Hub itself, when GitHub has one.
    self_update: Option<github::Installer>,
    /// The Hub's icon for the sidebar.
    brand: Arc<Image>,
    autostart: bool,
    /// Offer alpha builds (pre-releases) as well as stable ones.
    alpha: bool,
    /// Install releases by themselves once they have waited out the test period.
    auto_update: bool,
    /// The action waiting for its second click, and the app it is for.
    confirming: Option<(String, Action)>,
    /// The install in progress: its app and the live download state.
    transfer: Option<(String, Arc<Progress>)>,
    /// The app whose last install failed; its button offers to try again.
    failed_install: Option<String>,
    screen: Screen,
    /// The cards whose "Detalles" are open.
    expanded: Vec<String>,
    /// "Requisitos y detalles" of the open app, and whether "Ver todo" was pressed for its notes.
    details_open: bool,
    notes_open: bool,
    /// "Avanzado" in Settings.
    advanced_open: bool,
    /// Lists the apps with updates first, then the installed ones; by name otherwise.
    sort_by_status: bool,
    /// Every app's name in the order the grid shows them. Fixed by `reorder`, so cards stay put after an install.
    order: Vec<String>,
    /// Text typed on the library; shows only the apps whose name or description contains it.
    search: String,
    /// Where the library list was scrolled, kept while an app's detail is open.
    scroll: ScrollHandle,
    /// The palette: "system", or one of Excubia's ids. Saved in settings.json.
    palette_id: String,
    /// Keeps the keyboard on the window so typing reaches the search box.
    focus: FocusHandle,
    /// Apps waiting to be updated, in the order they will run.
    queue: VecDeque<String>,
    /// How many apps the queue started with, to number them as they run.
    queue_total: usize,
    queue_paused: bool,
    queue_failed: Vec<String>,
}

/// What the queue does next: start an app, wait while paused, or finish.
enum QueueStep {
    Install(String),
    Wait,
    Done,
}

impl Hub {
    fn new(cx: &mut Context<Self>, close_to_tray: Arc<AtomicBool>) -> Self {
        log::record(&format!("Hub {} started", env!("CARGO_PKG_VERSION")));
        // Without a tray icon the Hub could never come back, so closing must quit.
        let tray = match shell::create_tray() {
            Ok(tray) => Some(tray),
            Err(_) => {
                close_to_tray.store(false, Ordering::SeqCst);
                None
            }
        };
        let mut hub = Self {
            rows: Vec::new(),
            filter: Filter::All,
            message: String::new(),
            busy: false,
            close_to_tray,
            tray,
            self_update: None,
            brand: Arc::new(Image::from_bytes(ImageFormat::Png, BRAND_ICON.to_vec())),
            autostart: shell::autostart_on(),
            alpha: shell::alpha_channel_on(),
            auto_update: shell::auto_update_on(),
            confirming: None,
            transfer: None,
            failed_install: None,
            screen: Screen::Apps,
            expanded: Vec::new(),
            details_open: false,
            notes_open: false,
            advanced_open: false,
            sort_by_status: true,
            order: Vec::new(),
            search: String::new(),
            scroll: ScrollHandle::new(),
            palette_id: shell::palette_saved(),
            focus: cx.focus_handle(),
            queue: VecDeque::new(),
            queue_total: 0,
            queue_paused: false,
            queue_failed: Vec::new(),
        };
        hub.refresh(cx);
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            if this.update(cx, |hub, cx| hub.pump_tray(cx)).is_err() {
                break;
            }
        })
        .detach();
        // The first check comes soon after start, then every couple of hours.
        cx.spawn(async move |_, cx| {
            let mut wait = Duration::from_secs(20);
            let mut first = true;
            loop {
                cx.background_executor().timer(wait).await;
                wait = updates::CHECK_EVERY;
                let startup = std::mem::replace(&mut first, false);
                cx.background_executor().spawn(async move { updates::check_and_announce(startup) }).await;
            }
        })
        .detach();
        hub
    }

    fn pump_tray(&mut self, cx: &mut Context<Self>) {
        // Keeps the download bar moving while an install runs.
        if self.transfer.is_some() {
            cx.notify();
        }
        if shell::take_show_request() {
            shell::show_window();
        }
        let Some(tray) = &self.tray else { return };
        for action in shell::pending(tray) {
            match action {
                shell::TrayAction::Show => shell::show_window(),
                shell::TrayAction::UpdateAll => self.update_all(cx),
                shell::TrayAction::OpenData => {
                    self.open_path(shell::data_dir());
                    cx.notify();
                }
                shell::TrayAction::Quit => {
                    self.quit(cx);
                    return;
                }
            }
        }
    }

    fn quit(&mut self, cx: &mut Context<Self>) {
        // Dropping the tray removes its icon now, not when the process ends.
        self.tray = None;
        cx.quit();
    }

    fn update_self(&mut self, cx: &mut Context<Self>) {
        let Some(installer) = self.self_update.clone() else { return };
        if self.busy {
            return;
        }
        self.busy = true;
        self.message = tr("Actualizando MaxApps Hub…", "Updating MaxApps Hub…").into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { self_update::apply(&installer) })
                .await;
            this.update(cx, |hub, cx| match result {
                Ok(()) => hub.quit(cx),
                Err(error) => {
                    hub.busy = false;
                    hub.message = error;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn toggle_autostart(&mut self, cx: &mut Context<Self>) {
        let next = !self.autostart;
        self.message = match shell::set_autostart(next) {
            Ok(()) => {
                self.autostart = next;
                String::new()
            }
            Err(error) => error,
        };
        cx.notify();
    }

    /// Asks for the folder new apps go to. The dialog runs off the UI thread; apps already installed stay where they are.
    fn choose_install_folder(&mut self, cx: &mut Context<Self>) {
        let start = shell::install_root_saved().or_else(install::apps_root).unwrap_or_default();
        cx.spawn(async move |this, cx| {
            let chosen = cx.background_executor().spawn(async move { shell::pick_folder(&start) }).await;
            this.update(cx, |hub, cx| {
                match chosen {
                    Ok(Some(path)) => {
                        hub.message = match shell::save_install_root(Some(&path)) {
                            Ok(()) => tr("Las apps nuevas se instalarán en la carpeta elegida.", "New apps will install in the chosen folder.").to_string(),
                            Err(error) => error,
                        }
                    }
                    Ok(None) => {}
                    Err(error) => hub.message = error,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn reset_install_folder(&mut self, cx: &mut Context<Self>) {
        self.message = match shell::save_install_root(None) {
            Ok(()) => tr("Las apps nuevas volverán a la carpeta por defecto.", "New apps will go back to the default folder.").to_string(),
            Err(error) => error,
        };
        cx.notify();
    }

    /// Puts the Hub in its own folder with a shortcut and an uninstaller. Copies a large file, so it runs off the UI thread.
    fn install_hub_clicked(&mut self, cx: &mut Context<Self>) {
        if self.busy || system::hub_installed() {
            return;
        }
        self.busy = true;
        self.message = tr("Instalando el acceso directo…", "Adding the shortcut…").to_string();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { system::install_hub() }).await;
            this.update(cx, |hub, cx| {
                hub.busy = false;
                hub.message = match result {
                    Ok(()) => tr("Listo: el Hub está en el menú Inicio.", "Done: the Hub is in the Start menu.").to_string(),
                    Err(error) => error,
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle_auto_update(&mut self, cx: &mut Context<Self>) {
        let next = !self.auto_update;
        self.message = match shell::save_auto_update(next) {
            Ok(()) => {
                self.auto_update = next;
                String::new()
            }
            Err(error) => error,
        };
        cx.notify();
    }

    /// Alphas change which version each app offers, so the list is checked again at once.
    fn toggle_alpha(&mut self, cx: &mut Context<Self>) {
        let next = !self.alpha;
        self.message = match shell::save_alpha_channel(next) {
            Ok(()) => {
                self.alpha = next;
                String::new()
            }
            Err(error) => error,
        };
        self.refresh(cx);
    }

    /// Switches the language and rebuilds the tray menu, which was made in the old one.
    fn set_language(&mut self, english: bool, cx: &mut Context<Self>) {
        self.message = match shell::save_english(english) {
            Ok(()) => {
                i18n::set_english(english);
                if self.tray.is_some() {
                    self.tray = None;
                    self.tray = shell::create_tray().ok();
                }
                String::new()
            }
            Err(error) => error,
        };
        cx.notify();
    }

    /// Switches the palette and saves it. The window repaints in the new colors at once.
    fn set_palette(&mut self, id: &str, cx: &mut Context<Self>) {
        self.message = match shell::save_palette(id) {
            Ok(()) => {
                self.palette_id = id.to_string();
                String::new()
            }
            Err(error) => error,
        };
        cx.notify();
    }

    /// Installs every app that has a newer version, one after the other. Blocking
    /// installs run off the UI thread; the message says which one is going.
    fn update_all(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let names: Vec<String> = self
            .rows
            .iter()
            .filter(|row| has_update(row) && row.installed.as_ref().is_some_and(|installed| !installed.damaged))
            .map(|row| row.name.clone())
            .collect();
        if names.is_empty() {
            self.message = tr("No hay actualizaciones que instalar", "There are no updates to install").into();
            cx.notify();
            return;
        }
        self.queue_total = names.len();
        self.queue = names.into();
        self.queue_paused = false;
        self.queue_failed.clear();
        self.busy = true;
        self.run_queue(cx);
    }

    /// Runs the queue one app at a time. The step is decided on the window's side, so a pause
    /// or a move made while an app installs takes effect before the next one starts.
    fn run_queue(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        cx.spawn(async move |this, cx| loop {
            match this.update(cx, |hub, cx| hub.next_queue_step(cx)) {
                Ok(QueueStep::Install(name)) => {
                    let job = name.clone();
                    let result = cx
                        .background_executor()
                        .spawn(async move { run(Action::Install, &job, &Progress::default()) })
                        .await;
                    if let Err(error) = result {
                        this.update(cx, |hub, _| hub.queue_failed.push(format!("{name}: {error}"))).ok();
                    }
                }
                Ok(QueueStep::Wait) => cx.background_executor().timer(Duration::from_millis(300)).await,
                Ok(QueueStep::Done) | Err(_) => break,
            }
        })
        .detach();
    }

    fn next_queue_step(&mut self, cx: &mut Context<Self>) -> QueueStep {
        if self.queue_paused {
            self.message = tr("En pausa: pulsa Reanudar para seguir", "Paused: press Resume to go on").into();
            cx.notify();
            return QueueStep::Wait;
        }
        match self.queue.pop_front() {
            Some(name) => {
                let position = self.queue_total - self.queue.len();
                self.message = format!("{} {position}/{}: {name}…", tr("Actualizando", "Updating"), self.queue_total);
                cx.notify();
                QueueStep::Install(name)
            }
            None => {
                for failure in &self.queue_failed {
                    log::record(&format!("update failed: {failure}"));
                }
                self.busy = false;
                self.refresh(cx);
                QueueStep::Done
            }
        }
    }

    fn move_in_queue(&mut self, ix: usize, up: bool, cx: &mut Context<Self>) {
        let target = if up { ix.checked_sub(1) } else { Some(ix + 1).filter(|next| *next < self.queue.len()) };
        if let (Some(target), true) = (target, ix < self.queue.len()) {
            self.queue.swap(ix, target);
            cx.notify();
        }
    }

    fn toggle_queue_pause(&mut self, cx: &mut Context<Self>) {
        self.queue_paused = !self.queue_paused;
        cx.notify();
    }

    /// The queue as a list the user can reorder, with pause and resume.
    fn queue_panel(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        if self.queue_total == 0 || !self.busy {
            return div();
        }
        let waiting: Vec<_> = self
            .queue
            .iter()
            .enumerate()
            .map(|(ix, name)| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(name.clone()))
                    .child(
                        div()
                            .id(("queue-up", ix))
                            .px_2()
                            .rounded_md()
                            .bg(rgb(p.surface_2))
                            .cursor_pointer()
                            .child("↑")
                            .on_click(cx.listener(move |this, _, _, cx| this.move_in_queue(ix, true, cx))),
                    )
                    .child(
                        div()
                            .id(("queue-down", ix))
                            .px_2()
                            .rounded_md()
                            .bg(rgb(p.surface_2))
                            .cursor_pointer()
                            .child("↓")
                            .on_click(cx.listener(move |this, _, _, cx| this.move_in_queue(ix, false, cx))),
                    )
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .mb_4()
            .rounded_md()
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(tr("Cola de actualizaciones", "Update queue")))
                    .child(
                        div()
                            .id("queue-pause")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(p.surface_2))
                            .cursor_pointer()
                            .child(if self.queue_paused { tr("Reanudar", "Resume") } else { tr("Pausar", "Pause") })
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_queue_pause(cx))),
                    ),
            )
            .children(waiting)
    }

    /// Alpha builds for one app. Turned off for all apps at once when the global switch is on.
    fn toggle_app_alpha(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.busy || self.alpha {
            return;
        }
        match shell::save_alpha_app(name, !shell::alpha_app_on(name)) {
            Ok(()) => self.refresh(cx),
            Err(error) => {
                self.message = error;
                cx.notify();
            }
        }
    }

    /// Opens a folder or file in Explorer and says why when it cannot.
    fn open_path(&mut self, path: Option<std::path::PathBuf>) {
        self.message = match path {
            Some(path) => shell::open_in_explorer(&path).err().unwrap_or_default(),
            None => tr("No encuentro la carpeta de datos", "I can't find the data folder").into(),
        };
    }

    fn open_log(&mut self, cx: &mut Context<Self>) {
        // Writing one line first makes sure the file exists to be opened.
        log::record("log opened from Settings");
        match log::path() {
            Some(path) => self.open_path(Some(path)),
            None => self.open_path(None),
        }
        cx.notify();
    }

    fn toggle_close_mode(&mut self, cx: &mut Context<Self>) {
        if self.tray.is_none() {
            self.message = tr("Sin segundo plano disponible, cerrar siempre sale de la app", "Background mode is unavailable, closing always quits the app").into();
            cx.notify();
            return;
        }
        let next = !self.close_to_tray.load(Ordering::SeqCst);
        self.close_to_tray.store(next, Ordering::SeqCst);
        self.message = shell::save_close_to_tray(next).err().unwrap_or_default();
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.message = tr("Consultando GitHub…", "Checking GitHub…").into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let (rows, update) = cx
                .background_executor()
                .spawn(async {
                    let rows = registry::APPS.iter().map(status_of).collect::<Vec<_>>();
                    (rows, self_update::available())
                })
                .await;
            this.update(cx, |hub, cx| {
                // Only the first load fixes the order: later checks (after an install) must not move the cards.
                let first = hub.rows.is_empty();
                hub.rows = rows;
                if first {
                    hub.reorder();
                }
                hub.self_update = update;
                hub.busy = false;
                hub.message = String::new();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Fixes the grid's order by the current sort. Called when the user enters the library or presses
    /// a sort button, so the card under the cursor does not change after an install.
    fn reorder(&mut self) {
        let mut all: Vec<&Row> = self.rows.iter().collect();
        sort_rows(&mut all, self.sort_by_status);
        self.order = all.iter().map(|row| row.name.clone()).collect();
    }

    fn act(&mut self, name: String, action: Action, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.failed_install = None;
        self.message = match action {
            Action::Install => format!("{} {name}…", tr("Instalando", "Installing")),
            Action::Open => format!("{} {name}…", tr("Abriendo", "Opening")),
            Action::Uninstall => format!("{} {name}…", tr("Desinstalando", "Uninstalling")),
            Action::Rollback => format!("{} {name}…", tr("Volviendo a la versión anterior de", "Going back to the previous version of")),
        };
        let progress = Arc::new(Progress::default());
        if matches!(action, Action::Install) {
            self.transfer = Some((name.clone(), progress.clone()));
        }
        cx.notify();
        let job = name.clone();
        let watch = progress.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { run(action, &job, &progress) })
                .await;
            this.update(cx, |hub, cx| {
                hub.busy = false;
                hub.transfer = None;
                match result {
                    // A failed install gets the plain sentence and a retry button; the technical text is in the log.
                    Err(_) if matches!(action, Action::Install) && !watch.cancel.load(Ordering::SeqCst) => {
                        hub.failed_install = Some(name);
                        hub.message = tr(
                            "No se pudo instalar. Revisa tu conexión e inténtalo otra vez. El detalle técnico está en el registro de actividad.",
                            "Installation failed. Check your connection and try again. Technical details are in the activity log.",
                        )
                        .into();
                        cx.notify();
                    }
                    Err(error) => {
                        hub.message = error;
                        cx.notify();
                    }
                    Ok(_) if matches!(action, Action::Install | Action::Uninstall | Action::Rollback) => hub.refresh(cx),
                    Ok(_) => {
                        hub.message = String::new();
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// A two-step action: the first click asks for confirmation, the second one runs it.
    fn ask(&mut self, name: String, action: Action, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.confirming.as_ref() == Some(&(name.clone(), action)) {
            self.confirming = None;
            self.act(name, action, cx);
        } else {
            self.confirming = Some((name, action));
            cx.notify();
        }
    }

    fn cta(&mut self, name: String, cta: Cta, cx: &mut Context<Self>) {
        match cta {
            Cta::Act(action) => self.act(name, action, cx),
            Cta::Refresh => {
                if !self.busy {
                    self.refresh(cx);
                }
            }
        }
    }

    fn open_detail(&mut self, name: String, cx: &mut Context<Self>) {
        self.screen = Screen::Detail(name);
        self.details_open = false;
        self.notes_open = false;
        cx.notify();
    }

    fn toggle_card(&mut self, name: String, cx: &mut Context<Self>) {
        if let Some(ix) = self.expanded.iter().position(|open| *open == name) {
            self.expanded.remove(ix);
        } else {
            self.expanded.push(name);
        }
        cx.notify();
    }

    /// The download of this app, while the Hub has one going.
    fn download_of(&self, name: &str) -> Option<&Arc<Progress>> {
        self.transfer.as_ref().filter(|(app, _)| app == name).map(|(_, progress)| progress)
    }

    /// What the card and the detail show for an app: its chip, its support line and its button.
    fn status(&self, row: &Row, p: Palette) -> Status {
        if let Some(progress) = self.download_of(&row.name) {
            let (done, total) = progress_of(progress);
            if download_finished(progress) {
                return Status {
                    chip: tr("Instalando", "Installing"),
                    chip_colors: p.tone(p.info),
                    line: tr("Se está instalando.", "Installing.").into(),
                    line_color: p.text_secondary,
                    button: Button { label: tr("Instalando…", "Installing…").into(), cta: None },
                };
            }
            return Status {
                chip: tr("Descargando", "Downloading"),
                chip_colors: p.tone(p.info),
                line: tr("La descarga sigue en marcha.", "Download in progress.").into(),
                line_color: p.text_secondary,
                button: Button { label: progress_label(done, total), cta: None },
            };
        }
        let installed = row.installed.as_ref();
        if self.failed_install.as_deref() == Some(row.name.as_str()) {
            return Status {
                chip: tr("Error", "Error"),
                chip_colors: p.tone(p.err),
                line: tr(
                    "No se pudo instalar. Revisa tu conexión e inténtalo otra vez. El detalle técnico está en el registro de actividad.",
                    "Installation failed. Check your connection and try again. Technical details are in the activity log.",
                )
                .into(),
                line_color: p.err,
                button: Button { label: tr("Reintentar", "Try again").into(), cta: Some(Cta::Act(Action::Install)) },
            };
        }
        if installed.is_some_and(|installed| installed.damaged) {
            return Status {
                chip: tr("Hay que arreglarla", "Needs a fix"),
                chip_colors: p.tone(p.err),
                line: tr("El programa ya no coincide con la versión instalada.", "The program no longer matches the installed version.").into(),
                line_color: p.err,
                button: Button { label: tr("Arreglar", "Fix").into(), cta: Some(Cta::Act(Action::Install)) },
            };
        }
        match (installed, row.latest.as_ref()) {
            // The latest version lives in "Requisitos y detalles", not under the button.
            (None, Some(_)) => Status {
                chip: tr("No instalada", "Not installed"),
                chip_colors: p.tone(p.info),
                line: String::new(),
                line_color: p.text_secondary,
                button: Button { label: tr("Instalar", "Install").into(), cta: Some(Cta::Act(Action::Install)) },
            },
            (Some(installed), Some(latest)) if github::newer(latest, &installed.version) => Status {
                chip: tr("Hay una versión nueva", "There is a new version"),
                chip_colors: p.tone(p.primary_text),
                line: format!(
                    "{} {latest} {} ({} {})",
                    tr("Versión", "Version"),
                    tr("disponible", "available"),
                    tr("tienes", "you have"),
                    installed.version
                ),
                line_color: p.primary_text,
                button: Button { label: tr("Actualizar", "Update").into(), cta: Some(Cta::Act(Action::Install)) },
            },
            (Some(installed), Some(_)) => Status {
                chip: tr("Al día", "Up to date"),
                chip_colors: p.tone(p.ok),
                line: format!("{} {} · {}", tr("Versión", "Version"), installed.version, tr("al día", "up to date")),
                line_color: p.ok,
                button: Button { label: tr("Abrir", "Open").into(), cta: Some(Cta::Act(Action::Open)) },
            },
            _ if row.alpha_only => Status {
                chip: tr("Solo alfa", "Alpha only"),
                chip_colors: p.tone(p.muted),
                line: github::NoInstaller::OnlyAlpha.to_string(),
                line_color: p.text_secondary,
                button: Button { label: tr("No disponible", "Unavailable").into(), cta: None },
            },
            _ => Status {
                chip: tr("No se pudo comprobar", "Could not check"),
                chip_colors: p.tone(p.muted),
                line: tr("No se pudo comprobar si hay versiones nuevas.", "Could not check for new versions.").into(),
                line_color: p.text_secondary,
                button: Button { label: tr("Reintentar", "Try again").into(), cta: Some(Cta::Refresh) },
            },
        }
    }

    fn nav(&self, filter: Filter, p: Palette, cx: &mut Context<Self>) -> Stateful<Div> {
        let active = self.filter == filter && self.screen != Screen::Settings;
        let count = self.rows.iter().filter(|row| filter.matches(row)).count();
        div()
            .id(("nav", filter as usize))
            .px_3()
            .py_2()
            .rounded_md()
            .cursor_pointer()
            .bg(rgb(if active { p.primary_soft } else { p.surface }))
            .text_color(rgb(if active { p.text } else { p.text_secondary }))
            .hover(|style| style.bg(rgb(p.surface_2)))
            .child(format!("{} ({count})", filter.title()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.filter = filter;
                this.screen = Screen::Apps;
                this.reorder();
                cx.notify();
            }))
    }

    /// The sidebar on wide windows, a row of tabs on narrow ones: the brand, the filters and Settings.
    fn sidebar(&self, p: Palette, wide: bool, cx: &mut Context<Self>) -> Div {
        let brand = div()
            .flex()
            .items_center()
            .gap_3()
            .child(img(self.brand.clone()).size(px(36.)).rounded_xl())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().font_weight(FontWeight::BOLD).child("MaxApps Hub")),
            );
        let settings_on = self.screen == Screen::Settings;
        let settings = div()
            .id("settings")
            .px_3()
            .py_2()
            .rounded_md()
            .cursor_pointer()
            .text_color(rgb(if settings_on { p.text } else { p.text_secondary }))
            .bg(rgb(if settings_on { p.primary_soft } else { p.surface }))
            .hover(|style| style.bg(rgb(p.surface_2)))
            .child(tr("Ajustes", "Settings"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.screen = Screen::Settings;
                cx.notify();
            }));
        let bar = div().flex().flex_none().gap_1().bg(rgb(p.surface)).border_color(rgb(p.border));
        if wide {
            bar.flex_col()
                .w(px(232.))
                .h_full()
                .p_4()
                .border_r_1()
                .child(brand.mb_6())
                .child(self.nav(Filter::All, p, cx))
                .child(self.nav(Filter::Installed, p, cx))
                .child(self.nav(Filter::Updates, p, cx))
                .child(div().flex_1())
                .child(settings)
        } else {
            bar.flex_row()
                .flex_wrap()
                .items_center()
                .p_2()
                .border_b_1()
                .child(brand.mr_4())
                .child(self.nav(Filter::All, p, cx))
                .child(self.nav(Filter::Installed, p, cx))
                .child(self.nav(Filter::Updates, p, cx))
                .child(settings)
        }
    }

    fn search_box(&self, p: Palette) -> Stateful<Div> {
        let text = if self.search.is_empty() {
            div().text_color(rgb(p.text_muted)).child(tr("Buscar apps", "Search apps"))
        } else {
            div().child(self.search.clone())
        };
        div()
            .id("search")
            .px_3()
            .py_2()
            .min_w(px(220.))
            .rounded_md()
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(if self.search.is_empty() { p.border } else { p.border_strong }))
            .child(text)
    }

    /// The alpha setting of an app as a line. «Cambiar» toggles it, unless the general switch decides.
    fn alpha_line(&self, id: impl Into<ElementId>, row: &Row, p: Palette, cx: &mut Context<Self>) -> Div {
        let (label, can_change) = if self.alpha {
            (tr("Versiones alfa: sí, por el ajuste general", "Alpha versions: on, from the general setting"), false)
        } else if row.alpha_app {
            (tr("Versiones alfa: sí", "Alpha versions: on"), true)
        } else {
            (tr("Versiones alfa: no", "Alpha versions: off"), true)
        };
        let line = div().flex().items_center().gap_2().text_sm().text_color(rgb(p.text_secondary)).child(label);
        if !can_change {
            return line;
        }
        let name = row.name.clone();
        line.child(
            link(id, tr("Cambiar", "Change"), p.primary_text)
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_app_alpha(&name, cx))),
        )
    }

    /// A two-step text action. Its label changes while it waits for the second click.
    #[allow(clippy::too_many_arguments)]
    fn ask_link(
        &self,
        id: impl Into<ElementId>,
        name: &str,
        action: Action,
        idle: &'static str,
        armed: &'static str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let asking = self.confirming.as_ref().is_some_and(|(app, step)| app == name && *step == action);
        let name = name.to_string();
        link(id, if asking { armed } else { idle }, if asking { p.err } else { p.text_secondary })
            .on_click(cx.listener(move |this, _, _, cx| this.ask(name.clone(), action, cx)))
    }

    /// The download bar of an app, with Cancel. Its fill uses the app's accent.
    fn download_block(&self, ix: usize, progress: &Arc<Progress>, accent: u32, p: Palette, cx: &mut Context<Self>) -> Div {
        let (done, total) = progress_of(progress);
        let fraction = if total > 0 { (done as f32 / total as f32).min(1.0) } else { 0.0 };
        let cancel = progress.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().text_color(rgb(p.text)).child(progress_label(done, total)))
            .child(
                div()
                    .h(px(6.))
                    .w_full()
                    .rounded_full()
                    .bg(rgb(p.surface_3))
                    .child(div().h_full().rounded_full().bg(rgb(accent)).w(relative(fraction))),
            )
            .child(
                link(("cancel", ix), tr("Cancelar", "Cancel"), p.text_secondary)
                    .on_click(cx.listener(move |_, _, _, _| cancel.cancel.store(true, Ordering::SeqCst))),
            )
    }

    /// One app in the library: icon, name, phrase, state, one button. «Detalles» opens the rest.
    fn card(&self, ix: usize, row: &Row, p: Palette, cx: &mut Context<Self>) -> Div {
        let st = self.status(row, p);
        let description = registry::find(&row.name).map_or("", |app| app.description());
        let open = row.name.clone();
        // The icon, the name and the phrase open the app's detail.
        let opener = div()
            .id(("open", ix))
            .flex()
            .flex_col()
            .gap_3()
            .cursor_pointer()
            .child(monogram(&row.name, row.accent, 64.))
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(row.name.clone()),
            )
            .child(div().text_sm().text_color(rgb(p.text_secondary)).line_clamp(2).child(description))
            .on_click(cx.listener(move |this, _, _, cx| this.open_detail(open.clone(), cx)));
        let mut card = div()
            .w(px(268.))
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_2xl()
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.border))
            .child(opener)
            .child(chip(st.chip, st.chip_colors));
        card = match self.download_of(&row.name) {
            Some(progress) if !download_finished(progress) => {
                card.child(self.download_block(ix, progress, row.accent, p, cx))
            }
            _ => {
                let name = row.name.clone();
                let look = if st.button.cta.is_some() { Look::Outline } else { Look::Disabled };
                let action = button(("action", ix), st.button.label, p, look).h(px(36.)).w_full();
                let action = match st.button.cta {
                    Some(cta) => action.on_click(cx.listener(move |this, _, _, cx| this.cta(name.clone(), cta, cx))),
                    None => action,
                };
                card.child(action)
            }
        };
        let open_details = self.expanded.contains(&row.name);
        let toggle = row.name.clone();
        card = card.child(
            link(
                ("details", ix),
                if open_details { tr("Ocultar detalles", "Hide details") } else { tr("Detalles", "Details") },
                p.text_secondary,
            )
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_card(toggle.clone(), cx))),
        );
        if open_details {
            card = card.child(self.card_details(ix, row, p, cx));
        }
        card
    }

    /// The «Detalles» of a card: versions, size, alpha, and the two two-step actions.
    fn card_details(&self, ix: usize, row: &Row, p: Palette, cx: &mut Context<Self>) -> Div {
        let mut block = div().flex().flex_col().gap_2().pt_3().border_t_1().border_color(rgb(p.border));
        if let Some(installed) = &row.installed {
            block = block.child(info_line(tr("Versión instalada", "Installed version"), installed.version.clone(), p));
        }
        if let Some(latest) = &row.latest {
            block = block.child(info_line(tr("Última versión", "Latest version"), latest.clone(), p));
        }
        if let Some(bytes) = row.size {
            block = block.child(info_line(tr("Tamaño", "Size"), install::human_size(bytes), p));
        }
        block = block.child(self.alpha_line(("alpha-app", ix), row, p, cx));
        if row.installed.as_ref().is_some_and(|installed| installed.previous.is_some()) {
            block = block.child(self.ask_link(
                ("rollback", ix),
                &row.name,
                Action::Rollback,
                tr("Volver a la versión anterior", "Go back to the previous version"),
                tr("Confirmar volver atrás", "Confirm going back"),
                p,
                cx,
            ));
        }
        if row.installed.is_some() {
            block = block.child(self.ask_link(
                ("uninstall", ix),
                &row.name,
                Action::Uninstall,
                tr("Desinstalar", "Uninstall"),
                tr("Confirmar desinstalar", "Confirm uninstall"),
                p,
                cx,
            ));
        }
        block
    }

    /// The library: header with search, order and the update buttons, the queue, and the grid of cards.
    fn library(&self, p: Palette, pad: Pixels, cx: &mut Context<Self>) -> Div {
        let needle = self.search.to_lowercase();
        let mut visible: Vec<&Row> = self
            .rows
            .iter()
            .filter(|row| self.filter.matches(row) && matches_search(row, &needle))
            .collect();
        visible.sort_by_key(|row| self.order.iter().position(|name| *name == row.name).unwrap_or(usize::MAX));
        let count = visible.len();
        let cards: Vec<Div> = visible
            .into_iter()
            .enumerate()
            .map(|(ix, row)| self.card(ix, row, p, cx))
            .collect();
        let pending = self.rows.iter().filter(|row| has_update(row)).count();

        let update_all = (pending > 0).then(|| {
            button(
                ("update-all", 0usize),
                format!("{} ({pending})", tr("Actualizar todo", "Update all")),
                p,
                Look::Filled,
            )
            .h(px(36.))
            .on_click(cx.listener(|this, _, _, cx| this.update_all(cx)))
        });
        let header = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_4()
            .mb_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_size(px(28.)).font_weight(FontWeight::BOLD).child(self.filter.title())),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(self.search_box(p))
                    .child(
                        sort_button("sort-status", tr("Por estado", "By status"), self.sort_by_status, p).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.sort_by_status = true;
                                this.reorder();
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        sort_button("sort-name", tr("Por nombre", "By name"), !self.sort_by_status, p).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.sort_by_status = false;
                                this.reorder();
                                cx.notify();
                            }),
                        ),
                    )
                    .children(update_all)
                    .child(
                        button(("refresh", 0usize),tr("Buscar actualizaciones", "Check for updates"), p, Look::Outline)
                            .h(px(36.))
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            );
        let update_line = self.self_update.as_ref().map(|update| {
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .mb_4()
                .text_sm()
                .text_color(rgb(p.text_secondary))
                .child(format!(
                    "{} v{}",
                    tr("Hay una versión nueva de MaxApps Hub:", "A new version of MaxApps Hub is out:"),
                    update.version
                ))
                .child(
                    link("self-update", tr("Actualizar", "Update"), p.primary_text)
                        .on_click(cx.listener(|this, _, _, cx| this.update_self(cx))),
                )
        });
        let grid = if count == 0 {
            div()
                .text_color(rgb(p.text_secondary))
                .child(tr("No hay apps en esta vista.", "No apps in this view."))
                .into_any_element()
        } else {
            div().flex().flex_wrap().gap_4().children(cards).into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .p(pad)
            .child(header)
            .children(update_line)
            .child(self.queue_panel(p, cx))
            .child(
                div()
                    .id("library")
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .flex_1()
                    .child(grid),
            )
            .child(div().pt_4().text_sm().text_color(rgb(p.text_secondary)).child(self.message.clone()))
    }

    /// One app's page: the main button first, then the notes and the requirements folded away.
    fn detail(&self, row: &Row, p: Palette, wide: bool, pad: Pixels, cx: &mut Context<Self>) -> Stateful<Div> {
        let st = self.status(row, p);
        let description = registry::find(&row.name).map_or("", |app| app.description());
        let back = link("back", format!("← {}", tr("Todas las apps", "All apps")), p.text_secondary).on_click(cx.listener(
            |this, _, _, cx| {
                this.screen = Screen::Apps;
                this.reorder();
                cx.notify();
            },
        ));
        let header = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(monogram(&row.name, row.accent, 128.))
            .child(div().text_size(px(28.)).font_weight(FontWeight::BOLD).child(row.name.clone()))
            .child(div().text_sm().text_color(rgb(p.text_secondary)).child(description))
            .child(chip(st.chip, st.chip_colors));

        // The main button spans the side column; the support line sits under it.
        let look = if st.button.cta.is_some() { Look::Filled } else { Look::Disabled };
        let cta_name = row.name.clone();
        let main_button = button(("main", 0usize),st.button.label, p, look).h(px(48.)).w_full();
        let main_button = match st.button.cta {
            Some(cta) => main_button.on_click(cx.listener(move |this, _, _, cx| this.cta(cta_name.clone(), cta, cx))),
            None => main_button,
        };
        let mut side = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(header)
            .child(main_button);
        if !st.line.is_empty() {
            side = side.child(div().text_sm().text_color(rgb(st.line_color)).child(st.line));
        }
        if row.installed.is_some() {
            side = side.child(self.ask_link(
                ("d-uninstall", 0usize),
                &row.name,
                Action::Uninstall,
                tr("Desinstalar", "Uninstall"),
                tr("Confirmar desinstalar", "Confirm uninstall"),
                p,
                cx,
            ));
        }
        if row.installed.as_ref().is_some_and(|installed| installed.previous.is_some()) {
            side = side.child(self.ask_link(
                ("d-rollback", 0usize),
                &row.name,
                Action::Rollback,
                tr("Volver a la versión anterior", "Go back to the previous version"),
                tr("Confirmar volver atrás", "Confirm going back"),
                p,
                cx,
            ));
        }

        // Screenshots and "Qué hace" wait for their data, so they are not drawn yet.
        let mut main = div().flex().flex_col().gap_8().flex_1().min_w_0();
        let (notes, technical): (Vec<String>, Vec<String>) = whats_new(&row.notes).into_iter().partition(|line| !is_technical(line));
        // Hidden notes stay hidden everywhere on the page, including the technical lines under the details.
        let technical = if row.show_notes { technical } else { Vec::new() };
        if row.show_notes && !notes.is_empty() {
            let shown = if self.notes_open { notes.len() } else { WHATS_NEW_LINES };
            let mut list = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(section_title(tr("Qué hay de nuevo", "What's new")));
            for line in notes.iter().take(shown) {
                list = list.child(div().flex().gap_2().text_sm().child("•").child(line.clone()));
            }
            if notes.len() > shown {
                list = list.child(link(("notes-all", 0usize),tr("Ver todo", "See all"), p.primary_text).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.notes_open = true;
                        cx.notify();
                    }),
                ));
            }
            main = main.child(list);
        }

        let arrow = if self.details_open { "▾" } else { "▸" };
        let requirements_title = div()
            .id(("req-toggle", 0usize))
            .cursor_pointer()
            .child(section_title(format!("{arrow} {}", tr("Requisitos y detalles", "Requirements and details"))))
            .on_click(cx.listener(|this, _, _, cx| {
                this.details_open = !this.details_open;
                cx.notify();
            }));
        let mut requirements = div().flex().flex_col().gap_3().child(requirements_title);
        if self.details_open {
            let installed_text = row
                .installed
                .as_ref()
                .map_or_else(|| tr("No instalada", "Not installed").to_string(), |installed| installed.version.clone());
            let latest_text = row.latest.clone().unwrap_or_else(|| tr("Sin datos", "No data").to_string());
            let mut facts = div().flex().flex_col().gap_2().p_4().rounded_2xl().bg(rgb(p.surface)).border_1().border_color(rgb(p.border));
            if let Some(bytes) = row.download {
                facts = facts.child(info_line(tr("Tamaño de la descarga", "Download size"), install::human_size(bytes), p));
            }
            facts = facts
                .child(info_line(tr("Versión instalada", "Installed version"), installed_text, p))
                .child(info_line(tr("Última versión", "Latest version"), latest_text, p))
                .child(info_line(tr("Se guarda en", "Saved in"), folder_label(), p))
                .child(self.alpha_line(("d-alpha", 0usize), row, p, cx))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.text_secondary))
                        .child(tr("Registro de actividad: Ajustes > Avanzado", "Activity log: Settings > Advanced")),
                );
            if !row.languages.is_empty() {
                facts = facts.child(info_line(tr("Idiomas", "Languages"), row.languages.join(", "), p));
            }
            if !row.requirements.is_empty() {
                facts = facts.child(info_line(tr("Requisitos", "Requirements"), row.requirements.to_string(), p));
            }
            for line in &technical {
                facts = facts.child(div().text_sm().text_color(rgb(p.text_secondary)).child(line.clone()));
            }
            requirements = requirements.child(facts);
        }
        main = main.child(requirements);

        // Wide: the main column on the left and the 340 px side column with the button. Narrow: one column, header first.
        let body = if wide {
            div()
                .flex()
                .flex_row()
                .gap_8()
                .items_start()
                .child(main)
                .child(side.w(px(340.)).flex_none())
        } else {
            div().flex().flex_col().gap_8().child(side).child(main)
        };
        div()
            .id("detail")
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .flex_1()
            .p(pad)
            .gap_6()
            .child(back)
            .child(body)
    }

    /// «Apariencia»: the palette selector, one row per palette in Excubia's order. The one in use is marked.
    fn appearance_block(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let in_use = palette::preset(&self.palette_id).id;
        let rows: Vec<Stateful<Div>> = palette::PRESETS
            .iter()
            .enumerate()
            .map(|(ix, preset)| {
                let active = preset.id == in_use;
                let id = preset.id;
                div()
                    .id(("palette", ix))
                    .flex()
                    .justify_between()
                    .items_center()
                    .gap_4()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(p.surface))
                    .border_1()
                    .border_color(rgb(if active { p.primary } else { p.border }))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(p.surface_2)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .min_w_0()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(preset.name()))
                            .child(div().text_sm().text_color(rgb(p.text_secondary)).child(preset.note())),
                    )
                    .children(active.then(|| chip(tr("En uso", "In use"), p.tone(p.ok))))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_palette(id, cx)))
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(tr("Apariencia", "Appearance")))
            .child(div().text_sm().text_color(rgb(p.text_secondary)).child(tr("Paleta de colores", "Color palette")))
            .children(rows)
    }

    fn settings_panel(&self, p: Palette, cx: &mut Context<Self>) -> Stateful<Div> {
        let english = i18n::english();
        let yes_no = |on: bool| if on { tr("Sí", "On") } else { tr("No", "Off") };
        let close = if self.tray.is_none() {
            tr("Salir de la app", "Quit the app")
        } else if self.close_to_tray.load(Ordering::SeqCst) {
            tr("Seguir abierta en segundo plano", "Keep open in background")
        } else {
            tr("Salir del todo", "Quit")
        };
        let folder_detail = if shell::install_root_saved().is_some() {
            tr(
                "Las apps nuevas se instalan aquí. Las ya instaladas se quedan donde están.",
                "New apps install here. Apps already installed stay where they are.",
            )
        } else {
            tr(
                "Por defecto, dentro de tu usuario. Las apps ya instaladas se quedan donde están.",
                "Default, inside your user folder. Apps already installed stay where they are.",
            )
        };
        let hub_installed = system::hub_installed();
        // Scrolls when the window is shorter than the list of settings.
        let mut panel = div()
            .id("settings-panel")
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .flex_1()
            .p_6()
            .gap_3()
            .child(div().text_size(px(28.)).font_weight(FontWeight::BOLD).mb_2().child(tr("Ajustes", "Settings")))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(p.surface))
                    .border_1()
                    .border_color(rgb(p.border))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(tr("Idioma", "Language")))
                    .child(div().text_sm().text_color(rgb(p.text_secondary)).child(tr(
                        "Cambia los textos de la app.",
                        "Changes the app's text.",
                    )))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .mt_1()
                            .child(language_button(p, "lang-es", "Español", !english).on_click(cx.listener(
                                |this, _, _, cx| this.set_language(false, cx),
                            )))
                            .child(language_button(p, "lang-en", "English", english).on_click(cx.listener(
                                |this, _, _, cx| this.set_language(true, cx),
                            ))),
                    ),
            )
            .child(self.appearance_block(p, cx))
            .child(
                setting_row(
                    p,
                    tr("Iniciar con Windows", "Start with Windows"),
                    tr("Se abre en segundo plano al encender el PC", "Opens in the background when the PC starts"),
                    yes_no(self.autostart),
                    self.autostart,
                )
                .id("set-autostart")
                .cursor_pointer()
                .hover(|style| style.bg(rgb(p.surface_2)))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_autostart(cx))),
            )
            .child(
                setting_row(
                    p,
                    tr("Al cerrar la ventana", "When the window is closed"),
                    tr(
                        "Seguir abierta en segundo plano deja la app funcionando; salir del todo la cierra.",
                        "Keep open in background leaves the app running; quit closes it.",
                    ),
                    close,
                    self.tray.is_some() && self.close_to_tray.load(Ordering::SeqCst),
                )
                .id("set-close")
                .cursor_pointer()
                .hover(|style| style.bg(rgb(p.surface_2)))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_close_mode(cx))),
            )
            .child(
                setting_row(
                    p,
                    tr("Actualizar solo", "Update by itself"),
                    tr(
                        "Instala una versión sola cuando lleva 7 días publicada, para dar tiempo a detectar problemas.",
                        "Installs a release by itself once it has been out for 7 days, to give time to catch problems.",
                    ),
                    yes_no(self.auto_update),
                    self.auto_update,
                )
                .id("set-auto-update")
                .cursor_pointer()
                .hover(|style| style.bg(rgb(p.surface_2)))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_auto_update(cx))),
            )
            .child(
                setting_row(
                    p,
                    tr("Avanzado", "Advanced"),
                    "",
                    if self.advanced_open { tr("Ocultar", "Hide") } else { tr("Mostrar", "Show") },
                    false,
                )
                .id("set-advanced")
                .cursor_pointer()
                .hover(|style| style.bg(rgb(p.surface_2)))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.advanced_open = !this.advanced_open;
                    cx.notify();
                })),
            );
        if self.advanced_open {
            panel = panel
                .child(
                    setting_row(
                        p,
                        tr("Versiones alfa", "Alpha versions"),
                        tr(
                            "Ofrece versiones de prueba antes de la publicación normal. Pueden tener fallos.",
                            "Offers test builds before the normal release. They may have bugs.",
                        ),
                        yes_no(self.alpha),
                        self.alpha,
                    )
                    .id("set-alpha")
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(p.surface_2)))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_alpha(cx))),
                )
                .child(
                    setting_row(
                        p,
                        tr("Menú Inicio", "Start menu"),
                        tr(
                            "Pone el Hub en el menú Inicio y en Aplicaciones instaladas, para quitarlo desde Windows.",
                            "Puts the Hub in the Start menu and in Installed apps, so Windows can remove it.",
                        ),
                        if hub_installed { tr("Instalado", "Installed") } else { tr("Instalar", "Install") },
                        hub_installed,
                    )
                    .id("set-hub-shortcut")
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(p.surface_2)))
                    .on_click(cx.listener(|this, _, _, cx| this.install_hub_clicked(cx))),
                )
                .child(
                    setting_row(
                        p,
                        tr("Carpeta de instalación", "Install folder"),
                        folder_detail,
                        folder_label(),
                        false,
                    )
                    .id("set-install-folder")
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(p.surface_2)))
                    .on_click(cx.listener(|this, _, _, cx| this.choose_install_folder(cx))),
                )
                .children(shell::install_root_saved().is_some().then(|| {
                    setting_row(
                        p,
                        tr("Volver a la carpeta por defecto", "Back to the default folder"),
                        tr(
                            "Las apps nuevas se instalarán otra vez en la carpeta por defecto.",
                            "New apps will install in the default folder again.",
                        ),
                        tr("Restablecer", "Reset"),
                        false,
                    )
                    .id("set-install-folder-reset")
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(p.surface_2)))
                    .on_click(cx.listener(|this, _, _, cx| this.reset_install_folder(cx)))
                }))
                .child(setting_row(
                    p,
                    tr("Ruta completa", "Full folder path"),
                    tr("Dónde se guardan las apps nuevas.", "Where new apps are saved."),
                    full_folder(),
                    false,
                ))
                .child(
                    setting_row(
                        p,
                        tr("Registro de actividad", "Activity log"),
                        tr(
                            "Lo que hizo el Hub y los errores, para pedir ayuda. No sale de tu PC.",
                            "What the Hub did and the errors, to ask for help. It stays on your PC.",
                        ),
                        tr("Abrir", "Open"),
                        true,
                    )
                    .id("set-log")
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(p.surface_2)))
                    .on_click(cx.listener(|this, _, _, cx| this.open_log(cx))),
                );
        }
        panel
            .child(div().mt_auto().text_xs().text_color(rgb(p.text_muted)).child(format!(
                "MaxApps Hub {} · {}",
                env!("CARGO_PKG_VERSION"),
                if english { "English" } else { "Español" }
            )))
            .child(div().text_sm().text_color(rgb(p.text_secondary)).child(self.message.clone()))
    }
}

impl Render for Hub {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Keys go to the window's focus so the search box receives what is typed.
        if !self.focus.is_focused(window) {
            window.focus(&self.focus);
        }
        let p = palette::resolve(&self.palette_id, shell::system_look());
        let wide = window.viewport_size().width >= px(1000.);
        let pad = if wide { px(40.) } else { px(16.) };
        let detail_row = match &self.screen {
            Screen::Detail(name) => self.rows.iter().find(|row| row.name == *name),
            _ => None,
        };
        let sidebar = self.sidebar(p, wide, cx);
        let panel = if self.screen == Screen::Settings {
            self.settings_panel(p, cx).into_any_element()
        } else if let Some(row) = detail_row {
            self.detail(row, p, wide, pad, cx).into_any_element()
        } else {
            self.library(p, pad, cx).into_any_element()
        };

        let root = div()
            .flex()
            .size_full()
            .bg(rgb(p.bg))
            .text_color(rgb(p.text))
            .text_sm()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let keys = &event.keystroke;
                if this.screen != Screen::Apps || keys.modifiers.control || keys.modifiers.platform {
                    return;
                }
                match keys.key.as_str() {
                    "escape" => this.search.clear(),
                    "backspace" => {
                        this.search.pop();
                    }
                    _ => {
                        if let Some(text) = keys.key_char.as_deref().filter(|text| text.chars().all(|c| !c.is_control())) {
                            this.search.push_str(text);
                        }
                    }
                }
                cx.notify();
            }));
        let root = if wide { root.flex_row() } else { root.flex_col() };
        root.child(sidebar).child(panel)
    }
}

/// A Windows error box with `text`: the only way to tell the user why the window did not open.
fn show_error_box(text: &str) {
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (text, title) = (wide(text), wide("MaxApps Hub"));
    // SAFETY: both buffers are NUL-terminated and live until the call returns; no owner window.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_ICONERROR);
    }
}

fn main() {
    // Started by "Installed apps" in a copy kept beside a program: remove it, without opening the window.
    // Checked before the single-instance claim, so it also runs while the Hub is open.
    if std::env::args().any(|arg| arg == "--uninstall") {
        if let Err(error) = system::uninstall_self() {
            log::record(&format!("uninstall: failed: {error}"));
        }
        return;
    }
    if matches!(shell::claim_instance(), shell::Launch::Second) {
        return;
    }
    self_update::clean_previous();
    shell::register_app_id();
    // Before the window and the tray exist, so both come up in the saved language.
    i18n::set_english(shell::english_on());
    // Started at logon: stay in the tray instead of opening the window.
    let background = std::env::args().any(|arg| arg == "--background");
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(960.), px(620.)), cx);
        let close_to_tray = Arc::new(AtomicBool::new(shell::close_to_tray_saved()));
        cx.open_window(
            WindowOptions {
                show: !background,
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("MaxApps Hub".into()),
                    ..Default::default()
                }),
                window_min_size: Some(size(px(720.), px(480.))),
                ..Default::default()
            },
            move |window, cx| {
                shell::remember_window(window);
                let flag = close_to_tray.clone();
                window.on_window_should_close(cx, move |_, _| {
                    if flag.load(Ordering::SeqCst) {
                        shell::hide_window();
                        false
                    } else {
                        true
                    }
                });
                cx.new(|cx| Hub::new(cx, close_to_tray))
            },
        )
        .unwrap_or_else(|error| {
            // A release build has no console: without this box the Hub would just vanish.
            let text = format!(
                "{}\n\n{} {error}",
                tr(
                    "MaxApps Hub no pudo abrir su ventana. Suele deberse al controlador gráfico (DirectX): actualiza el controlador de la tarjeta gráfica y vuelve a abrir el Hub.",
                    "MaxApps Hub could not open its window. This is usually the graphics driver (DirectX): update the graphics card driver and open the Hub again.",
                ),
                tr("Detalle:", "Details:"),
            );
            show_error_box(&text);
            std::process::exit(2);
        });
        if !background {
            cx.activate(true);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, installed: Option<&str>, latest: Option<&str>) -> Row {
        Row {
            name: name.into(),
            accent: 0,
            installed: installed.map(|version| Installed {
                version: version.into(),
                exe: None,
                uninstall: None,
                damaged: false,
                previous: None,
            }),
            latest: latest.map(String::from),
            download: None,
            notes: String::new(),
            alpha_app: false,
            alpha_only: false,
            size: None,
            languages: &[],
            requirements: "",
            show_notes: false,
        }
    }

    #[test]
    fn by_status_lists_updates_then_installed_then_the_rest_each_by_name() {
        let rows = [
            row("Zeta", None, Some("1.0")),
            row("Beta", Some("1.0"), Some("1.0")),
            row("Gamma", Some("1.0"), Some("2.0")),
            row("Alfa", Some("1.0"), Some("1.0")),
            row("Delta", Some("1.0"), Some("2.0")),
        ];
        let mut refs: Vec<&Row> = rows.iter().collect();
        sort_rows(&mut refs, true);
        let names: Vec<&str> = refs.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["Delta", "Gamma", "Alfa", "Beta", "Zeta"]);
    }

    #[test]
    fn by_name_ignores_status() {
        let rows = [row("Zeta", None, Some("1.0")), row("Gamma", Some("1.0"), Some("2.0")), row("Alfa", None, None)];
        let mut refs: Vec<&Row> = rows.iter().collect();
        sort_rows(&mut refs, false);
        let names: Vec<&str> = refs.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["Alfa", "Gamma", "Zeta"]);
    }

    #[test]
    fn initials_come_from_the_first_two_words() {
        assert_eq!(initials("Excubia"), "E");
        assert_eq!(initials("Download Lovable"), "DL");
        assert_eq!(initials("ARK ASA Config Manager"), "AA");
    }

    #[test]
    fn whats_new_is_plain_lines_without_markdown() {
        let notes = "# Novedades\n- uno\n- **dos**\n\n* tres `x`";
        assert_eq!(whats_new(notes), ["Novedades", "uno", "dos", "tres x"]);
    }

    #[test]
    fn installer_and_build_lines_are_technical() {
        assert!(is_technical("Compilado desde el commit 1a2b"));
        assert!(is_technical("SHA-256 de prueba: 9f8e"));
        assert!(is_technical("Instalación silenciosa con /S"));
        assert!(!is_technical("Arregla el arranque al encender el PC"));
        assert!(!is_technical("Mejora la lista de apps"));
    }

    #[test]
    fn app_header_pairing_and_install_command_lines_are_technical() {
        assert!(is_technical("Escritorio Excubia 2.3.910 (estable)."));
        assert!(is_technical("Versión de prueba 2.4.0 (alfa)."));
        assert!(is_technical("Emparejamiento: usa el mismo host que Tailscale."));
        assert!(is_technical("irm https://example.com/instalar.ps1 | iex"));
        assert!(!is_technical("Arregla el arranque al encender el PC"));
    }
}
