#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod github;
mod i18n;
mod install;
mod log;
mod registry;
mod self_update;
mod shell;
mod signing;
mod updates;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    div, prelude::*, px, relative, rgb, size, App, Application, Bounds, Context, Div, FocusHandle,
    FontWeight, KeyDownEvent, Render, Stateful, TitlebarOptions, Window, WindowBounds, WindowOptions,
};

use i18n::tr;
use install::{Installed, Progress};

/// What the window shows: the app list or the settings.
#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Apps,
    Settings,
}

/// One app as the window shows it.
#[derive(Clone)]
struct Row {
    name: String,
    accent: u32,
    installed: Option<Installed>,
    latest: Option<String>,
    /// The release notes of the latest version, read before the user updates.
    notes: String,
    /// Alpha builds are offered for this app alone.
    alpha_app: bool,
    error: Option<String>,
    /// The bytes the installed program takes up, counted in the background when the list loads.
    size: Option<u64>,
}

#[derive(Clone, Copy)]
enum Action {
    Install,
    Open,
    Uninstall,
    Rollback,
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

fn status_of(app: &registry::DesktopApp) -> Row {
    let installed = install::installed(app);
    let (latest, notes, error) = match github::latest_installer(app) {
        Ok(installer) => (Some(installer.version), installer.notes, None),
        Err(error) => {
            log::record(&format!("could not read the releases of {}: {error}", app.name));
            (None, String::new(), Some(error))
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
        notes,
        alpha_app: shell::alpha_app_on(app.name),
        error,
        size,
    }
}

/// The first part of the release notes, on one line and without Markdown marks, for the card.
fn excerpt(notes: &str) -> String {
    const LIMIT: usize = 160;
    let plain = notes.replace(|c: char| matches!(c, '#' | '*' | '`'), " ");
    let text = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= LIMIT {
        text
    } else {
        format!("{}…", text.chars().take(LIMIT).collect::<String>())
    }
}

/// The label, colors, detail and button of a card.
struct Status {
    label: &'static str,
    detail: String,
    fg: u32,
    bg: u32,
    button: Option<(&'static str, Action)>,
}

/// Adds the installed size to a detail line, when it is known.
fn with_size(detail: String, row: &Row) -> String {
    match row.size {
        Some(bytes) => format!("{detail} · {}", install::human_size(bytes)),
        None => detail,
    }
}

fn status(row: &Row) -> Status {
    if let Some(error) = &row.error {
        return Status { label: tr("Error", "Error"), detail: error.clone(), fg: 0xfca5a5, bg: 0x3b1515, button: None };
    }
    if let Some(installed) = row.installed.as_ref().filter(|installed| installed.damaged) {
        return Status {
            label: tr("Dañada", "Damaged"),
            detail: format!(
                "{} {}",
                tr("El programa ya no coincide con la versión", "The program no longer matches version"),
                installed.version
            ),
            fg: 0xfca5a5,
            bg: 0x3b1515,
            button: Some((tr("Reparar", "Repair"), Action::Install)),
        };
    }
    match (&row.installed, &row.latest) {
        (None, Some(latest)) => Status {
            label: tr("No instalada", "Not installed"),
            detail: format!("{} {latest}", tr("Última versión", "Latest version")),
            fg: 0xcbd5e1,
            bg: 0x1e293b,
            button: Some((tr("Instalar", "Install"), Action::Install)),
        },
        (Some(installed), Some(latest)) if github::newer(latest, &installed.version) => Status {
            label: tr("Actualización", "Update"),
            detail: with_size(format!("{} → {latest}", installed.version), row),
            fg: 0xfcd34d,
            bg: 0x3a2a0b,
            button: Some((tr("Actualizar", "Update"), Action::Install)),
        },
        (Some(installed), Some(_)) => Status {
            label: tr("Al día", "Up to date"),
            detail: with_size(format!("{} {}", tr("Versión", "Version"), installed.version), row),
            fg: 0x6ee7b7,
            bg: 0x0f2e26,
            button: Some((tr("Abrir", "Open"), Action::Open)),
        },
        _ => Status { label: tr("Sin datos", "No data"), detail: String::new(), fg: 0x94a3b8, bg: 0x1e293b, button: None },
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
            let installer = github::latest_installer(app)?;
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

/// A square tile with the app's first letter, in the app's accent color.
fn monogram(name: &str, color: u32, dim: f32) -> Div {
    let letter = name.chars().next().unwrap_or('?').to_uppercase().to_string();
    div()
        .size(px(dim))
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .bg(rgb(color))
        .text_color(rgb(0xffffff))
        .font_weight(FontWeight::BOLD)
        .child(letter)
}

fn chip(label: &'static str, fg: u32, bg: u32) -> Div {
    div()
        .px_2()
        .py_1()
        .rounded_full()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .bg(rgb(bg))
        .text_color(rgb(fg))
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
    autostart: bool,
    /// Offer alpha builds (pre-releases) as well as stable ones.
    alpha: bool,
    /// Install releases by themselves once they have waited out the test period.
    auto_update: bool,
    /// The app whose uninstall waits for its second click.
    confirming: Option<String>,
    /// The install in progress: its app and the live download state.
    transfer: Option<(String, Arc<Progress>)>,
    screen: Screen,
    /// Lists the apps with updates first, then the installed ones; by name otherwise.
    sort_by_status: bool,
    /// Text typed on the apps screen; shows only the apps whose name contains it.
    search: String,
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
            autostart: shell::autostart_on(),
            alpha: shell::alpha_channel_on(),
            auto_update: shell::auto_update_on(),
            confirming: None,
            transfer: None,
            screen: Screen::Apps,
            sort_by_status: false,
            search: String::new(),
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
    fn queue_panel(&self, cx: &mut Context<Self>) -> Div {
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
                            .bg(rgb(0x1e2b4a))
                            .cursor_pointer()
                            .child("↑")
                            .on_click(cx.listener(move |this, _, _, cx| this.move_in_queue(ix, true, cx))),
                    )
                    .child(
                        div()
                            .id(("queue-down", ix))
                            .px_2()
                            .rounded_md()
                            .bg(rgb(0x1e2b4a))
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
            .bg(rgb(0x0e1a2e))
            .border_1()
            .border_color(rgb(0x1c2c47))
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
                            .bg(rgb(0x1e2b4a))
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
            self.message = tr("Sin icono de bandeja, cerrar siempre sale de la app", "Without a tray icon, closing always quits the app").into();
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
                hub.rows = rows;
                hub.self_update = update;
                hub.busy = false;
                hub.message = String::new();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn act(&mut self, name: String, action: Action, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
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
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { run(action, &name, &progress) })
                .await;
            this.update(cx, |hub, cx| {
                hub.busy = false;
                hub.transfer = None;
                match result {
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

    /// First click asks for confirmation; the second one uninstalls.
    fn uninstall_clicked(&mut self, name: String, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.confirming.as_deref() != Some(name.as_str()) {
            self.confirming = Some(name);
        } else {
            self.confirming = None;
            self.act(name, Action::Uninstall, cx);
        }
        cx.notify();
    }

    fn nav(&self, cx: &mut Context<Self>, filter: Filter) -> impl IntoElement {
        let active = self.filter == filter;
        let count = self.rows.iter().filter(|row| filter.matches(row)).count();
        div()
            .id(("nav", filter as usize))
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(if active { 0x1c2c4a } else { 0x0e1a2e }))
            .text_color(rgb(if active { 0xffffff } else { 0x94a3b8 }))
            .hover(|style| style.bg(rgb(0x18263f)))
            .cursor_pointer()
            .child(filter.title())
            .child(div().text_xs().child(count.to_string()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.filter = filter;
                this.screen = Screen::Apps;
                cx.notify();
            }))
    }

    fn card(&self, ix: usize, row: &Row, cx: &mut Context<Self>) -> impl IntoElement {
        let st = status(row);
        let description = registry::find(&row.name).map_or("", |app| app.description());
        let mut card = div()
            .w(px(268.))
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_xl()
            .bg(rgb(0x142038))
            .border_1()
            .border_color(rgb(0x23345a))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(monogram(&row.name, row.accent, 44.))
                    .child(chip(st.label, st.fg, st.bg)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(row.name.clone()))
                    .child(div().text_xs().text_color(rgb(0x94a3b8)).child(description)),
            )
            .child(div().text_xs().text_color(rgb(0x7c8db0)).child(st.detail));
        let notes = excerpt(&row.notes);
        if !notes.is_empty() && (row.installed.is_none() || has_update(row)) {
            card = card.child(div().text_xs().text_color(rgb(0x94a3b8)).child(notes));
        }
        let alpha_label = if self.alpha {
            tr("Alfa: incluida por el ajuste general", "Alpha: included by the general setting")
        } else if row.alpha_app {
            tr("Alfa: sí · toca para quitar", "Alpha: on · click to turn off")
        } else {
            tr("Alfa: no · toca para incluir", "Alpha: off · click to include")
        };
        let mut alpha_line = div().id(("alpha-app", ix)).text_xs().text_color(rgb(0x7c8db0)).child(alpha_label);
        if !self.alpha {
            let name = row.name.clone();
            alpha_line = alpha_line
                .cursor_pointer()
                .hover(|style| style.text_color(rgb(0xcbd5e1)))
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_app_alpha(&name, cx)));
        }
        card = card.child(alpha_line);
        if let Some((_, progress)) = self.transfer.as_ref().filter(|(name, _)| *name == row.name) {
            let done = progress.done.load(Ordering::SeqCst);
            let total = progress.total.load(Ordering::SeqCst);
            let fraction = if total > 0 { (done as f32 / total as f32).min(1.0) } else { 0.0 };
            let label = if total > 0 {
                format!("{} {}%", tr("Descargando", "Downloading"), (fraction * 100.0) as u32)
            } else {
                tr("Descargando…", "Downloading…").to_string()
            };
            let cancel = progress.clone();
            card = card.child(
                div()
                    .mt_auto()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().text_color(rgb(0xcbd5e1)).child(label))
                    .child(
                        div()
                            .h(px(6.))
                            .w_full()
                            .rounded_full()
                            .bg(rgb(0x1e2b4a))
                            .child(div().h_full().rounded_full().bg(rgb(row.accent)).w(relative(fraction))),
                    )
                    .child(
                        div()
                            .id(("cancel", ix))
                            .text_xs()
                            .text_center()
                            .text_color(rgb(0x7c8db0))
                            .cursor_pointer()
                            .hover(|style| style.text_color(rgb(0xfca5a5)))
                            .child(tr("Cancelar", "Cancel"))
                            .on_click(cx.listener(move |_, _, _, _| cancel.cancel.store(true, Ordering::SeqCst))),
                    ),
            );
        } else if let Some((label, action)) = st.button {
            let name = row.name.clone();
            let bg = match action {
                Action::Install => row.accent,
                Action::Open | Action::Uninstall | Action::Rollback => 0x1e2b4a,
            };
            card = card.child(
                div()
                    .id(("action", ix))
                    .mt_auto()
                    .py_2()
                    .rounded_lg()
                    .bg(rgb(bg))
                    .text_color(rgb(0xffffff))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_center()
                    .cursor_pointer()
                    .hover(|style| style.opacity(0.9))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| this.act(name.clone(), action, cx))),
            );
        }
        if row.error.is_some() {
            card = card.child(
                div()
                    .id(("retry", ix))
                    .mt_auto()
                    .py_2()
                    .rounded_lg()
                    .bg(rgb(0x1e2b4a))
                    .text_color(rgb(0xffffff))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_center()
                    .cursor_pointer()
                    .hover(|style| style.opacity(0.9))
                    .child(tr("Reintentar", "Try again"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.busy {
                            this.refresh(cx);
                        }
                    })),
            );
        }
        if row.installed.is_some() {
            let name = row.name.clone();
            let armed = self.confirming.as_deref() == Some(row.name.as_str());
            card = card.child(
                div()
                    .id(("uninstall", ix))
                    .text_xs()
                    .text_center()
                    .text_color(rgb(if armed { 0xfca5a5 } else { 0x7c8db0 }))
                    .cursor_pointer()
                    .hover(|style| style.text_color(rgb(0xfca5a5)))
                    .child(if armed {
                        tr("Confirmar desinstalar", "Confirm uninstall")
                    } else {
                        tr("Desinstalar", "Uninstall")
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.uninstall_clicked(name.clone(), cx))),
            );
        }
        if let Some(previous) = row.installed.as_ref().and_then(|installed| installed.previous.clone()) {
            let name = row.name.clone();
            card = card.child(
                div()
                    .id(("rollback", ix))
                    .text_xs()
                    .text_center()
                    .text_color(rgb(0x7c8db0))
                    .cursor_pointer()
                    .hover(|style| style.text_color(rgb(0xe2e8f0)))
                    .child(format!("{} {previous}", tr("Volver a", "Go back to")))
                    .on_click(cx.listener(move |this, _, _, cx| this.act(name.clone(), Action::Rollback, cx))),
            );
        }
        card
    }
}

impl Render for Hub {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Keys go to the window's focus so the search box receives what is typed.
        if !self.focus.is_focused(window) {
            window.focus(&self.focus);
        }
        let needle = self.search.to_lowercase();
        let mut visible: Vec<&Row> = self
            .rows
            .iter()
            .filter(|row| self.filter.matches(row) && row.name.to_lowercase().contains(&needle))
            .collect();
        let by_status = self.sort_by_status;
        visible.sort_by(|a, b| {
            let rank = |row: &Row| if has_update(row) { 0 } else if row.installed.is_some() { 1 } else { 2 };
            let by_name = a.name.cmp(&b.name);
            if by_status { rank(a).cmp(&rank(b)).then(by_name) } else { by_name }
        });
        let count = visible.len();
        let cards: Vec<_> = visible
            .into_iter()
            .enumerate()
            .map(|(ix, row)| self.card(ix, row, cx))
            .collect();

        let sidebar = div()
            .flex()
            .flex_col()
            .w(px(208.))
            .h_full()
            .p_4()
            .gap_1()
            .bg(rgb(0x0e1a2e))
            .border_r_1()
            .border_color(rgb(0x1c2c47))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .mb_6()
                    .child(monogram("MaxApps Hub", 0x6366f1, 36.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().font_weight(FontWeight::BOLD).child("MaxApps Hub"))
                            .child(div().text_xs().text_color(rgb(0x7c8db0)).child(tr("Escritorio", "Desktop"))),
                    ),
            )
            .child(self.nav(cx, Filter::All))
            .child(self.nav(cx, Filter::Installed))
            .child(self.nav(cx, Filter::Updates))
            .child(div().flex_1())
            .children(self.self_update.as_ref().map(|update| {
                div()
                    .id("self-update")
                    .px_3()
                    .py_2()
                    .mb_2()
                    .rounded_md()
                    .bg(rgb(0x6366f1))
                    .text_color(rgb(0xffffff))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_center()
                    .cursor_pointer()
                    .hover(|style| style.opacity(0.9))
                    .child(format!("{} v{}", tr("Actualizar a", "Update to"), update.version))
                    .on_click(cx.listener(|this, _, _, cx| this.update_self(cx)))
            }))
            .child(
                div()
                    .id("settings")
                    .px_3()
                    .py_2()
                    .mb_2()
                    .rounded_md()
                    .text_color(rgb(if self.screen == Screen::Settings { 0xffffff } else { 0x94a3b8 }))
                    .bg(rgb(if self.screen == Screen::Settings { 0x1c2c4a } else { 0x0e1a2e }))
                    .hover(|style| style.bg(rgb(0x18263f)))
                    .cursor_pointer()
                    .child(tr("Ajustes", "Settings"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.screen = Screen::Settings;
                        cx.notify();
                    })),
            )
            .children({
                let pending = self.rows.iter().filter(|row| has_update(row)).count();
                (pending > 0).then(|| {
                    div()
                        .id("update-all")
                        .px_3()
                        .py_2()
                        .mb_2()
                        .rounded_md()
                        .bg(rgb(0x1e2b4a))
                        .text_center()
                        .cursor_pointer()
                        .hover(|style| style.opacity(0.9))
                        .child(format!("{} ({pending})", tr("Actualizar todo", "Update all")))
                        .on_click(cx.listener(|this, _, _, cx| this.update_all(cx)))
                })
            })
            .child(
                div()
                    .id("refresh")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x2a3b5f))
                    .text_center()
                    .cursor_pointer()
                    .child(tr("Buscar actualizaciones", "Check for updates"))
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            );

        let grid = if count == 0 {
            div()
                .text_color(rgb(0x7c8db0))
                .child(tr("Nada que mostrar en esta vista.", "Nothing to show in this view."))
                .into_any_element()
        } else {
            div().flex().flex_wrap().gap_4().children(cards).into_any_element()
        };

        let main = div()
            .flex()
            .flex_col()
            .flex_1()
            .p_6()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .mb_5()
                    .child(div().text_2xl().font_weight(FontWeight::BOLD).child(self.filter.title()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x7c8db0))
                            .child(format!("{count} {}", tr("en esta vista", "in this view"))),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .mt_2()
                            .child(
                                sort_button("sort-name", tr("Por nombre", "By name"), !self.sort_by_status).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.sort_by_status = false;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                sort_button("sort-status", tr("Por estado", "By status"), self.sort_by_status).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.sort_by_status = true;
                                        cx.notify();
                                    }),
                                ),
                            ),
                    ),
            )
            .child(self.search_box())
            .child(self.queue_panel(cx))
            .child(grid)
            .child(div().mt_auto().pt_4().text_sm().text_color(rgb(0x94a3b8)).child(self.message.clone()));

        let panel = if self.screen == Screen::Settings {
            self.settings_panel(cx).into_any_element()
        } else {
            main.into_any_element()
        };

        div()
            .flex()
            .size_full()
            .bg(rgb(0x0b1424))
            .text_color(rgb(0xe2e8f0))
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
            }))
            .child(sidebar)
            .child(panel)
    }
}

impl Hub {
    /// The search box on the apps screen. It takes what is typed; Escape clears it.
    fn search_box(&self) -> Stateful<Div> {
        let text = if self.search.is_empty() {
            div().text_color(rgb(0x7c8db0)).child(tr("Buscar por nombre · Esc borra", "Search by name · Esc clears"))
        } else {
            div().child(self.search.clone())
        };
        div()
            .id("search")
            .px_3()
            .py_2()
            .mb_4()
            .min_w(px(260.))
            .rounded_md()
            .bg(rgb(0x0e1a2e))
            .border_1()
            .border_color(rgb(0x2a3b5f))
            .child(text)
    }
}

/// A small switch between two ways of listing the apps; the active one is lit.
fn sort_button(id: &'static str, label: &'static str, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .text_xs()
        .cursor_pointer()
        .text_color(rgb(if active { 0xffffff } else { 0x94a3b8 }))
        .bg(rgb(if active { 0x6366f1 } else { 0x1e2b4a }))
        .child(label)
}

/// A setting as a card: its name and what it does, and its current value as a chip.
fn setting_row(name: &'static str, detail: &'static str, value: &'static str, on: bool) -> Div {
    div()
        .flex()
        .justify_between()
        .items_center()
        .gap_4()
        .p_3()
        .rounded_lg()
        .bg(rgb(0x142038))
        .border_1()
        .border_color(rgb(0x23345a))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .min_w_0()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                .child(div().text_xs().text_color(rgb(0x7c8db0)).child(detail)),
        )
        .child(if on { chip(value, 0x6ee7b7, 0x0f2e26) } else { chip(value, 0xcbd5e1, 0x1e293b) })
}

/// A language choice: a button that is filled when it is the current language.
fn language_button(id: &'static str, label: &'static str, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .px_4()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .font_weight(FontWeight::SEMIBOLD)
        .bg(rgb(if active { 0x6366f1 } else { 0x1e2b4a }))
        .text_color(rgb(0xffffff))
        .hover(|style| style.opacity(0.9))
        .child(label)
}

impl Hub {
    fn settings_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let english = i18n::english();
        let yes_no = |on: bool| if on { tr("Sí", "On") } else { tr("No", "Off") };
        let close = if self.tray.is_none() {
            tr("Salir (sin bandeja)", "Quit (no tray)")
        } else if self.close_to_tray.load(Ordering::SeqCst) {
            tr("Seguir en la bandeja", "Keep in tray")
        } else {
            tr("Salir del todo", "Quit")
        };
        // Scrolls when the window is shorter than the list of settings.
        div()
            .id("settings-panel")
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .flex_1()
            .p_6()
            .gap_3()
            .child(div().text_2xl().font_weight(FontWeight::BOLD).mb_2().child(tr("Ajustes", "Settings")))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(0x142038))
                    .border_1()
                    .border_color(rgb(0x23345a))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(tr("Idioma", "Language")))
                    .child(div().text_xs().text_color(rgb(0x7c8db0)).child(tr(
                        "Cambia los textos de la app. El menú de la bandeja se actualiza al momento.",
                        "Changes the app's text. The tray menu updates right away.",
                    )))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .mt_1()
                            .child(language_button("lang-es", "Español", !english).on_click(cx.listener(
                                |this, _, _, cx| this.set_language(false, cx),
                            )))
                            .child(language_button("lang-en", "English", english).on_click(cx.listener(
                                |this, _, _, cx| this.set_language(true, cx),
                            ))),
                    ),
            )
            .child(
                setting_row(
                    tr("Iniciar con Windows", "Start with Windows"),
                    tr("Arranca en la bandeja al iniciar sesión en Windows.", "Starts in the tray when you sign in to Windows."),
                    yes_no(self.autostart),
                    self.autostart,
                )
                .id("set-autostart")
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0x18263f)))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_autostart(cx))),
            )
            .child(
                setting_row(
                    tr("Al cerrar la ventana", "When the window is closed"),
                    tr(
                        "Seguir en la bandeja deja la app abierta; salir del todo la cierra.",
                        "Keep in tray leaves the app running; quit closes it.",
                    ),
                    close,
                    self.tray.is_some() && self.close_to_tray.load(Ordering::SeqCst),
                )
                .id("set-close")
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0x18263f)))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_close_mode(cx))),
            )
            .child(
                setting_row(
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
                .hover(|style| style.bg(rgb(0x18263f)))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_alpha(cx))),
            )
            .child(
                setting_row(
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
                .hover(|style| style.bg(rgb(0x18263f)))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_auto_update(cx))),
            )
            .child(
                setting_row(
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
                .hover(|style| style.bg(rgb(0x18263f)))
                .on_click(cx.listener(|this, _, _, cx| this.open_log(cx))),
            )
            .child(div().mt_auto().text_xs().text_color(rgb(0x7c8db0)).child(format!(
                "MaxApps Hub {} · {}",
                env!("CARGO_PKG_VERSION"),
                if english { "English" } else { "Español" }
            )))
            .child(div().text_sm().text_color(rgb(0x94a3b8)).child(self.message.clone()))
    }
}

fn main() {
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
        .expect("no se pudo abrir MaxApps Hub");
        if !background {
            cx.activate(true);
        }
    });
}
